#![feature(seek_stream_len)]
extern crate crossbeam_channel;
use async_std::task;
use common::{Item, Result};
use crossbeam_channel::{unbounded, Sender};
use db::Container;
use output::output_write;
use serde_json::json;
use std::collections::hash_map::DefaultHasher;
use std::collections::HashMap;
use std::fs::File;
use std::hash::{Hash, Hasher};
use std::io::{BufRead, BufReader, Seek, SeekFrom};
use std::sync::{Arc, RwLock};

#[derive(Debug)]
pub enum SendFileEvent {
    Close,
    Other,
}

#[derive(Clone)]
pub struct FileReaderWriter {
    file_handles: Arc<Vec<RwLock<HashMap<String, Sender<SendFileEvent>>>>>,
}

impl FileReaderWriter {
    pub fn new(num_workers: usize) -> Self {
        let mut file_handles: Vec<RwLock<HashMap<String, Sender<SendFileEvent>>>> =
            Vec::with_capacity(num_workers);
        for _i in 0..num_workers {
            file_handles.push(RwLock::new(HashMap::new()))
        }
        Self {
            file_handles: Arc::new(file_handles),
        }
    }

    fn hash(&self, k: &str) -> usize {
        let mut hasher = DefaultHasher::new();
        k.to_owned().hash(&mut hasher);
        hasher.finish() as usize
    }

    fn remove(&self, k: &str) {
        let cache = self.file_handles.clone();
        let writer = cache[(self.hash(k) % cache.len()) as usize].write();
        match writer {
            Ok(mut w) => {
                w.remove(k);
            }
            Err(e) => {
                eprintln!("[ERROR] cache remove failed: {:?}", e)
            }
        }
    }

    fn get(&self, k: &str) -> Option<Sender<SendFileEvent>> {
        let cache = self.file_handles.clone();
        let reader = cache[(self.hash(k) % cache.len()) as usize].write();
        match reader {
            Ok(r) => match r.get(k) {
                Some(v) => Some(v.clone()),
                None => None,
            },
            Err(e) => {
                eprintln!("[ERROR] cache get key {:?} failed: {:?}", k, e);
                None
            }
        }
    }

    fn contains_key(&self, k: &str) -> bool {
        let cache = self.file_handles.clone();
        let reader = cache[(self.hash(k) % cache.len()) as usize].write();
        match reader {
            Ok(r) => r.contains_key(k),
            Err(e) => {
                eprintln!("[ERROR] cache get key {:?} failed: {:?}", k, e);
                false
            }
        }
    }

    fn registry(&self, k: &str, v: Sender<SendFileEvent>) {
        let writer = self.file_handles[(self.hash(k) % self.file_handles.len()) as usize].write();
        match writer {
            Ok(mut w) => {
                w.insert(k.to_string(), v);
            }
            Err(e) => {
                eprintln!("[ERROR] cache insert failed: {:?}", e)
            }
        }
    }

    pub fn close_event(&self, path: &str) {
        if let Some(tx) = self.get(path) {
            if let Err(e) = tx.send(SendFileEvent::Close) {
                eprintln!("[ERROR] frw send close to {:?} handle error: {:?}", path, e);
            }
            self.remove(path);
        }
    }

    pub fn remove_event(&self, path: &str) {
        if let Some(tx) = self.get(path) {
            if let Err(e) = tx.send(SendFileEvent::Close) {
                eprintln!("[ERROR] frw send remove to {:?} handle error: {:?}", path, e);
            }
            self.remove(path);
        };

        db::delete(path);
    }

    fn send_write_event(&self, path: &str) -> Result<()> {
        if let Some(handle) = self.get(path) {
            handle.send(SendFileEvent::Other)?;
        }
        Ok(())
    }

    pub fn write_event(&self, path: &str) {
        if let Err(e) = self.send_write_event(path) {
            eprintln!("[ERROR] frw send write event error: {:?}", e)
        }
    }

    fn _file_size(path: &str) -> i64 {
        let mut file = match File::open(&path) {
            Ok(file) => file,
            Err(e) => {
                eprintln!("[ERROR] frw open file {:?} error: {:?}", path, e);
                return -1;
            }
        };
        file.stream_len().unwrap() as i64
    }

    fn open_seek_buffer(path: &str, offset: i64) -> Result<BufReader<File>> {
        let mut file = match File::open(path) {
            Ok(file) => file,
            Err(e) => {
                return Err(Box::new(e));
            }
        };

        if let Err(e) = file.seek(SeekFrom::Current(offset)) {
            return Err(Box::new(e));
        }

        Ok(BufReader::new(file))
    }

    async fn read_fn(
        br: &mut BufReader<File>,
        bf: &mut String,
        offset: &mut i64,
        container: &Container,
    ) {
        while let Ok(line_size) = br.read_line(bf) {
            if line_size == 0 {
                break;
            }
            output_write(&container.output, &encode_message(&container, bf.as_str()));
            db::incr_offset(&container.path, line_size as i64);
            bf.clear();

            *offset += line_size as i64;
        }
    }

    pub fn open_event(&self, container: &mut Container) {
        if self.contains_key(&container.path) {
            return;
        }

        let container_clone = container.clone();
        let mut offset = container.offset;

        let (tx, rx) = unbounded::<SendFileEvent>();
        task::spawn(async move {
            let mut bf = String::new();
            let mut br = match Self::open_seek_buffer(&container_clone.path, container_clone.offset) {
                Ok(it) => it,
                Err(e) => {
                    eprintln!("{:?}", e);
                    return;
                }
            };

            while let Ok(evt) = rx.recv() {
                if let SendFileEvent::Close = evt {
                    break;
                } else {
                    Self::read_fn(&mut br, &mut bf, &mut offset, &container_clone).await
                }
            }
        });

        self.registry(&container.path, tx);

        db::update(&container.set_state_run());
        if let Err(e) = self.send_write_event(&container.path) {
            eprintln!("[ERROR] frw send first event error: {:?}", e);
        }
    }
}

fn encode_message<'a>(container: &'a Container, message: &'a str) -> String {
    if message.len() == 0 {
        return "".to_string();
    }
    let message_item = Item::from(message);
    if message_item.is_json() {
        json!({
            "custom":
                {
                  "nodeId":container.pod_name,
                  "container":container.container,
                  "serviceName":container.service_name,
                  "ips":container.ips,
                  "ns":container.ns,
                  "version":"v1.0.0",
                },
            "message":message_item.get_key("log")}
        )
        .to_string()
    } else {
        json!({
            "custom":
                {
                  "nodeId":container.pod_name,
                  "container":container.container,
                  "serviceName":container.service_name,
                  "ips":container.ips,
                  "ns":container.ns,
                  "version":"v1.0.0",
                },
            "message":message_item.string()}
        )
        .to_string()
    }
}

#[cfg(test)]
mod tests {
    use crate::FileReaderWriter;
    use db::Container;

    #[test]
    fn it_works() {
        let input = FileReaderWriter::new(10);
        input.open_event(&mut Container::default());
    }
}

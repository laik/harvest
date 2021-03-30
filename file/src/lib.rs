#![feature(seek_stream_len)]
extern crate crossbeam_channel;
use async_std::task;
use common::{Item, Result};
use crossbeam_channel::{unbounded, Sender};
use db::Pod;
use output::output_write;
use serde_json::json;
use std::io::{BufRead, BufReader, Seek, SeekFrom};
use std::{
    collections::hash_map::DefaultHasher,
    fs::File,
    hash::{Hash, Hasher},
};
use std::{
    collections::HashMap,
    sync::{Arc, RwLock},
};

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
                eprintln!("cache remove failed: {:?}", e)
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
                eprintln!("cache get key {:?} failed: {:?}", k, e);
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
                eprintln!("cache get key {:?} failed: {:?}", k, e);
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
                eprintln!("cache insert failed: {:?}", e)
            }
        }
    }

    pub fn close_event(&self, path: &str) {
        if let Some(tx) = self.get(path) {
            if let Err(e) = tx.send(SendFileEvent::Close) {
                eprintln!("frw send close to {:?} handle error: {:?}", path, e);
            }
            self.remove(path);
        }
    }

    pub fn remove_event(&self, path: &str) {
        if let Some(tx) = self.get(path) {
            if let Err(e) = tx.send(SendFileEvent::Close) {
                eprintln!("frw send remove to {:?} handle error: {:?}", path, e);
            }
            self.remove(path);
        };

        db::delete(path);
    }

    fn send_write_event(&self, path: &str) -> Result<()> {
        if let Some(handle) = self.get(path) {
            handle.send(SendFileEvent::Other)?;
        } else {
            println!(
                "[INFO] not found path: {:?} on handles {:?}",
                path, self.file_handles
            );
        }
        Ok(())
    }

    pub fn write_event(&self, path: &str) {
        println!("recv write event {:?}", path);
        if let Err(e) = self.send_write_event(path) {
            eprintln!("frw send write event error: {:?}", e)
        }
    }

    fn file_size(path: &str) -> i64 {
        let mut file = match File::open(&path) {
            Ok(file) => file,
            Err(e) => {
                eprintln!("frw open file {:?} error: {:?}", path, e);
                return -1;
            }
        };
        file.stream_len().unwrap() as i64
    }

    fn open_seek_bufr(path: &str, offset: i64) -> Result<BufReader<File>> {
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
        is_until_align_offset: &mut bool,
        pod: &Pod,
    ) {
        while let Ok(line_size) = br.read_line(bf) {
            if line_size == 0 {
                break;
            }
            output_write(&pod.output, &encode_message(&pod, bf.as_str()));
            db::incr_offset(&pod.path, line_size as i64);
            bf.clear();

            *offset += line_size as i64;

            if !(*is_until_align_offset) {
                if offset >= &mut (Self::file_size(&pod.path) as i64) {
                    (*is_until_align_offset) = true;
                }
            }
            // println!(
            //     "is_until_align_offset {:?}, offset {:?}, filesize {:?}",
            //     is_until_align_offset,
            //     offset,
            //     Self::file_size(&pod.path) as i64
            // );
            if *is_until_align_offset {
                break;
            }
        }
    }

    pub fn open_event(&self, pod: &mut Pod) {
        if self.contains_key(&pod.path) {
            return;
        }

        let pod_clone = pod.clone();
        let mut offset = pod.offset;

        let (tx, rx) = unbounded::<SendFileEvent>();
        task::spawn(async move {
            let mut bf = String::new();
            let mut is_until_align_offset = false;

            let mut br = match Self::open_seek_bufr(&pod_clone.path, pod_clone.offset) {
                Ok(it) => it,
                Err(e) => {
                    eprintln!("{:?}", e);
                    return;
                }
            };
            while let Ok(evt) = rx.recv() {
                if let SendFileEvent::Close = evt {
                    break;
                }
                Self::read_fn(
                    &mut br,
                    &mut bf,
                    &mut offset,
                    &mut is_until_align_offset,
                    &pod_clone,
                )
                .await
            }
        });

        self.registry(&pod.path, tx);

        db::update(&pod.set_state_run());
    }
}

fn encode_message<'a>(pod: &'a Pod, message: &'a str) -> String {
    if message.len() == 0 {
        return "".to_string();
    }
    let message_item = Item::from(message);
    if message_item.is_json() {
        json!({
            "custom":
                {
                  "nodeId":pod.pod_name,
                  "container":pod.container,
                  "serviceName":pod.service_name,
                  "ips":pod.ips,
                  "version":"v1.0.0",
                },
            "message":message_item.get_key("log")}
        )
        .to_string()
    } else {
        json!({
            "custom":
                {
                  "nodeId":pod.pod_name,
                  "container":pod.container,
                  "serviceName":pod.service_name,
                  "ips":pod.ips,
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
    use db::Pod;

    #[test]
    fn it_works() {
        let input = FileReaderWriter::new(10);
        input.open_event(&mut Pod::default());
    }
}

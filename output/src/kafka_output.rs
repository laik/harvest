use super::{IOutput, Item, Result};
use async_std::task;
use kafka::producer::{Producer, Record, RequiredAcks};
// use ringbuf::{Consumer as RConsumer, Producer as RProducer, RingBuffer};
use crossbeam_channel::{bounded, Receiver, Sender};

use common::{retry_fn, retry_fn_mut};
use std::{
    collections::HashMap,
    sync::{atomic::Ordering, Arc},
    thread,
    time::Duration,
};
use std::{sync::atomic::AtomicUsize, time::Instant};

#[derive(Clone, Debug)]
struct KafkaOutputConfig {
    broker: Vec<String>,
    topic: String,
}

struct Count(AtomicUsize);

impl Count {
    pub fn new() -> Count {
        return Count(AtomicUsize::new(0));
    }

    pub fn increase(&self) {
        self.0.fetch_add(1, Ordering::SeqCst);
    }

    pub fn value(&self) -> usize {
        self.0.load(Ordering::SeqCst) as usize
    }
}

impl Clone for Count {
    fn clone(&self) -> Self {
        Count(AtomicUsize::new(self.0.load(Ordering::SeqCst)))
    }
}

pub(crate) struct KafkaOuput {
    channels: HashMap<String, Sender<Item>>,
    buffer_size: usize,
    count: Count,
    current: Arc<Count>,
}

impl KafkaOuput {
    pub fn new(buffer_size: usize) -> KafkaOuput {
        Self {
            channels: HashMap::new(),
            buffer_size: buffer_size,
            count: Count::new(),
            current: Arc::new(Count::new()),
        }
    }

    // channel =  kafka:topic@10.200.100.200:9092,10.200.100.201:9092
    fn parse_uri_to_producer(&self, channel: &str) -> KafkaOutputConfig {
        let type_topic_ips = channel.split("@").collect::<Vec<&str>>();
        let broker = type_topic_ips[1]
            .split(",")
            .map(|k| k.to_string())
            .collect::<Vec<String>>();
        let topic = type_topic_ips[0].split(":").collect::<Vec<&str>>()[1];
        KafkaOutputConfig {
            broker: broker,
            topic: topic.to_string(),
        }
    }

    fn write_in(&mut self, channel: &str, item: &Item) {
        let prod = self.channels.get_mut(channel).unwrap();
        let write = || -> bool {
            if prod.is_full() {
                return false;
            }
            match prod.send(item.clone()) {
                Ok(_) => true,
                Err(_) => false,
            }
        };
        retry_fn_mut(write, Duration::from_millis(1))
    }

    async fn write_out(
        topic: &str,
        cons: Receiver<Item>,
        kp: &mut Producer,
        buffer_size: usize,
        current: Arc<Count>,
    ) {
        let mut index = 0;
        let mut now = Instant::now();
        let mut write_buffer = Vec::with_capacity(buffer_size);
        loop {
            if cons.is_empty() || now.elapsed().as_secs() < 5 {
                thread::sleep(Duration::from_millis(1));
                continue;
            }

            if let Ok(it) = cons.recv() {
                write_buffer.push(Record::from_key_value(
                    topic,
                    format!("{:?}", index),
                    it.string(),
                ));

                index += 1;
                current.increase();
            }

            if index >= write_buffer.capacity() || now.elapsed().as_secs() > 5 {
                match kp.send_all(&write_buffer) {
                    Ok(_) => {
                        index = 0;
                        now = Instant::now();
                        write_buffer.clear();
                    }
                    Err(e) => {
                        eprintln!("{:?}", e);
                        continue;
                    }
                }
            }
        }
    }

    fn not_exist_create(&mut self, channel: &str) -> Result<()> {
        let cfg = self.parse_uri_to_producer(channel);
        let mut kp = match Producer::from_hosts(cfg.broker)
            .with_ack_timeout(Duration::from_secs(1))
            .with_required_acks(RequiredAcks::One)
            .create()
        {
            Ok(it) => it,
            Err(e) => return Err(Box::new(e)),
        };

        let buffer_size = self.buffer_size.clone();
        let (s, r) = bounded(buffer_size);

        let topic = cfg.topic.clone();
        let current_count = Arc::clone(&self.current);

        task::spawn(async move {
            Self::write_out(&topic, r, &mut kp, buffer_size, current_count).await;
        });

        self.channels.insert(channel.to_string(), s);

        Ok(())
    }

    fn write_to_channel_queue(&mut self, channel: &str, item: Item) -> Result<()> {
        self.write_in(channel, &item);
        Ok(())
    }
}

impl IOutput for KafkaOuput {
    fn write(&mut self, channel: &str, item: Item) -> Result<()> {
        if !self.channels.contains_key(channel) {
            self.not_exist_create(channel)?;
        }
        self.count.increase();
        self.write_to_channel_queue(channel, item)
    }

    fn wait(&self, _: usize) -> bool {
        retry_fn(
            || -> bool { self.current.value() >= self.count.value() },
            Duration::from_millis(1),
        );
        true
    }
}

#[cfg(test)]
mod tests {
    use super::KafkaOuput;
    use crate::IOutput;
    use common::Item;

    #[test]
    fn kafka_working() {
        //first docker run a kafka
        let mut ko = KafkaOuput::new(10240);
        for _ in 0..1024000 {
            let item = Item::from("................................................................................................................................");
            if let Err(e) = ko.write(&"kafka:test3@10.200.100.200:9092", item) {
                panic!("{:?}", e)
            }
        }
        ko.wait(0);
    }
}

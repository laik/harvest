#[macro_use]
extern crate lazy_static;
mod database;

mod container;
pub use container::{Container, ContainerList, ContainerListMarshaller, GetContainer, State};
use database::Message;
use event::Listener;

pub use common::new_arc_rwlock;
pub use database::Event;
pub(crate) use database::{MemDatabase, MemDatabaseEventDispatcher};

use serde::{Deserialize, Serialize};

lazy_static! {
    static ref MEM: MemDatabase = {
        let m = MemDatabase::new(new_arc_rwlock(MemDatabaseEventDispatcher::new()));
        m
    };
}

pub fn incr_offset(uuid: &str, offset: i64) {
    MEM.tx
        .send(Message {
            event: Event::IncrOffset,
            container: Container {
                path: uuid.to_string(),
                last_offset: offset,
                ..Default::default()
            },
        })
        .unwrap()
}

pub fn update(container: &Container) {
    MEM.tx
        .send(Message {
            event: Event::Update,
            container: container.clone(),
        })
        .unwrap();
}

pub fn insert(container: &Container) {
    MEM.tx
        .send(Message {
            event: Event::Insert,
            container: container.clone(),
        })
        .unwrap();
}

pub fn delete(uuid: &str) {
    MEM.tx
        .send(Message {
            event: Event::Delete,
            container: Container {
                path: uuid.to_string(),
                ..Default::default()
            },
        })
        .unwrap();
}

pub fn all_to_json() -> ContainerListMarshaller {
    ContainerListMarshaller(
        MEM.containers
            .read()
            .unwrap()
            .iter()
            .map(|(_, v)| v.clone())
            .collect::<Vec<Container>>(),
    )
}

pub fn close() {
    MEM.tx
        .send(Message {
            event: Event::Close,
            container: Container {
                ..Default::default()
            },
        })
        .unwrap();
}

pub fn get(uuid: &str) -> Option<Container> {
    match MEM.containers.read() {
        Ok(containers) => match containers.get(uuid) {
            Some(container) => Some(container.clone()),
            None => None,
        },
        Err(_) => None,
    }
}

pub fn get_pod(pod_name: &str) -> Option<Container> {
    if let Some((_, container)) = MEM
        .containers
        .read()
        .unwrap()
        .iter()
        .find(|(_, v)| v.pod_name == pod_name)
    {
        return Some(container.clone());
    } else {
        None
    }
}

pub fn get_container_slice_by_pod(ns: &str, pod_name: &str) -> Vec<(String, Container)> {
    let result = MEM
        .containers
        .read()
        .unwrap()
        .iter()
        .filter(|(_, v)| v.ns == ns && v.pod_name == pod_name)
        .map(|(uuid, container)| (uuid.clone(), container.clone()))
        .collect::<Vec<(String, Container)>>();
    result
}

pub fn delete_with_ns_container(ns: &str, container_name: &str) {
    MEM.tx
        .send(Message {
            event: Event::Delete,
            container: Container {
                ns: ns.to_string(),
                container: container_name.to_string(),
                ..Default::default()
            },
        })
        .unwrap();
}

pub fn container_upload_stop(ns: &str, container_name: &str) {
    let res = get_container_slice_by_pod(ns, container_name);
    for (_, mut container) in res {
        if container.is_stop() {
            continue;
        }
        container.un_upload();
        container.state_stop();
        update(&container);
    }
}

pub fn container_upload_start(ns: &str, container_name: &str) {
    let res = get_container_slice_by_pod(ns, container_name);
    for (_, mut container) in res {
        if container.is_upload() && container.is_running() {
            continue;
        }
        container.upload();
        container.state_running();
        update(&container);
    }
}

pub fn registry_open_event_listener<L>(l: L)
where
    L: Listener<Container> + Send + Sync + 'static,
{
    match MEM.dispatchers.write() {
        Ok(mut dispatcher) => dispatcher.registry_open_event_listener(l),
        Err(e) => {
            eprintln!("{:?}", e)
        }
    }
}

pub fn registry_close_event_listener<L>(l: L)
where
    L: Listener<Container> + Send + Sync + 'static,
{
    match MEM.dispatchers.write() {
        Ok(mut dispatcher) => dispatcher.registry_close_event_listener(l),
        Err(e) => {
            eprintln!("{:?}", e)
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Filter {
    pub max_length: i64,
    pub expr: String,
}

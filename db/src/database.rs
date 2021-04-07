use super::{new_arc_rwlock, Container};
use async_std::task;
use crossbeam_channel::{unbounded, Sender};
use event::{Dispatch, Listener};
use std::sync::RwLock;
use std::{collections::HashMap, sync::Arc};
use strum::AsRefStr;

#[derive(AsRefStr, Debug, Clone)]
pub enum Event {
    #[strum(serialize = "insert")]
    Insert,
    #[strum(serialize = "update")]
    Update,
    #[strum(serialize = "delete")]
    Delete,
    #[strum(serialize = "offset")]
    IncrOffset,
    #[strum(serialize = "close")]
    Close,
}

unsafe impl Sync for Event {}
unsafe impl Send for Event {}

pub(crate) type UUID = String;

#[derive(Debug, Clone)]
pub struct Message {
    pub event: Event,
    pub container: Container,
}

#[derive(AsRefStr, Debug, Clone)]
enum ListenerEvent {
    #[strum(serialize = "open")]
    Open,
    #[strum(serialize = "close")]
    Close,
}

pub struct MemDatabaseEventDispatcher {
    dispatchers: Dispatch<Container>,
}

impl MemDatabaseEventDispatcher {
    pub(crate) fn new() -> Self {
        Self {
            dispatchers: Dispatch::<Container>::new(),
        }
    }

    pub(crate) fn registry_open_event_listener<L>(&mut self, l: L)
    where
        L: Listener<Container> + Send + Sync + 'static,
    {
        self.dispatchers.registry(ListenerEvent::Open.as_ref(), l)
    }

    pub(crate) fn registry_close_event_listener<L>(&mut self, l: L)
    where
        L: Listener<Container> + Send + Sync + 'static,
    {
        self.dispatchers.registry(ListenerEvent::Close.as_ref(), l)
    }

    pub(crate) fn dispatch_open_event(&mut self, container: &Container) {
        self.dispatchers
            .dispatch(ListenerEvent::Open.as_ref(), container)
    }

    pub(crate) fn dispatch_close_event(&mut self, container: &Container) {
        self.dispatchers
            .dispatch(ListenerEvent::Close.as_ref(), container)
    }
}

pub struct MemDatabase {
    // pod key is the pod path uuid
    pub(crate) containers: Arc<RwLock<HashMap<UUID, Container>>>,
    // internal event send queue
    pub(crate) tx: Sender<Message>,
    // db event dispatchers
    pub(crate) dispatchers: Arc<RwLock<MemDatabaseEventDispatcher>>,
}

impl MemDatabase {
    pub fn new(dispatchers: Arc<RwLock<MemDatabaseEventDispatcher>>) -> Self {
        let (tx, rx) = unbounded::<Message>();
        let hm = new_arc_rwlock(HashMap::<UUID, Container>::new());
        let t_hm = Arc::clone(&hm);
        let t_dispatchers = Arc::clone(&dispatchers);
        task::spawn(async move {
            while let Ok(msg) = rx.recv() {
                let evt = msg.event;
                let container = msg.container;

                let mut m = match t_hm.write() {
                    Ok(m) => m,
                    Err(e) => {
                        eprintln!("MemDatabase thread write hm failed, error:{:?}", e);
                        continue;
                    }
                };

                match evt {
                    Event::Update => {
                        m.entry(container.path.to_string())
                            .or_insert(container.clone())
                            .merge_with(&container);
                        match container.state {
                            crate::State::Running => {
                                match t_dispatchers.write() {
                                    Ok(mut dispatch) => dispatch.dispatch_open_event(&container),
                                    Err(e) => {
                                        eprintln!("MemDatabase thread dispath open event failed, error:{:?}",e)
                                    }
                                }
                            }
                            crate::State::Stopped => {
                                match t_dispatchers.write() {
                                    Ok(mut dispatch) => dispatch.dispatch_close_event(&container),
                                    Err(e) => {
                                        eprintln!("MemDatabase thread dispath close event failed, error:{:?}",e)
                                    }
                                }
                            }
                            _ => {}
                        }
                    }
                    Event::Delete => {
                        if container.ns != "" && container.path == "" && container.pod_name != "" {
                            m.retain(|_, inner| !inner.compare_ns_pod(&container));
                            continue;
                        }
                        m.remove(&container.path);
                    }
                    Event::IncrOffset => {
                        if let Some(inner) = m.get_mut(&container.path) {
                            inner.last_offset = container.last_offset;
                            inner.offset += container.last_offset
                        };
                    }

                    Event::Close => {
                        break;
                    }
                    Event::Insert => {
                        m.insert(container.path.clone(), container);
                    }
                };
            }
        });

        Self {
            containers: hm,
            tx,
            dispatchers,
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::Event;
    #[test]
    fn event_it_works() {
        assert_eq!(Event::Insert.as_ref(), "insert");
        assert_eq!(Event::Delete.as_ref(), "delete");
    }
}

#![feature(proc_macro_hygiene, decl_macro)]
#![feature(trait_alias)]
#[macro_use]
extern crate rocket_contrib;
#[macro_use]
extern crate rocket;
#[macro_use]
extern crate lazy_static;

mod api;
mod handle;
mod server;

use db::Container;
use event::{Dispatch, Listener};
pub use serde_json;

pub(crate) use api::*;

pub use common::{new_arc_rwlock, Result};
pub(crate) use handle::{
    DBCloseEvent, DBOpenEvent, ScannerCloseEvent, ScannerCreateEvent, ScannerWriteEvent,
    TaskRunEvent, TaskStopEvent,
};
pub use server::Harvest;

use async_std::task;
use crossbeam_channel::{unbounded, Sender};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use strum::AsRefStr;

type TaskList = Vec<Task>;

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct TaskListMarshaller(TaskList);

impl TaskListMarshaller {
    pub fn to_json(&self) -> String {
        match serde_json::to_string(&self.0) {
            Ok(contents) => contents,
            Err(_) => "".to_owned(),
        }
    }
}

pub(crate) trait GetTask {
    fn get(&self) -> &Task;
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub(crate) struct Task {
    pub(crate) container: Container,
}

impl GetTask for Task {
    fn get(&self) -> &Task {
        self
    }
}

fn to_string_slice(src: &[&str]) -> Vec<String> {
    let mut result = Vec::<String>::new();
    for item in src {
        result.push(item.to_string())
    }
    result
}

impl<'a> From<Cmd<'a>> for Task {
    fn from(cmd: Cmd) -> Self {
        Self {
            container: Container {
                ns: cmd.ns.to_string(),
                node_name: cmd.node_name.to_string(),
                service_name: cmd.service_name.to_string(),
                pod_name: cmd.pod_name.to_string(),
                offset: cmd.offset as i64,
                ips: to_string_slice(&cmd.ips),
                output: cmd.output.to_string(),
                filter: cmd.filter.clone(),
                ..Default::default()
            },
        }
    }
}

impl Default for Task {
    fn default() -> Self {
        Self {
            container: Container::default(),
        }
    }
}

#[derive(Debug)]
enum TaskMessage {
    Run(Task),
    Stop(Task),
    Close,
}
#[derive(AsRefStr, Debug, Clone)]
pub enum TaskStorageListenerEvent {
    #[strum(serialize = "run")]
    RUN,
    #[strum(serialize = "stop")]
    STOP,
}

pub struct TaskStorageEventDispatcher {
    dispatchers: Dispatch<Task>,
}

impl TaskStorageEventDispatcher {
    pub(crate) fn new() -> Self {
        Self {
            dispatchers: Dispatch::<Task>::new(),
        }
    }

    pub(crate) fn registry_run_event_listener<L>(&mut self, l: L)
    where
        L: Listener<Task> + Send + Sync + 'static,
    {
        self.dispatchers
            .registry(TaskStorageListenerEvent::RUN.as_ref(), l)
    }

    pub(crate) fn registry_stop_event_listener<L>(&mut self, l: L)
    where
        L: Listener<Task> + Send + Sync + 'static,
    {
        self.dispatchers
            .registry(TaskStorageListenerEvent::STOP.as_ref(), l)
    }

    pub(crate) fn dispatch_run_event(&mut self, task: &Task) {
        self.dispatchers
            .dispatch(TaskStorageListenerEvent::RUN.as_ref(), task)
    }

    pub(crate) fn dispatch_stop_event(&mut self, task: &Task) {
        self.dispatchers
            .dispatch(TaskStorageListenerEvent::STOP.as_ref(), task)
    }
}

// pub (crate) struct TaskListener
pub(crate) struct TaskStorage {
    data: Arc<RwLock<HashMap<String, Task>>>,
    // internal event send queue
    tx: Sender<TaskMessage>,
    // internal event dispatcher
    dispatchers: Arc<RwLock<TaskStorageEventDispatcher>>,
}

impl TaskStorage {
    pub fn new(dispatchers: Arc<RwLock<TaskStorageEventDispatcher>>) -> Self {
        let data = Arc::new(RwLock::new(HashMap::<String, Task>::new()));
        let (tx, rx) = unbounded::<TaskMessage>();

        let thread_tasks = Arc::clone(&data);
        let t_dispatchers = Arc::clone(&dispatchers);
        task::spawn(async move {
            while let Ok(task_message) = rx.recv() {
                match task_message {
                    TaskMessage::Close => {
                        return;
                    }
                    TaskMessage::Run(mut task) => {
                        let mut tasks = match thread_tasks.write() {
                            Ok(it) => it,
                            Err(e) => {
                                eprintln!("{}", e);
                                continue;
                            }
                        };

                        let pod_slice = db::get_container_slice_by_pod(
                            &task.container.ns,
                            &task.container.pod_name,
                        );

                        for (_, mut container) in pod_slice {
                            container
                                .merge_with(&task.container)
                                .upload()
                                .state_running();

                            task.container = container;
                            tasks
                                .entry(task.container.pod_name.clone())
                                .or_insert(task.clone());
                            match t_dispatchers.write() {
                                Ok(mut dispatch) => dispatch.dispatch_run_event(&task),
                                Err(e) => eprintln!("{}", e),
                            }
                        }
                    }
                    TaskMessage::Stop(mut task) => {
                        let mut tasks = match thread_tasks.write() {
                            Ok(it) => it,
                            Err(e) => {
                                eprintln!("{}", e);
                                continue;
                            }
                        };
                        for (_, mut container) in db::get_container_slice_by_pod(
                            &task.container.ns,
                            &task.container.pod_name,
                        ) {
                            container.un_upload().state_stop();
                            task.container = container;
                            tasks
                                .entry(task.container.pod_name.clone())
                                .or_insert(task.clone());
                            match t_dispatchers.write() {
                                Ok(mut dispatch) => dispatch.dispatch_stop_event(&task),
                                Err(e) => eprintln!("{}", e),
                            }
                        }
                    }
                }
            }
        });
        Self {
            data,
            tx,
            dispatchers,
        }
    }
}

lazy_static! {
    static ref TASKS: TaskStorage = {
        let task_storage = TaskStorage::new(new_arc_rwlock(TaskStorageEventDispatcher::new()));
        task_storage
    };
}

pub(crate) fn run_task(task: Task) {
    TASKS.tx.send(TaskMessage::Run(task)).unwrap();
}

pub(crate) fn stop_task(task: Task) {
    TASKS.tx.send(TaskMessage::Stop(task)).unwrap();
}

pub(crate) fn task_close() {
    TASKS.tx.send(TaskMessage::Close).unwrap();
}

pub(crate) fn tasks_json() -> TaskListMarshaller {
    TaskListMarshaller(tasks())
}

pub(crate) fn tasks() -> TaskList {
    if let Ok(tasks) = TASKS.data.read() {
        return tasks.iter().map(|(_, v)| v.clone()).collect::<Vec<Task>>();
    }
    vec![]
}

pub(crate) fn registry_task_run_event_listener<L>(l: L)
where
    L: Listener<Task> + Send + Sync + 'static,
{
    match TASKS.dispatchers.write() {
        Ok(mut dispatcher) => dispatcher.registry_run_event_listener(l),
        Err(e) => eprintln!("{}", e),
    }
}

pub(crate) fn registry_task_stop_event_listener<L>(l: L)
where
    L: Listener<Task> + Send + Sync + 'static,
{
    match TASKS.dispatchers.write() {
        Ok(mut dispatcher) => dispatcher.registry_stop_event_listener(l),
        Err(e) => eprintln!("{}", e),
    }
}

pub(crate) fn get_pod_task(pod_name: &str) -> Option<Task> {
    match TASKS.data.read() {
        Ok(db) => match db.get(pod_name) {
            Some(task) => Some(task.clone()),
            None => None,
        },
        Err(e) => {
            eprintln!("{}", e);
            None
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        assert_eq!(2 + 2, 4);
    }
}

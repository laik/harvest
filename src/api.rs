use std::{collections::HashMap, time::Duration};

use super::{run_task, stop_task, tasks_json, Task};
use common::retry_fn;
use rocket::get;
use rocket_contrib::json::JsonValue;
use serde::{Deserialize, Serialize};
use sse_client::EventSource;

const RUN: &'static str = "run";
const STOP: &'static str = "stop";
const HELLO: &'static str = "hello";

pub(crate) fn recv_tasks(addr: &str, node_name: &str) {
    retry_fn(
        || -> bool {
            let event_sources = match EventSource::new(addr) {
                Ok(it) => it,
                Err(e) => {
                    eprintln!("{:?}", e);
                    return false;
                }
            };

            for event in event_sources.receiver().iter() {
                let request = match serde_json::from_str::<Cmd>(&event.data) {
                    Ok(it) => it,
                    Err(e) => {
                        eprintln!(
                        "recv event parse json error or connect to api server error: {:?} \n data: {:?}",
                        e, &event.data
                    );
                        continue;
                    }
                };

                if !request.has_node_events(&node_name) {
                    continue;
                }

                output::registry_kafka_output(request.output);

                for task in request.to_pod_tasks() {
                    if request.op == RUN {
                        println!(
                            "[INFO] task recv run task ns:{:?},pod:{:?},ouput:{:?}",
                            &task.container.ns, &task.container.pod_name, &task.container.output
                        );
                        run_task(task);
                    } else if request.op == STOP {
                        println!(
                            "[INFO] task recv run task ns:{:?},pod:{:?},ouput:{:?}",
                            &task.container.ns, &task.container.pod_name, &task.container.output
                        );
                        stop_task(task);
                    } else if request.op == HELLO {
                        println!("[INFO] task recv hello task");
                    } else {
                        println!("[INFO] recv api server unknown event: {:?}", request)
                    }
                }
            }
            false
        },
        Duration::from_secs(1),
    )
}

/*
{
   "op":"run",
   "ns":"default",
   "service_name":"xx_service",
   "filter":{"max_length":1024,"expr":"[INFO]"},
   "output":"fake_output",
   "pods":[
      {
         "node":"node1",
         "pod" :"pod-12345",
         "container":"nginx",
         "ips":[
            "127.0.0.1"
         ],
         "offset":0
      }
   ]
}
*/

#[derive(Serialize, Deserialize, Debug)]
pub(crate) struct Cmd<'a> {
    op: &'a str,
    pub(crate) ns: &'a str,
    pub(crate) output: &'a str,
    pub(crate) filter: HashMap<&'a str, &'a str>,
    pub(crate) service_name: &'a str,
    pub(crate) pods: Vec<Pod<'a>>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub(crate) struct Pod<'a> {
    pub(crate) node: &'a str,
    pub(crate) pod: &'a str,
    pub(crate) container: &'a str,
    pub(crate) ips: Vec<&'a str>,
    pub(crate) offset: i64,
}

fn copy_hash_map<'a>(data: &HashMap<&'a str, &'a str>) -> HashMap<String, String> {
    let mut result = HashMap::new();
    for (key, value) in data {
        result.insert(key.to_string(), value.to_string());
    }
    result
}

impl<'a> Cmd<'a> {
    pub fn to_pod_tasks(&self) -> Vec<Task> {
        self.pods
            .iter()
            .map(|req_pod| {
                let mut task = Task::from(req_pod.clone());
                task.container.ns = self.ns.to_string();
                task.container.output = self.output.to_string();
                task.container.service_name = self.service_name.to_string();
                task.container.filter = copy_hash_map(&self.filter);
                task
            })
            .collect::<Vec<Task>>()
    }
}

impl<'a> Cmd<'a> {
    pub fn has_node_events(&self, node_name: &str) -> bool {
        for pod in self.pods.iter() {
            if pod.node == node_name {
                return true;
            }
        }
        false
    }
}

#[derive(Serialize, Deserialize, Debug)]
pub(crate) struct Request {
    namespace: String,
    pod: String,
    filter: String,
    output: String,
    upload: bool,
}

#[get("/tasks")]
pub(crate) fn query_tasks() -> JsonValue {
    json!(tasks_json())
}

#[get("/pods")]
pub(crate) fn query_all_pod() -> JsonValue {
    json!(db::all_to_json())
}

#[get("/pod/<name>")]
pub(crate) fn query_pod(name: String) -> JsonValue {
    if let Some(pod) = db::get_container(&name) {
        return json!(pod);
    } else {
        json!({})
    }
}

#[catch(404)]
pub(crate) fn not_found() -> JsonValue {
    json!({
        "status": "error",
        "reason": "Resource was not found."
    })
}

#[cfg(test)]
mod tests {
    use super::Cmd;

    #[test]
    fn cmd_it_works() {
        let data = r#"
        {
            "op":"run",
            "ns":"kube-system",
            "service_name":"echoer-api",
            "filter":{
               "max_length":"1024",
               "expr":"[INFO]"
            },
            "output":"fake_output",
            "pods":[
               {
                  "node":"node1",
                  "pod":"echoer-api-86c648d678-z2p9p",
                  "container":"echoer-api-86c648d678-z2p9p",
                  "ips":[
                     "127.0.0.1"
                  ],
                  "offset":0
               }
            ]
         }"#;

        match serde_json::from_str::<Cmd>(data) {
            Ok(it) => it,
            Err(e) => {
                panic!("{:?}", e)
            }
        };
    }
}

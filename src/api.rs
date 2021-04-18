use std::time::Duration;

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
    let uri = addr.to_string() + node_name;
    retry_fn(
        || -> bool {
            let event_sources = match EventSource::new(&uri) {
                Ok(it) => it,
                Err(e) => {
                    eprintln!("[ERROR] create event source failed, error: {:?}", e);
                    return false;
                }
            };

            let mut error_cnt = 0;

            for event in event_sources.receiver().iter() {
                let cmd = match serde_json::from_str::<Cmd>(&event.data) {
                    Ok(it) => it,
                    Err(e) => {
                        eprintln!(
                            "[ERROR] recv cmd from api server error: {:?}, data: {:?}",
                            e, &event.data
                        );
                        error_cnt += 1;
                        if error_cnt >= 5 {
                            return false;
                        }
                        continue;
                    }
                };

                if !cmd.has_node_events(&node_name) {
                    continue;
                }

                output::registry_kafka_output(cmd.output);

                if cmd.op == RUN {
                    println!(
                        "[INFO] task recv run task ns:{:?},pod:{:?},ouput:{:?}",
                        &cmd.ns, &cmd.pod_name, &cmd.output
                    );
                    run_task(Task::from(cmd));
                } else if cmd.op == STOP {
                    println!(
                        "[INFO] task recv run task ns:{:?},pod:{:?},ouput:{:?}",
                        &cmd.ns, &cmd.pod_name, &cmd.output
                    );
                    stop_task(Task::from(cmd));
                } else if cmd.op == HELLO {
                    println!("[INFO] task recv hello !");
                } else {
                    println!("[INFO] recv api server unknown event: {:?}", cmd)
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
    "node":"node1",
    "pod" :"pod-12345",
    "ips":[ "127.0.0.1"],
    "offset":0
}
*/

#[derive(Serialize, Deserialize, Debug)]
pub(crate) struct Cmd<'a> {
    op: &'a str,
    pub(crate) ns: &'a str,
    pub(crate) output: &'a str,
    pub(crate) filter: db::Filter,
    pub(crate) service_name: &'a str,
    pub(crate) node_name: &'a str,
    pub(crate) pod_name: &'a str,
    pub(crate) ips: Vec<&'a str>,
    pub(crate) offset: u64,
}

impl<'a> Cmd<'a> {
    pub fn has_node_events(&self, node_name: &str) -> bool {
        if self.node_name == node_name {
            return true;
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
    if let Some(pod) = db::get_pod(&name) {
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
        let data = r#"{"op":"run","ns":"default","service_name":"xx_service","filter":{"max_length":1024,"expr":"[INFO]"},"output":"fake_output","node_name":"node1","pod_name" :"pod-12345","container":"nginx1","ips":[ "127.0.0.1"],"offset":0}"#;

        match serde_json::from_str::<Cmd>(data) {
            Ok(it) => it,
            Err(e) => {
                panic!("{:?}", e)
            }
        };
    }
}

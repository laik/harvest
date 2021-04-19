use super::Filter;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub enum State {
    Ready,
    Running,
    Stopped,
}
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct Container {
    pub ns: String,
    pub service_name: String,
    pub pod_name: String,
    pub container: String,
    pub path: String, // on this the uuid path is unique identifier
    pub offset: i64,
    pub is_upload: bool,
    pub state: State,
    pub filter: Filter,
    pub output: String,
    pub ips: Vec<String>,
    pub last_offset: i64,
    pub node_name: String,
}

impl Container {
    pub fn state_running(&mut self) -> &mut Self {
        self.state = State::Running;
        self
    }

    pub fn state_ready(&mut self) -> &mut Self {
        self.state = State::Ready;
        self
    }

    pub fn state_stop(&mut self) -> &mut Self {
        self.state = State::Stopped;
        self
    }

    pub fn is_upload(&self) -> bool {
        self.is_upload == true
    }

    pub fn upload(&mut self) -> &mut Self {
        self.is_upload = true;
        self
    }

    pub fn un_upload(&mut self) -> &mut Self {
        self.is_upload = false;
        self
    }

    pub fn merge(&mut self, other: &Container) -> &mut Self {
        self.is_upload = other.is_upload;
        self.filter = other.filter.clone();
        self.output = other.output.clone();
        self.offset = other.offset.clone();
        self.node_name = other.node_name.clone();
        self.service_name = other.service_name.clone();
        if other.ips.len() > 0 {
            self.ips.clone_from(&other.ips)
        }
        self.state = other.state.clone();
        self
    }

    pub fn is_running(&self) -> bool {
        self.state == State::Running
    }

    pub fn is_ready(&self) -> bool {
        self.state == State::Ready
    }

    pub fn is_stop(&self) -> bool {
        self.state == State::Stopped
    }

    pub fn merge_with(&mut self, other: &Container) -> &mut Self {
        self.merge(other)
    }

    pub fn compare_ns_pod(&self, other: &Container) -> bool {
        self.ns == other.ns && self.pod_name == other.pod_name
    }

    pub fn incr_offset(&mut self, offset: i64) -> &mut Self {
        self.offset += offset;
        self.last_offset = offset;
        self
    }
}

impl Default for Container {
    fn default() -> Container {
        Container {
            service_name: "".to_string(),
            path: "".to_string(),
            offset: 0,
            ns: "".to_string(),
            pod_name: "".to_string(),
            container: "".to_string(),
            is_upload: false,
            state: State::Ready,
            filter: Filter {
                max_length: 0,
                expr: "".to_string(),
            },
            output: "".to_string(),
            ips: Vec::new(),
            last_offset: 0,
            node_name: "".to_string(),
        }
    }
}

pub trait GetContainer {
    fn get(&self) -> Option<&Container>;
}

impl GetContainer for Container {
    fn get(&self) -> Option<&Container> {
        Some(self)
    }
}

pub type ContainerList = Vec<Container>;

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ContainerListMarshaller(pub ContainerList);

impl ContainerListMarshaller {
    pub fn to_json(&self) -> String {
        match serde_json::to_string(&self.0) {
            Ok(contents) => contents,
            Err(_) => "".to_owned(),
        }
    }
}

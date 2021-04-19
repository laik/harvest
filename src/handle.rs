use crate::{get_pod_task, GetTask};
use db::GetContainer;
use event::Listener;
use file::FileReaderWriter;
use scan::GetPathEventInfo;

pub(crate) struct DBOpenEvent(pub FileReaderWriter);
impl<T> Listener<T> for DBOpenEvent
where
    T: Clone + GetContainer,
{
    fn handle(&self, t: T) {
        self.0.open_event(&mut t.get().unwrap().clone())
    }
}

pub(crate) struct DBCloseEvent(pub FileReaderWriter);
impl<T> Listener<T> for DBCloseEvent
where
    T: Clone + GetContainer,
{
    fn handle(&self, t: T) {
        self.0.remove_event(&t.get().unwrap().path)
    }
}

pub(crate) struct ScannerCreateEvent(pub FileReaderWriter);
impl<T> Listener<T> for ScannerCreateEvent
where
    T: Clone + GetPathEventInfo,
{
    fn handle(&self, t: T) {
        let mut container = t.get().to_pod();
        db::insert(&container);
        if let Some(t) = get_pod_task(&container.pod_name) {
            if !t.container.is_upload() {
                return;
            }
            self.0.open_event(&mut container);
            self.0.write_event(&container.path)
        }
    }
}

pub(crate) struct ScannerWriteEvent(pub FileReaderWriter);
impl<T> Listener<T> for ScannerWriteEvent
where
    T: Clone + GetPathEventInfo,
{
    fn handle(&self, t: T) {
        self.0.write_event(&t.get().to_pod().path);
    }
}

pub(crate) struct ScannerCloseEvent();
impl<T> Listener<T> for ScannerCloseEvent
where
    T: Clone + GetPathEventInfo,
{
    fn handle(&self, t: T) {
        let mut container = t.get().to_pod();
        container.state_stop();
        db::update(&container);
    }
}

pub(crate) struct TaskRunEvent(pub FileReaderWriter);
impl<T> Listener<T> for TaskRunEvent
where
    T: Clone + GetTask,
{
    fn handle(&self, t: T) {
        let mut container = t.get().container.clone();
        self.0.open_event(&mut container);
    }
}

pub(crate) struct TaskStopEvent(pub FileReaderWriter);
impl<T> Listener<T> for TaskStopEvent
where
    T: Clone + GetTask,
{
    fn handle(&self, t: T) {
        self.0.close_event(&t.get().container.path)
    }
}

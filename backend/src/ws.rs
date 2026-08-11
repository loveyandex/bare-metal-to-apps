use serde::Serialize;
use tokio::sync::broadcast;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Event {
    ServiceStatus {
        service_id: Uuid,
        project_id: Uuid,
        status: String,
        message: Option<String>,
    },
    ServiceLog {
        service_id: Uuid,
        line: String,
    },
    ServiceCreated {
        project_id: Uuid,
        service_id: Uuid,
    },
    ServiceDeleted {
        project_id: Uuid,
        service_id: Uuid,
    },
}

#[derive(Clone)]
pub struct Hub {
    tx: broadcast::Sender<Event>,
}

impl Hub {
    pub fn new() -> Self {
        let (tx, _rx) = broadcast::channel(1024);
        Self { tx }
    }

    pub fn subscribe(&self) -> broadcast::Receiver<Event> {
        self.tx.subscribe()
    }

    pub fn publish(&self, event: Event) {
        let _ = self.tx.send(event);
    }
}

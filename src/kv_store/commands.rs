use rhai::{Array, Dynamic};
use tokio::sync::{mpsc, oneshot};

type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;

/// Provided by the requester and used by the manager task to send
/// the command response back to the requester.
pub type Responder<T> = oneshot::Sender<Result<T>>;

pub type Sender = mpsc::Sender<Command>;

#[derive(Debug, Clone)]
pub enum Value {
    Dynamic(Dynamic),
    Array(Array),
}

#[allow(dead_code)]
#[derive(Debug)]
pub enum Command {
    Get {
        key: String,
        resp: Responder<Value>,
    },
    Exists {
        key: String,
        resp: Responder<bool>,
    },
    Set {
        key: String,
        value: Dynamic,
        resp: Responder<()>,
    },
    SetArray {
        key: String,
        value: Array,
        resp: Responder<()>,
    },
    Delete {
        key: String,
        resp: Responder<()>,
    },
    Append {
        key: String,
        value: Dynamic,
        resp: Responder<()>,
    },
    ListKeys {
        resp: Responder<Vec<String>>,
    },
}

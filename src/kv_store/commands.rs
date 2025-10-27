use serde_json::Value as JsonValue;
use tokio::sync::{mpsc, oneshot};

type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;

/// Provided by the requester and used by the manager task to send
/// the command response back to the requester.
pub type Responder<T> = oneshot::Sender<Result<T>>;

pub type Sender = mpsc::Sender<Command>;

#[allow(dead_code)]
#[derive(Debug)]
pub enum Command {
    Get {
        key: String,
        resp: Responder<Option<JsonValue>>,
    },
    Exists {
        key: String,
        resp: Responder<bool>,
    },
    Set {
        key: String,
        value: JsonValue,
        resp: Responder<()>,
    },
    Delete {
        key: String,
        resp: Responder<()>,
    },
    Append {
        key: String,
        value: JsonValue,
        resp: Responder<()>,
    },
    ListKeys {
        resp: Responder<Vec<String>>,
    },
}

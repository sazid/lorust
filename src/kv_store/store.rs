use std::collections::BTreeMap;

use serde_json::Value as JsonValue;
use tokio::{sync::mpsc, task::JoinHandle};

use crate::kv_store::commands::{Command, Sender, Value};

// 1. Create the receiver, transmitter
// 2. Create the hashmap/btreemap to hold the data
// 3. Return the receivers and transmitters

struct KvStore {
    data: BTreeMap<String, JsonValue>,
}

#[allow(dead_code)]
impl KvStore {
    pub fn new() -> KvStore {
        KvStore {
            data: BTreeMap::new(),
        }
    }

    pub fn get(&self, key: impl ToString) -> Option<&JsonValue> {
        let key = key.to_string();
        self.data.get(&key)
    }

    pub fn exists(&self, key: impl ToString) -> bool {
        let key = key.to_string();
        self.data.contains_key(&key)
    }

    pub fn set(&mut self, key: impl ToString, value: JsonValue) -> Option<JsonValue> {
        let key = key.to_string();
        self.data.insert(key, value)
    }

    pub fn delete(&mut self, key: impl ToString) -> Option<JsonValue> {
        let key = key.to_string();
        self.data.remove(&key)
    }

    pub fn append(&mut self, key: impl ToString, value: JsonValue) {
        let key = key.to_string();

        let arr = match self.data.get_mut(&key) {
            Some(arr) => arr,
            None => return,
        };

        if let JsonValue::Array(arr) = arr {
            arr.push(value);
        }
    }

    pub fn extend_array(&mut self, key: impl ToString, values: Vec<JsonValue>) {
        let key = key.to_string();

        let arr = match self.data.get_mut(&key) {
            Some(arr) => arr,
            None => return,
        };

        if let JsonValue::Array(arr) = arr {
            arr.extend(values);
        }
    }

    pub fn clear(&mut self) {
        self.data.clear();
    }

    pub fn list_keys(&self) -> Vec<String> {
        self.data.keys().cloned().collect()
    }
}

pub async fn new() -> (JoinHandle<()>, Sender) {
    let (tx, mut rx) = mpsc::channel(32);

    let manager = tokio::spawn(async move {
        let mut store = KvStore::new();

        // Start receiving messages
        while let Some(cmd) = rx.recv().await {
            let empty_ok = Ok(());
            match cmd {
                Command::Get { key, resp } => {
                    let res = store.get(key);
                    match res {
                        Some(val) => resp.send(Ok(Value::Json(val.clone()))),
                        None => resp.send(Ok(Value::Json(JsonValue::Null))),
                    }
                    .expect("setting values should never fail");
                }
                Command::Set { key, value, resp } => {
                    store.set(key, value);
                    let _ = resp.send(empty_ok);
                }
                Command::Delete { key, resp } => {
                    store.delete(key);
                    let _ = resp.send(empty_ok);
                }
                Command::Append { key, value, resp } => {
                    store.append(key, value);
                    let _ = resp.send(empty_ok);
                }
                Command::ExtendArray { key, values, resp } => {
                    store.extend_array(key, values);
                    let _ = resp.send(empty_ok);
                }
                Command::Exists { key, resp } => {
                    let exists = store.exists(key);
                    let _ = resp.send(Ok(exists));
                }
                Command::SetArray { key, value, resp } => {
                    store.set(key, JsonValue::Array(value));
                    let _ = resp.send(empty_ok);
                }
                Command::ListKeys { resp } => {
                    let keys = store.list_keys();
                    let _ = resp.send(Ok(keys));
                }
            }
        }
    });

    (manager, tx)
}

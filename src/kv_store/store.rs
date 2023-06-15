use std::collections::BTreeMap;

use rhai::Dynamic;
use tokio::sync::mpsc;

use crate::kv_store::commands::{Command, Sender, Value};

// 1. Create the receiver, transmitter
// 2. Create the hashmap/btreemap to hold the data
// 3. Return the receivers and transmitters

struct KvStore {
    data: BTreeMap<String, Value>,
}

#[allow(dead_code)]
impl KvStore {
    pub fn new() -> KvStore {
        KvStore {
            data: BTreeMap::new(),
        }
    }

    pub fn get(&self, key: impl ToString) -> Option<&Value> {
        let key = key.to_string();
        self.data.get(&key)
    }

    pub fn exists(&self, key: impl ToString) -> bool {
        let key = key.to_string();
        self.data.contains_key(&key)
    }

    pub fn set(&mut self, key: impl ToString, value: Value) -> Option<Value> {
        let key = key.to_string();
        self.data.insert(key, value)
    }

    pub fn delete(&mut self, key: impl ToString) -> Option<Value> {
        let key = key.to_string();
        self.data.remove(&key)
    }

    pub fn append(&mut self, key: impl ToString, value: Dynamic) {
        let key = key.to_string();

        // self.data
        //     .entry(key)
        //     .and_modify(|val| match val {
        //         Value::Vec(arr) => arr.push(value),
        //         _ => (),
        //     })
        //     .or_insert(Value::Vec(Vec::new()));

        let arr = match self.data.get_mut(&key) {
            Some(arr) => arr,
            None => return,
        };

        if let Value::Array(arr) = arr {
            arr.push(value);
        }
    }

    pub fn clear(&mut self) {
        self.data.clear();
    }
}

pub async fn new() -> Sender {
    let (tx, mut rx) = mpsc::channel(32);

    let _ = tokio::spawn(async move {
        let mut store = KvStore::new();

        // Start receiving messages
        while let Some(cmd) = rx.recv().await {
            let empty_ok = Ok(());
            match cmd {
                Command::Get { key, resp } => {
                    let res = store.get(key);
                    match res {
                        Some(val) => resp.send(Ok(val.clone())),
                        None => resp.send(Ok(Value::Dynamic(Dynamic::from(())))),
                    }
                    .expect("setting values should never fail");
                }
                Command::Set { key, value, resp } => {
                    store.set(key, Value::Dynamic(value));
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
                Command::Exists { key, resp } => {
                    let exists = store.exists(key);
                    let _ = resp.send(Ok(exists));
                }
                Command::SetArray { key, value, resp } => {
                    store.set(key, Value::Array(value));
                    let _ = resp.send(empty_ok);
                }
            }
        }
    });

    tx.clone()
}

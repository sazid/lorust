use std::{collections::BTreeMap, sync::Arc};
use thiserror::Error;

use std::sync::RwLock;

use serde::{de::DeserializeOwned, Deserialize, Serialize};

#[derive(Error, Debug)]
pub enum KvError {
    #[error("could not deserialize json string")]
    DeserializeFailed,

    #[error("no data found with the given key: {key:?}")]
    NoDataFound { key: String },
}

pub struct KvStore {
    data: Arc<RwLock<BTreeMap<String, String>>>,
}

// type Result<T> = Result<T, Box<dyn std::error::Error + Send + Sync>>;

#[allow(dead_code)]
impl KvStore {
    pub fn new() -> KvStore {
        KvStore {
            data: Arc::new(RwLock::new(BTreeMap::new())),
        }
    }

    pub fn clear(&self) {
        self.data.write().unwrap().clear();
    }

    pub fn set(&self, key: String, value: String) -> Option<String> {
        self.data.write().unwrap().insert(key, value)
    }

    pub fn get(&self, key: String) -> Option<String> {
        self.data
            .read()
            .unwrap()
            .get(&key)
            .and_then(|s| Some(s.clone()))
    }

    pub fn set_json(
        &self,
        key: String,
        value: &impl Serialize,
    ) -> std::result::Result<Option<String>, Box<dyn std::error::Error + Send + Sync>> {
        let value = serde_json::to_string(value)?;
        Ok(self.data.write().unwrap().insert(key, value))
    }

    pub fn get_json<T>(&self, key: String) -> std::result::Result<T, KvError>
    where
        T: DeserializeOwned,
    {
        let reader = self.data.read().unwrap();
        let value = match reader.get(&key) {
            Some(s) => s.clone(),
            None => return Err(KvError::NoDataFound { key: key }),
        };

        let value = serde_json::from_str(&value);
        match value {
            Ok(val) => Ok(val),
            Err(_) => Err(KvError::DeserializeFailed),
        }
    }
}

impl Clone for KvStore {
    fn clone(&self) -> Self {
        Self {
            data: Arc::clone(&self.data),
        }
    }
}

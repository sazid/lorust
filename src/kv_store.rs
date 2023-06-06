use std::{any::Any, collections::BTreeMap, sync::Arc};
use thiserror::Error;

use parking_lot::{MappedRwLockReadGuard, RwLock};

use serde::{de::DeserializeOwned, Serialize};

#[derive(Error, Debug)]
pub enum KvError {
    #[error("could not deserialize json string")]
    DeserializeError,

    #[error("no data found with the given key: {key:?}")]
    NoDataFound { key: String },
}

pub struct KvStore {
    data: Arc<RwLock<BTreeMap<String, Box<dyn Any + Send + Sync>>>>,
}

type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;

#[allow(dead_code)]
impl KvStore {
    pub fn new() -> KvStore {
        KvStore {
            data: Arc::new(RwLock::new(BTreeMap::new())),
        }
    }

    pub fn clear(&self) {
        self.data.write().clear();
    }

    pub fn set<T: Send + Sync + 'static>(&self, key: String, value: T) -> Result<()> {
        self.data.write().insert(key, Box::new(value));
        Ok(())
    }

    pub fn get<T: Send + Sync + 'static>(&self, key: String) -> MappedRwLockReadGuard<Option<&T>> {
        let lock: parking_lot::lock_api::RwLockReadGuard<
            parking_lot::RawRwLock,
            BTreeMap<String, Box<dyn Any + Send + Sync>>,
        > = self.data.read();

        MappedRwLockReadGuard::map(
            &lock,
            |lock: parking_lot::lock_api::RwLockReadGuard<
                parking_lot::RawRwLock,
                BTreeMap<String, Box<dyn Any + Send + Sync>>,
            >| { lock.get(&key).and_then(|s| s.downcast_ref()) },
        )
    }

    pub fn set_json(&self, key: String, value: &impl Serialize) -> Result<()> {
        let value = serde_json::to_string(value)?;
        self.data.write().insert(key, Box::new(value));
        Ok(())
    }

    pub fn get_json<T>(&self, key: String) -> Option<T>
    where
        T: DeserializeOwned,
    {
        let reader = self.data.read();
        let value = reader.get(&key)?;

        let value = *value.downcast_ref::<&str>()?;
        let value = serde_json::from_str(value);
        value.ok()
    }
}

impl Clone for KvStore {
    fn clone(&self) -> Self {
        Self {
            data: Arc::clone(&self.data),
        }
    }
}

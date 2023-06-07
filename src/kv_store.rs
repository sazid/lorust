use std::{collections::BTreeMap, str::FromStr, sync::Arc};
use thiserror::Error;

use parking_lot::{MappedRwLockReadGuard, RwLock, RwLockReadGuard};

use rhai::Dynamic;

use serde::{de::DeserializeOwned, Serialize};

pub struct KvStore {
    data: Arc<RwLock<BTreeMap<String, Dynamic>>>,
}

#[derive(Error, Debug)]
pub enum KvError {
    #[error("could not cast from `&str` to `rhai::Dynamic`")]
    CastFromStrError,
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

    pub fn set<T: Clone + Send + Sync + 'static>(
        &self,
        key: impl ToString,
        value: T,
    ) -> Option<Dynamic> {
        let key = key.to_string();
        self.data.write().insert(key, Dynamic::from(value))
    }

    pub fn get<T: 'static>(&self, key: impl ToString) -> Option<MappedRwLockReadGuard<&T>> {
        let key = key.to_string();
        RwLockReadGuard::try_map(self.data.read(), |m| {
            m.get(&key).and_then(|v| v.clone().try_cast())
        })
        .ok()
    }

    pub fn set_json(
        &self,
        key: impl ToString,
        value: &impl Serialize,
    ) -> Result<Option<rhai::Dynamic>> {
        let key = key.to_string();
        let value = serde_json::to_string(value)?;
        let value = Dynamic::from_str(&value).map_err(|_| KvError::CastFromStrError)?;
        Ok(self.data.write().insert(key, value))
    }

    pub fn get_json<T>(&self, key: impl ToString) -> Option<T>
    where
        T: DeserializeOwned,
    {
        let key = key.to_string();
        let reader = self.data.read();
        let value = reader.get(&key)?;

        let value = value.clone().into_string().ok()?;
        let value = serde_json::from_str(&value).ok();
        value
    }
}

impl Clone for KvStore {
    fn clone(&self) -> Self {
        Self {
            data: Arc::clone(&self.data),
        }
    }
}

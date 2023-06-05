use std::{collections::BTreeMap, sync::Arc};

use std::sync::RwLock;

pub struct KvStore {
    data: Arc<RwLock<BTreeMap<String, String>>>,
}

impl KvStore {
    pub fn new() -> KvStore {
        KvStore {
            data: Arc::new(RwLock::new(BTreeMap::new())),
        }
    }

    pub fn clear(&mut self) {
        self.data.write().unwrap().clear();
    }

    pub fn set(&mut self, key: String, value: String) {
        self.data.write().unwrap().insert(key, value);
    }

    pub fn get(&self, key: String) -> Option<String> {
        match self.data.read().unwrap().get(&key) {
            Some(s) => Some(s.clone()),
            None => None,
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

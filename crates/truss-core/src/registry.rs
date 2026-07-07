use crate::error::{Error, Result};
use rustc_hash::FxHashMap;

#[derive(Debug, Default)]
pub struct Registry {
    entries: FxHashMap<String, String>,
}

impl Registry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn insert(&mut self, key: String, value: String) -> Result<()> {
        if key.is_empty() {
            return Err(Error::Argument("registry key cannot be empty".to_string()));
        }
        let _ = self.entries.insert(key, value);
        Ok(())
    }

    pub fn get(&self, key: &str) -> Option<&str> {
        self.entries.get(key).map(String::as_str)
    }

    pub fn validate(&self) -> Result<()> {
        if self.entries.is_empty() {
            return Err(Error::EmptyRegistry);
        }
        Ok(())
    }
}

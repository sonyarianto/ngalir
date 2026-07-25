use anyhow::Result;

use crate::error::FlowError;
use serde_json::Value;
use std::collections::HashMap;
use std::path::PathBuf;

pub(crate) struct StateStore {
    pub(crate) path: Option<PathBuf>,
    pub(crate) data: HashMap<String, Value>,
}

impl StateStore {
    pub(crate) fn disabled() -> Self {
        Self {
            path: None,
            data: HashMap::new(),
        }
    }

    pub(crate) fn load_or_new(path: PathBuf) -> Self {
        let data = std::fs::read_to_string(&path)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default();
        Self {
            path: Some(path),
            data,
        }
    }

    pub(crate) fn contains(&self, id: &str) -> bool {
        self.data.contains_key(id)
    }

    pub(crate) fn insert(&mut self, id: String, value: Value) {
        self.data.insert(id, value);
    }

    pub(crate) fn save(&self) -> Result<()> {
        let Some(path) = &self.path else {
            return Ok(());
        };
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| FlowError::StateError(e.to_string()))?;
        }
        let tmp = path.with_extension("tmp");
        let json =
            serde_json::to_string(&self.data).map_err(|e| FlowError::StateError(e.to_string()))?;
        std::fs::write(&tmp, &json).map_err(|e| FlowError::StateError(e.to_string()))?;
        std::fs::rename(&tmp, path).map_err(|e| FlowError::StateError(e.to_string()))?;
        Ok(())
    }
}

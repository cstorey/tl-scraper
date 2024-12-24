use std::{collections::HashMap, path::PathBuf};

use chrono::Days;
use serde::Deserialize;
use uuid::Uuid;

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct ProviderConfig {
    pub(crate) output: PathBuf,
    pub(crate) requisition_id: Uuid,
    pub(crate) history_days: Option<u64>,
}
impl ProviderConfig {
    pub(crate) fn history_days(&self) -> Days {
        Days::new(self.history_days.unwrap_or(90))
    }
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct ScraperConfig {
    pub(crate) provider: HashMap<String, ProviderConfig>,
}

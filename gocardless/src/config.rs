use std::{collections::HashMap, path::PathBuf};

use chrono::Days;
use clap::Args;
use color_eyre::eyre::Context;
use serde::Deserialize;
use uuid::Uuid;

#[derive(Debug, Clone, Args)]
pub(crate) struct ConfigArg {
    #[clap(short = 'c', long = "config", help = "Configuration file")]
    config: PathBuf,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct ProviderConfig {
    pub(crate) output: PathBuf,
    pub(crate) requisition_id: Uuid,
    pub(crate) history_days: Option<u64>,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct ScraperConfig {
    pub(crate) provider: HashMap<String, ProviderConfig>,
}

impl ConfigArg {
    pub(crate) async fn load(&self) -> color_eyre::Result<ScraperConfig> {
        let content = tokio::fs::read_to_string(&self.config)
            .await
            .wrap_err_with(|| format!("Reading config file: {:?}", self.config))?;
        let config = toml::from_str(&content).context("Parse toml")?;

        Ok(config)
    }
}

impl ProviderConfig {
    pub(crate) fn history_days(&self) -> Days {
        Days::new(self.history_days.unwrap_or(90))
    }
}

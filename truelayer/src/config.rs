use std::{collections::HashMap, fs::File, path::PathBuf};

use anyhow::{anyhow, Context, Result};
use serde::{Deserialize, Serialize};

use crate::{ClientCreds, Environment};

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct MainConfig {
    pub client_credentials: PathBuf,
    pub environment: Environment,
    pub request_timeout_s: Option<u64>,
}
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ProviderConfig {
    pub user_token: PathBuf,
    pub target_dir: PathBuf,
    #[serde(default)]
    pub scrape_accounts: bool,
    #[serde(default)]
    pub scrape_cards: bool,
    #[serde(default)]
    pub scrape_info: bool,
}
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ScraperConfig {
    pub main: MainConfig,
    pub providers: HashMap<String, ProviderConfig>,
}
impl ScraperConfig {
    pub fn credentials(&self) -> Result<ClientCreds> {
        let rdr = File::open(&self.main.client_credentials).with_context(|| {
            format!(
                "Opening client credentials: {:?}",
                self.main.client_credentials
            )
        })?;
        let client_creds = serde_json::from_reader(rdr).with_context(|| {
            format!(
                "Decoding client credentials: {:?}",
                self.main.client_credentials
            )
        })?;
        Ok(client_creds)
    }

    pub fn provider(&self, name: &str) -> Result<&ProviderConfig> {
        if let Some(provider) = self.providers.get(name) {
            Ok(provider)
        } else {
            Err(anyhow!(
                "Provider not found: {}, known: {:?}",
                name,
                self.providers.keys().collect::<Vec<_>>()
            ))
        }
    }
}

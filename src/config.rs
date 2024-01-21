use std::path::PathBuf;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct MainConfig {
    pub client_credentials: PathBuf,
}
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ScraperConfig {
    pub main: MainConfig,
}

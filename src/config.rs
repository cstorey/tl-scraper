use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::Environment;

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct MainConfig {
    pub client_credentials: PathBuf,
    pub environment: Environment,
}
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ScraperConfig {
    pub main: MainConfig,
}

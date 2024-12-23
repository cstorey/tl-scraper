use std::path::Path;

use chrono::{DateTime, Duration, Utc};
use color_eyre::Result;
use serde::{Deserialize, Serialize};
use tracing::{debug, instrument};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct Secrets {
    secret_id: String,
    secret_key: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct GCToken {
    access: String,
    access_expires: i64,
    refresh: String,
    refresh_expires: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct Token {
    pub(crate) access: String,
    access_expires: DateTime<Utc>,
    refresh: String,
    refresh_expires: DateTime<Utc>,
}
impl Token {
    pub(crate) fn from_gc_token(authed_at: DateTime<Utc>, gctoken: &GCToken) -> Token {
        Token {
            access: gctoken.access.clone(),
            access_expires: authed_at + Duration::seconds(gctoken.access_expires),
            refresh: gctoken.refresh.clone(),
            refresh_expires: authed_at + Duration::seconds(gctoken.refresh_expires),
        }
    }
}

#[instrument(skip_all, fields(?path))]
pub(crate) async fn store_token(path: &Path, tok: &Token) -> Result<()> {
    let buf = serde_json::to_vec(&tok)?;
    tokio::fs::write(&path, buf).await?;
    debug!(?path, "Stored token");
    Ok(())
}

#[instrument(skip_all, fields(?path))]
pub(crate) async fn load_token(path: &Path) -> Result<Token> {
    let buf = tokio::fs::read(&path).await?;
    let secrets = serde_json::from_slice(&buf)?;

    debug!(?path, "Loaded token");

    Ok(secrets)
}

#[instrument(skip_all, fields(?path))]
pub(crate) async fn load_secrets(path: &Path) -> Result<Secrets> {
    let buf = tokio::fs::read(&path).await?;
    let secrets = serde_json::from_slice(&buf)?;

    debug!(?path, "Loaded secrets");

    Ok(secrets)
}

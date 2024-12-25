use std::path::PathBuf;

use chrono::{DateTime, Duration, Utc};
use clap::Parser;
use color_eyre::Result;
use serde::{Deserialize, Serialize};
use tracing::{debug, info, instrument};

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Secrets {
    secret_id: String,
    secret_key: String,
}

#[derive(Debug, Parser)]
pub struct Cmd {
    #[clap(short = 's', long = "secrets", help = "Secrets file")]
    secrets: PathBuf,
    #[clap(short = 't', long = "token", help = "Token file")]
    token: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct GCToken {
    access: String,
    access_expires: i64,
    refresh: String,
    refresh_expires: i64,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
struct Token {
    access: String,
    access_expires: DateTime<Utc>,
    refresh: String,
    refresh_expires: DateTime<Utc>,
}

impl Cmd {
    #[instrument("connect", skip_all)]
    pub(crate) async fn run(&self) -> Result<()> {
        let secrets = self.load_secrets().await?;

        info!("Connecting");

        let client = reqwest::Client::new();

        let authed_at = Utc::now();

        let resp = client
            .post("https://bankaccountdata.gocardless.com/api/v2/token/new/")
            .json(&secrets)
            .send()
            .await?
            .error_for_status()?;

        let gc_token = resp.json::<GCToken>().await?;

        info!("Fetched token: {:#?}", gc_token);

        let tok = gc_token.as_of(authed_at);

        self.store_token(&tok).await?;

        Ok(())
    }

    #[instrument(skip_all, fields(path=?self.token))]
    async fn store_token(&self, tok: &Token) -> Result<()> {
        let buf = serde_json::to_vec(&tok)?;
        tokio::fs::write(&self.token, buf).await?;
        debug!(token=?self.token, "Stored token");
        Ok(())
    }

    #[instrument(skip_all, fields(path=?self.secrets))]
    async fn load_secrets(&self) -> Result<Secrets> {
        let buf = tokio::fs::read(&self.secrets).await?;
        let secrets = serde_json::from_slice(&buf)?;

        debug!("Loaded secrets");

        Ok(secrets)
    }
}

impl GCToken {
    fn as_of(&self, authed_at: DateTime<Utc>) -> Token {
        Token {
            access: self.access.clone(),
            access_expires: authed_at + Duration::seconds(self.access_expires),
            refresh: self.refresh.clone(),
            refresh_expires: authed_at + Duration::seconds(self.refresh_expires),
        }
    }
}

use std::path::PathBuf;

use chrono::Utc;
use clap::Parser;
use color_eyre::Result;
use tracing::{info, instrument};

use crate::auth::{load_secrets, store_token, GCToken, Token};

#[derive(Debug, Parser)]
pub struct Cmd {
    #[clap(short = 's', long = "secrets", help = "Secrets file")]
    secrets: PathBuf,
    #[clap(short = 't', long = "token", help = "Token file")]
    token: PathBuf,
}

impl Cmd {
    #[instrument("connect", skip_all)]
    pub(crate) async fn run(&self) -> Result<()> {
        let secrets = load_secrets(&self.secrets).await?;

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

        let tok = Token::from_gc_token(authed_at, &gc_token);

        store_token(&self.token, &tok).await?;

        Ok(())
    }
}

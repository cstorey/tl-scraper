use std::path::PathBuf;

use clap::Parser;
use color_eyre::Result;
use serde::{Deserialize, Serialize};
use tracing::{info, instrument};

use crate::{auth::load_token, client::BankDataClient};

#[derive(Debug, Parser)]
pub struct Cmd {
    #[clap(short = 't', long = "token", help = "Token file")]
    token: PathBuf,
}

#[derive(Debug, Serialize, Deserialize)]
struct Institution {
    id: String,
    name: String,
    bic: String,
    transaction_total_days: String,
    max_access_valid_for_days: String,
    countries: Vec<String>,
    logo: String,
}

impl Cmd {
    #[instrument("institutions", skip_all)]
    pub(crate) async fn run(&self) -> Result<()> {
        let token = load_token(&self.token).await?;

        let client = BankDataClient::new(token);

        let data = client
            .get::<Vec<Institution>>("/api/v2/institutions/?country=gb")
            .await?;

        info!("Institutions: {}", serde_json::to_string_pretty(&data)?);

        Ok(())
    }
}
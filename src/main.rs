use std::path::Path;

use anyhow::Result;
use secrecy::SecretString;
use structopt::StructOpt;
use tl_scraper::TlClient;
use tracing::info;

#[derive(Debug, StructOpt)]
struct Options {
    #[structopt(short = "i", long = "client-id")]
    client_id: String,
    #[structopt(short = "s", long = "client-secret")]
    client_secret: SecretString,
    #[structopt(short = "t", long = "access-code")]
    access_code: SecretString,
}

const TOKEN_FILE: &str = "token.json";

#[tokio::main]
async fn main() -> Result<()> {
    let opts = Options::from_args();

    tracing_log::LogTracer::init()?;
    tracing::subscriber::set_global_default(
        tracing_subscriber::FmtSubscriber::builder()
            .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
            .with_ansi(false)
            .with_timer(tracing_subscriber::fmt::time::ChronoUtc::rfc3339())
            .with_thread_names(true)
            .with_thread_ids(true)
            .finish(),
    )?;

    let client_id = opts.client_id;
    let client_secret = opts.client_secret;
    let access_code = opts.access_code;
    let client = reqwest::Client::new();

    let token_path = Path::new(TOKEN_FILE);
    let tl =
        TlClient::authenticate(client, token_path, client_id, client_secret, access_code).await?;

    let info_response = tl.fetch_info().await?;

    info!(json=?info_response, "Response");

    Ok(())
}

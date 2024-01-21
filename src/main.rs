use std::{fs::File, path::PathBuf, sync::Arc};

use anyhow::{Context, Result};
use chrono::NaiveDate;
use clap::{Parser, Subcommand};
use secrecy::SecretString;
use serde::Deserialize;
use tl_scraper::{Environment, JobPool, ScraperConfig, TlClient};
use tracing::{debug, instrument, Instrument, Span};

#[derive(Debug, Parser)]
struct Options {
    #[clap(short = 'c', long = "config")]
    config: PathBuf,
    #[clap(short = 'u', long = "user-token")]
    user_token: PathBuf,
    #[clap(short = 'l', long = "live")]
    live: bool,
    #[clap(subcommand)]
    command: Commands,
}

#[derive(Debug, Deserialize)]
struct ClientCreds {
    id: String,
    secret: SecretString,
}

#[derive(Debug, Subcommand)]
enum Commands {
    Auth {},
    Sync(Sync),
}

#[derive(Debug, Parser)]
struct Sync {
    from_date: NaiveDate,
    to_date: NaiveDate,
    #[clap(short = 'i', long = "info")]
    scrape_info: bool,
    #[clap(short = 'a', long = "accounts")]
    scrape_accounts: bool,
    #[clap(short = 'c', long = "cards")]
    scrape_cards: bool,
    #[clap(short = 't', long = "concurrent-tasks")]
    concurrency: Option<usize>,
    target_dir: PathBuf,
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_log::LogTracer::init()?;
    tracing::subscriber::set_global_default(
        tracing_subscriber::FmtSubscriber::builder()
            .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
            .with_ansi(false)
            .with_timer(tracing_subscriber::fmt::time::UtcTime::rfc_3339())
            .with_thread_names(true)
            .with_thread_ids(true)
            .finish(),
    )?;

    run().await?;

    Ok(())
}

async fn run() -> Result<()> {
    let opts = Options::parse();

    let config: ScraperConfig = {
        let content = std::fs::read_to_string(&opts.config).context("Reading config file")?;
        toml::from_str(&content).context("Parse toml")?
    };

    let client_creds: ClientCreds = {
        let rdr = File::open(&config.main.client_credentials).with_context(|| {
            format!(
                "Opening client credentials: {:?}",
                config.main.client_credentials
            )
        })?;
        serde_json::from_reader(rdr).with_context(|| {
            format!(
                "Decoding client credentials: {:?}",
                config.main.client_credentials
            )
        })?
    };

    let client = reqwest::Client::new();

    let tl = Arc::new(TlClient::new(
        client,
        opts.truelayer_env(),
        &opts.user_token,
        client_creds.id,
        client_creds.secret,
    ));

    match opts.command {
        Commands::Auth {} => tl_scraper::authenticate(tl).await?,
        Commands::Sync(sync_opts) => {
            sync(tl, sync_opts).await?;
        }
    };
    Ok(())
}

#[instrument(skip_all)]
async fn sync(
    tl: Arc<TlClient>,
    Sync {
        from_date,
        to_date,
        scrape_info,
        scrape_accounts: accounts,
        scrape_cards: cards,
        target_dir,
        concurrency,
    }: Sync,
) -> Result<(), anyhow::Error> {
    let target_dir = Arc::from(target_dir.into_boxed_path());
    let (pool, handle) = JobPool::new(concurrency.unwrap_or(1));
    if scrape_info {
        debug!("Scraping info");
        handle.spawn(
            tl_scraper::sync_info(tl.clone(), Arc::clone(&target_dir)).instrument(Span::current()),
        )?;
    }
    if accounts {
        debug!("Scraping accounts");
        handle.spawn(
            tl_scraper::sync_accounts(
                tl.clone(),
                target_dir.clone(),
                from_date..=to_date,
                handle.clone(),
            )
            .instrument(Span::current()),
        )?;
    }
    if cards {
        debug!("Scraping cards");
        handle.spawn(
            tl_scraper::sync_cards(
                tl.clone(),
                target_dir.clone(),
                from_date..=to_date,
                handle.clone(),
            )
            .instrument(Span::current()),
        )?;
    }
    drop(handle);
    debug!("Waiting for finish");
    pool.run().await?;
    Ok(())
}

impl Options {
    pub(crate) fn truelayer_env(&self) -> Environment {
        if self.live {
            Environment::Live
        } else {
            Environment::Sandbox
        }
    }
}

use std::{fs::File, path::PathBuf, sync::Arc};

use anyhow::{anyhow, Context, Result};
use chrono::NaiveDate;
use clap::{Parser, Subcommand};
use futures::TryFutureExt;
use secrecy::SecretString;
use serde::Deserialize;
use tokio::try_join;
use tracing::{debug, instrument, Instrument, Span};

use tl_scraper::{JobHandle, JobPool, ProviderConfig, ScraperConfig, TlClient};

#[derive(Debug, Parser)]
struct Options {
    #[clap(short = 'c', long = "config")]
    config: PathBuf,
    #[clap(short = 'p', long = "provider")]
    provider: String,
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
    #[clap(short = 't', long = "concurrent-tasks")]
    concurrency: Option<usize>,
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

    let provider = config.providers.get(&opts.provider).ok_or_else(|| {
        anyhow!(
            "Provider not found: {}, known: {:?}",
            opts.provider,
            config.providers.keys().collect::<Vec<_>>()
        )
    })?;

    let client = reqwest::Client::new();

    match opts.command {
        Commands::Auth {} => {
            let tl = Arc::new(TlClient::new(
                client,
                config.main.environment,
                &provider.user_token,
                client_creds.id,
                client_creds.secret,
            ));
            tl_scraper::authenticate(tl).await?
        }
        Commands::Sync(sync_opts) => {
            let (pool, handle) = JobPool::new(sync_opts.concurrency.unwrap_or(1));
            let tl = Arc::new(TlClient::new(
                client,
                config.main.environment,
                &provider.user_token,
                client_creds.id,
                client_creds.secret,
            ));

            try_join!(
                pool.run().map_err(|e| e.context("Job pool")),
                sync(tl, sync_opts, provider, handle).map_err(|e| e.context("Sync scheduler")),
            )?;
        }
    };
    Ok(())
}

#[instrument(skip_all)]
async fn sync(
    tl: Arc<TlClient>,
    Sync {
        from_date, to_date, ..
    }: Sync,
    provider: &ProviderConfig,
    handle: JobHandle,
) -> Result<(), anyhow::Error> {
    let target_dir = Arc::from(provider.target_dir.clone().into_boxed_path());
    if provider.scrape_info {
        debug!("Scraping info");
        handle.spawn(
            tl_scraper::sync_info(tl.clone(), Arc::clone(&target_dir)).instrument(Span::current()),
        )?;
    }
    if provider.scrape_accounts {
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
    if provider.scrape_cards {
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
    debug!("Scheduled sync tasks");
    Ok(())
}

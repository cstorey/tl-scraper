use std::{path::PathBuf, sync::Arc};

use anyhow::{Context, Result};
use chrono::NaiveDate;
use clap::{Parser, Subcommand};
use futures::TryFutureExt;
use reqwest::Client;
use tokio::try_join;
use tracing::{debug, instrument, Instrument, Span};

use tl_scraper::{
    ClientCreds, Environment, JobHandle, JobPool, ProviderConfig, ScraperConfig, TlClient,
};

#[derive(Debug, Parser)]
struct Options {
    #[clap(short = 'c', long = "config")]
    config: PathBuf,
    #[clap(subcommand)]
    command: Commands,
}

#[derive(Debug, Subcommand)]
enum Commands {
    Auth {
        #[clap(short = 'p', long = "provider")]
        provider: String,
    },
    Sync(Sync),
}

#[derive(Debug, Parser)]
struct Sync {
    #[clap(short = 'p', long = "provider")]
    provider: Vec<String>,
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

    let client_creds = config.credentials()?;

    let client = reqwest::Client::new();

    match opts.command {
        Commands::Auth { provider } => {
            let provider: &ProviderConfig = config.provider(&provider)?;
            tl_scraper::authenticate(&client, config.main.environment, provider, &client_creds)
                .await?;
        }
        Commands::Sync(ref sync_opts) => {
            let (pool, handle) = JobPool::new(sync_opts.concurrency.unwrap_or(1));

            try_join!(
                pool.run().map_err(|e| e.context("Job pool")),
                sync_all(client, sync_opts, &config, &client_creds, handle),
            )?;
        }
    };
    Ok(())
}

async fn sync_all(
    client: Client,
    sync_opts: &Sync,
    config: &ScraperConfig,
    client_creds: &ClientCreds,
    handle: JobHandle,
) -> Result<()> {
    for provider_name in sync_opts.provider.iter() {
        let provider: &ProviderConfig = config.provider(provider_name)?;

        sync(
            client.clone(),
            config.main.environment,
            sync_opts,
            provider_name,
            provider,
            client_creds,
            handle.clone(),
        )
        .await
        .with_context(|| format!("Sync scheduler: {}", &provider_name))?;
    }
    drop(handle);
    Ok(())
}

#[instrument(skip_all, fields(provider=%provider_name))]
async fn sync(
    client: Client,
    environment: Environment,
    Sync {
        from_date, to_date, ..
    }: &Sync,
    provider_name: &str,
    provider: &ProviderConfig,
    client_creds: &ClientCreds,
    handle: JobHandle,
) -> Result<(), anyhow::Error> {
    let target_dir = Arc::from(provider.target_dir.clone().into_boxed_path());
    let tl = Arc::new(TlClient::new(
        client,
        environment,
        &provider.user_token,
        client_creds,
    ));

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
                *from_date..=*to_date,
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
                *from_date..=*to_date,
                handle.clone(),
            )
            .instrument(Span::current()),
        )?;
    }
    drop(handle);
    debug!("Scheduled sync tasks");
    Ok(())
}

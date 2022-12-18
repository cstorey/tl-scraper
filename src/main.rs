use std::{fs::File, path::PathBuf, sync::Arc};

use anyhow::{Context, Result};
use chrono::NaiveDate;
use secrecy::SecretString;
use serde::Deserialize;
use structopt::StructOpt;
use tl_scraper::{Environment, JobPool, TlClient};
use tracing::debug;

#[derive(Debug, StructOpt)]
struct Options {
    #[structopt(short = "c", long = "client-credentials")]
    client_credentials: PathBuf,
    #[structopt(short = "u", long = "user-token")]
    user_token: PathBuf,
    #[structopt(short = "l", long = "live")]
    live: bool,
    #[structopt(subcommand)]
    command: Commands,
}

#[derive(Debug, Deserialize)]
struct ClientCreds {
    id: String,
    secret: SecretString,
}

#[derive(Debug, StructOpt)]
enum Commands {
    Auth {
        #[structopt(short = "t", long = "access-code")]
        access_code: SecretString,
    },
    Info {},
    Accounts {},
    AccountBalance {
        account_id: String,
    },
    AccountTx {
        account_id: String,
        from_date: NaiveDate,
        to_date: NaiveDate,
    },
    Cards {},
    CardBalance {
        account_id: String,
    },
    CardTx {
        account_id: String,
        from_date: NaiveDate,
        to_date: NaiveDate,
    },
    Sync {
        from_date: NaiveDate,
        to_date: NaiveDate,
        #[structopt(short = "i", long = "info")]
        scrape_info: bool,
        #[structopt(short = "a", long = "accounts")]
        scrape_accounts: bool,
        #[structopt(short = "c", long = "cards")]
        scrape_cards: bool,
        #[structopt(short = "t", long = "concurrent-tasks")]
        concurrency: Option<usize>,
        target_dir: PathBuf,
    },
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
    let opts = Options::from_args();

    let client_creds: ClientCreds =
        serde_json::from_reader(File::open(&opts.client_credentials).with_context(|| {
            format!("Opening client credentials: {:?}", opts.client_credentials)
        })?)
        .with_context(|| format!("Decoding client credentials: {:?}", opts.client_credentials))?;

    let client = reqwest::Client::new();

    let tl = Arc::new(TlClient::new(
        client,
        opts.truelayer_env(),
        &opts.user_token,
        client_creds.id,
        client_creds.secret,
    ));

    match opts.command {
        Commands::Auth { access_code } => {
            tl.authenticate(access_code).await?;
        }
        Commands::Info {} => {
            let info_response = tl.fetch_info().await?;

            println!("{:#?}", info_response);
        }
        Commands::Accounts {} => {
            let accounts_response = tl.fetch_accounts().await?;

            println!("{:#?}", accounts_response);
        }
        Commands::AccountBalance { account_id } => {
            let balance_response = tl.account_balance(&account_id).await?;

            println!("{:#?}", balance_response);
        }
        Commands::AccountTx {
            account_id,
            from_date,
            to_date,
        } => {
            let response = tl
                .account_transactions(&account_id, from_date, to_date)
                .await?;

            println!("{:#?}", response);
        }
        Commands::Cards {} => {
            let cards_response = tl.fetch_cards().await?;

            println!("{:#?}", cards_response);
        }
        Commands::CardBalance {
            account_id: card_id,
        } => {
            let balance_response = tl.card_balance(&card_id).await?;

            println!("{:#?}", balance_response);
        }
        Commands::CardTx {
            account_id,
            from_date,
            to_date,
        } => {
            let response = tl
                .card_transactions(&account_id, from_date, to_date)
                .await?;

            println!("{:#?}", response);
        }
        Commands::Sync {
            from_date,
            to_date,
            scrape_info,
            scrape_accounts: accounts,
            scrape_cards: cards,
            target_dir,
            concurrency,
        } => {
            let target_dir = Arc::from(target_dir.into_boxed_path());
            let (pool, handle) = JobPool::new(concurrency.unwrap_or(1));

            if scrape_info {
                debug!("Scraping info");
                handle.spawn(tl_scraper::sync_info(tl.clone(), Arc::clone(&target_dir)))?;
            }

            if accounts {
                debug!("Scraping accounts");
                handle.spawn(tl_scraper::sync_accounts(
                    tl.clone(),
                    target_dir.clone(),
                    from_date,
                    to_date,
                    handle.clone(),
                ))?;
            }

            if cards {
                debug!("Scraping cards");
                handle.spawn(tl_scraper::sync_cards(
                    tl.clone(),
                    target_dir.clone(),
                    from_date,
                    to_date,
                    handle.clone(),
                ))?;
            }

            // Needed to close the channel.
            drop(handle);

            debug!("Waiting for finish");
            pool.run().await?;
        }
    };
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

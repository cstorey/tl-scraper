use std::path::Path;

use anyhow::Result;
use chrono::NaiveDate;
use secrecy::SecretString;
use structopt::StructOpt;
use tl_scraper::TlClient;

#[derive(Debug, StructOpt)]
struct Options {
    #[structopt(short = "i", long = "client-id")]
    client_id: String,
    #[structopt(short = "s", long = "client-secret")]
    client_secret: SecretString,
    #[structopt(subcommand)]
    command: Commands,
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
}

const TOKEN_FILE: &str = "token.json";

#[tokio::main]
async fn main() -> Result<()> {
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

    run().await?;

    Ok(())
}

async fn run() -> Result<()> {
    let opts = Options::from_args();

    let client_id = opts.client_id;
    let client_secret = opts.client_secret;
    let client = reqwest::Client::new();

    let token_path = Path::new(TOKEN_FILE);
    let tl = TlClient::new(client, token_path, client_id, client_secret);

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
    };
    Ok(())
}

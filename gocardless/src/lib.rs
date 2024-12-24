mod accounts;
mod auth;
mod client;
mod connect;
mod institutions;
mod sync;
mod transactions;

use clap::Parser;
use color_eyre::Result;

#[derive(Debug, Parser)]
pub enum Command {
    Auth(auth::Cmd),
    Institutions(institutions::Cmd),
    Connect(connect::Cmd),
    ListAccounts(accounts::ListCmd),
    ListTransactions(transactions::Cmd),
    Sync(sync::Cmd),
}

impl Command {
    pub async fn run(&self) -> Result<()> {
        match self {
            Command::Auth(cmd) => cmd.run().await?,
            Command::Institutions(cmd) => cmd.run().await?,
            Command::Connect(cmd) => cmd.run().await?,
            Command::ListAccounts(cmd) => cmd.run().await?,
            Command::ListTransactions(cmd) => cmd.run().await?,
            Command::Sync(cmd) => cmd.run().await?,
        }

        Ok(())
    }
}

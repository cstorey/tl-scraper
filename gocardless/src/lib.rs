mod accounts;
mod auth;
mod client;
mod config;
mod connect;
mod institutions;
mod sync;
mod transactions;

use clap::Parser;
use color_eyre::Result;

#[derive(Debug, Parser)]
pub enum Command {
    Institutions(institutions::Cmd),
    Connect(connect::Cmd),
    Sync(sync::Cmd),
}

impl Command {
    pub async fn run(&self) -> Result<()> {
        match self {
            Command::Institutions(cmd) => cmd.run().await?,
            Command::Connect(cmd) => cmd.run().await?,
            Command::Sync(cmd) => cmd.run().await?,
        }

        Ok(())
    }
}

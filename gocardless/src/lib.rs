mod auth;
mod connect;
mod institutions;

use clap::Parser;
use color_eyre::Result;

#[derive(Debug, Parser)]
pub enum Command {
    Connect(connect::Cmd),
    Institutions(institutions::Cmd),
}

impl Command {
    pub async fn run(&self) -> Result<()> {
        match self {
            Command::Connect(cmd) => cmd.run().await?,
            Command::Institutions(cmd) => cmd.run().await?,
        }

        Ok(())
    }
}

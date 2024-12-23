mod auth;
mod connect;

use clap::Parser;
use color_eyre::Result;

#[derive(Debug, Parser)]
pub enum Command {
    Connect(connect::Cmd),
}

impl Command {
    pub async fn run(&self) -> Result<()> {
        match self {
            Command::Connect(cmd) => cmd.run().await?,
        }

        Ok(())
    }
}

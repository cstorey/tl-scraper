use clap::Parser;
use color_eyre::Result;

use gc_scraper::Command;

#[tokio::main]
async fn main() -> Result<()> {
    setup_logging()?;
    color_eyre::install()?;

    let cmd = Command::parse();

    cmd.run().await?;

    Ok(())
}

fn setup_logging() -> Result<()> {
    use tracing_error::ErrorLayer;
    use tracing_subscriber::prelude::*;
    use tracing_subscriber::{fmt, EnvFilter};

    let filter_layer = EnvFilter::try_from_default_env().or_else(|_| EnvFilter::try_new("info"))?;

    tracing_subscriber::registry()
        .with(filter_layer)
        .with(fmt::layer())
        .with(ErrorLayer::default())
        .init();

    Ok(())
}

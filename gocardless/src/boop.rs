use clap::Parser;
use tracing::info;

#[derive(Debug, Parser)]
pub struct Cmd {
    #[clap(short = 's', long = "snoot", help = "Whether to boop the snoot or not")]
    snoot: bool,
}
impl Cmd {
    pub(crate) fn run(&self) -> color_eyre::Result<()> {
        info!(snoot=?self.snoot, "booping");
        if !self.snoot {
            Err(std::io::Error::other("must boop snoot"))?;
        }
        Ok(())
    }
}

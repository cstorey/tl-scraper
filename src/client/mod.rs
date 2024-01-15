mod authentication;
mod driver;

pub(crate) const REDIRECT_URI: &str = "https://console.truelayer.com/redirect-page";

pub use driver::{AccountsResult, CardsResult, Environment, TlClient};

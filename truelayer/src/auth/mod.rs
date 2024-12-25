use std::{net::IpAddr, sync::Arc};

use anyhow::{Context, Result};
use axum::{
    http::uri::{Scheme, Uri},
    response::{IntoResponse, Response},
    Router,
};
use tokio::net::TcpListener;
use tokio_util::sync::CancellationToken;
use tracing::{error, info};

use crate::{ClientCreds, Environment, ProviderConfig, TlClient};

mod start;

struct WebError(anyhow::Error);

type WebResult<T> = std::result::Result<T, WebError>;

pub async fn authenticate(
    client: &reqwest::Client,
    environment: Environment,
    provider: &ProviderConfig,
    client_creds: &ClientCreds,
    listen_port: u16,
) -> Result<()> {
    let cnx = CancellationToken::new();
    let tl = Arc::new(TlClient::new(
        client.clone(),
        environment,
        &provider.user_token,
        client_creds,
    ));

    let ip_addr = IpAddr::from([127, 0, 0, 1]);
    let listener = TcpListener::bind((ip_addr, listen_port))
        .await
        .with_context(|| format!("Bind to address: {}:{}", ip_addr, listen_port))?;

    let listen_address = listener.local_addr().context("listen address")?;
    let base_url = Uri::builder()
        .scheme(Scheme::HTTP)
        .authority(listen_address.to_string())
        .path_and_query("")
        .build()
        .context("Build base URI")?;
    let app = Router::new().merge(start::routes(cnx.clone(), tl.clone(), base_url));

    eprintln!("Please visit http://{}/", listen_address,);

    axum::serve(listener, app)
        .with_graceful_shutdown(cnx.clone().cancelled_owned())
        .await
        .context("Running server")?;
    info!("Done!");
    Ok(())
}

impl IntoResponse for WebError {
    fn into_response(self) -> Response {
        error!(error=?self.0, "Error handling request");
        "Error handling response".into_response()
    }
}

impl From<anyhow::Error> for WebError {
    fn from(value: anyhow::Error) -> Self {
        Self(value)
    }
}

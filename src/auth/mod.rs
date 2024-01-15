use std::{net::IpAddr, sync::Arc};

use anyhow::Context;
use axum::{
    debug_handler,
    response::{IntoResponse, Response},
    routing::get,
    Router,
};
use tokio::net::TcpListener;
use tokio_util::sync::CancellationToken;
use tracing::error;

use crate::TlClient;

struct WebError(anyhow::Error);

type Result<T> = std::result::Result<T, WebError>;

pub async fn authenticate(_client: Arc<TlClient>) -> anyhow::Result<()> {
    let cnx = CancellationToken::new();

    let app = Router::new().route("/", get(index));

    let listener = TcpListener::bind((IpAddr::from([127, 0, 0, 1]), 5500))
        .await
        .context("Bind to port")?;

    eprintln!(
        "Please visit http://{}/",
        listener.local_addr().context("listen address")?
    );

    axum::serve(listener, app)
        .with_graceful_shutdown(cnx.clone().cancelled_owned())
        .await
        .context("Running server")?;
    Ok(())
}

#[debug_handler]
async fn index() -> Result<impl IntoResponse> {
    Ok("Hi!")
}

impl IntoResponse for WebError {
    fn into_response(self) -> Response {
        error!(error=?self.0, "Error handling request");
        "Error handling response".into_response()
    }
}

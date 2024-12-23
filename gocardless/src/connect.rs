use std::{net::IpAddr, path::PathBuf};

use axum::{
    debug_handler,
    extract::{Query, State},
    http::{uri::Scheme, StatusCode, Uri},
    response::{IntoResponse, Response},
    routing::get,
    Router,
};
use clap::Parser;
use color_eyre::{eyre::Context, Result};
use serde::{Deserialize, Serialize};
use tokio::net::TcpListener;
use tokio_util::sync::CancellationToken;
use tracing::{debug, error, field, info, instrument, warn, Span};
use uuid::Uuid;

use crate::auth::load_token;

#[derive(Debug, Parser)]
pub struct Cmd {
    #[clap(short = 't', long = "token", help = "Token file")]
    token: PathBuf,
    #[clap(short = 'i', long = "institution", help = "Institution ID")]
    institution_id: String,
    #[clap(short = 'p', long = "port", help = "HTTP Listener port")]
    port: u16,
}

#[derive(Debug, Serialize)]
struct RequisitionReq {
    institution_id: String,
    redirect: String,
}
#[derive(Debug, Serialize, Deserialize)]
struct RequisitionResp {
    id: Uuid,
    link: String,
    #[serde(flatten)]
    other: serde_json::Value,
}

impl Cmd {
    #[instrument("auth", skip_all, fields(requisition_id))]
    pub(crate) async fn run(&self) -> Result<()> {
        let token = load_token(&self.token).await?;

        let client = reqwest::Client::new();

        let cnx = CancellationToken::new();
        let ip_addr = IpAddr::from([127, 0, 0, 1]);
        let listener = TcpListener::bind((ip_addr, self.port))
            .await
            .with_context(|| format!("Bind to address: {}:{}", ip_addr, self.port))?;

        let listen_address = listener.local_addr().context("listen address")?;
        let base_url = Uri::builder()
            .scheme(Scheme::HTTP)
            .authority(listen_address.to_string())
            .path_and_query("")
            .build()
            .context("Build base URI")?;

        let req = RequisitionReq {
            institution_id: self.institution_id.clone(),
            redirect: base_url.to_string(),
        };

        let requisition = client
            .post("https://bankaccountdata.gocardless.com/api/v2/requisitions/")
            .bearer_auth(&token.access)
            .json(&req)
            .send()
            .await?
            .error_for_status()?
            .json::<RequisitionResp>()
            .await?;

        Span::current().record("requisition_id", field::display(&requisition.id));

        debug!(
            "Got requisition: {}",
            serde_json::to_string_pretty(&requisition)?
        );

        let app = Router::new().merge(routes(cnx.clone(), requisition.id));

        println!("Go to link: {}", requisition.link);
        info!("Awaiting response");

        axum::serve(listener, app)
            .with_graceful_shutdown(cnx.clone().cancelled_owned())
            .await
            .context("Running server")?;

        Ok(())
    }
}

#[derive(Clone)]
struct AxumState {
    cnx: CancellationToken,
    expected_requisition_id: Uuid,
}

#[derive(Debug, Deserialize)]
struct RequisitionCallbackQuery {
    #[serde(rename = "ref")]
    id: Uuid,
}

struct WebError(color_eyre::Report);

type WebResult<T> = std::result::Result<T, WebError>;

fn routes(cnx: CancellationToken, expected_requisition_id: Uuid) -> Router {
    Router::new()
        .route("/", get(handle_redirect))
        .with_state(AxumState {
            cnx,
            expected_requisition_id,
        })
}

#[instrument(skip_all, fields(
    requisition_id=%q.id,
))]
#[debug_handler]
async fn handle_redirect(
    State(state): State<AxumState>,
    Query(q): Query<RequisitionCallbackQuery>,
) -> WebResult<Response> {
    if q.id != state.expected_requisition_id {
        warn!("Unexpected requisition token");
        return Ok((StatusCode::NOT_FOUND, "Unexpected requisition").into_response());
    }

    state.cnx.cancel();

    info!("Received confirmation");

    Ok((StatusCode::OK, "Ok").into_response())
}

impl IntoResponse for WebError {
    fn into_response(self) -> Response {
        error!(error=?self.0, "Error handling request");
        (StatusCode::INTERNAL_SERVER_ERROR, "Error handling response").into_response()
    }
}

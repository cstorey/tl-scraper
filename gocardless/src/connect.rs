use std::{fmt, net::IpAddr, path::PathBuf};

use axum::{
    debug_handler,
    extract::{Query, State},
    http::{header::CONTENT_TYPE, uri::Scheme, StatusCode, Uri},
    response::{IntoResponse, Response},
    routing::get,
    Router,
};
use clap::Parser;
use color_eyre::{
    eyre::{eyre, Context},
    Report, Result,
};
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
struct Requisition {
    id: Uuid,
    link: String,
    status: RequisitionStatus,
    #[serde(flatten)]
    other: serde_json::Value,
}

#[derive(Debug, Serialize, Deserialize)]
enum RequisitionStatus {
    // Requisition has been successfully created
    #[serde(rename = "CR")]
    Created,
    // End-user is giving consent at GoCardless's consent screen
    #[serde(rename = "GC")]
    GivingConsent,
    // End-user is redirected to the financial institution for authentication
    #[serde(rename = "UA")]
    UndergoingAuthentication,
    // Either SSN verification has failed or end-user has entered incorrect credentials
    #[serde(rename = "RJ")]
    Rejected,
    // End-user is selecting accounts
    #[serde(rename = "SA")]
    SelectingAccounts,
    // End-user is granting access to their account information
    #[serde(rename = "GA")]
    GrantingAccess,
    // Account has been successfully linked to requisition
    #[serde(rename = "LN")]
    Linked,
    // Access to accounts has expired as set in End User Agreement
    #[serde(rename = "EX")]
    Expired,
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
            .parse_error()
            .await?
            .json::<Requisition>()
            .await?;

        Span::current().record("requisition_id", field::display(&requisition.id));

        debug!(?requisition, "Got requisition",);

        let app = Router::new().merge(routes(cnx.clone(), requisition.id));

        println!("Go to link: {}", requisition.link);
        info!("Awaiting response");

        axum::serve(listener, app)
            .with_graceful_shutdown(cnx.clone().cancelled_owned())
            .await
            .context("Running server")?;

        let url = format!(
            "https://bankaccountdata.gocardless.com/api/v2/requisitions/{}/",
            requisition.id
        );
        debug!(?url, "Requisition URL");
        let requisition = client
            .get(url)
            .bearer_auth(&token.access)
            .send()
            .await?
            .parse_error()
            .await?
            .json::<serde_json::Value>()
            .await?;

        debug!(?requisition, "Got requisition",);

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

struct WebError(Report);

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

#[derive(Debug, Deserialize)]
struct ErrorResponse {
    summary: String,
    detail: String,
    status_code: u16,
}

trait RequestErrors: Sized {
    async fn parse_error(self) -> Result<Self>;
}

impl RequestErrors for reqwest::Response {
    async fn parse_error(self) -> Result<Self> {
        let resp = self;
        if resp.status().is_client_error() {
            match resp.headers().get(CONTENT_TYPE) {
                Some(content_type) if content_type.as_bytes() == b"application/json" => {
                    let err = resp.json::<ErrorResponse>().await?;
                    return Err(err.into());
                }
                Some(content_type) => {
                    warn!(?content_type, "unknown content type");
                    let status = resp.status();
                    let content = resp.text().await?;
                    debug!(?content, "Response body");
                    return Err(eyre!(
                        "Unrecognised response; code: {status}; content: {content:?}"
                    ));
                }
                None => {}
            }
        }
        Ok(resp.error_for_status()?)
    }
}

impl std::error::Error for ErrorResponse {
    fn description(&self) -> &str {
        &self.detail
    }
}

impl fmt::Display for ErrorResponse {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let ErrorResponse {
            summary,
            detail,
            status_code,
        } = self;
        write!(
            f,
            "Summary: {summary:?}; details: {detail:?}, status_code: {status_code:?}",
        )
    }
}

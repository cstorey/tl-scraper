use std::net::IpAddr;

use axum::{
    debug_handler,
    extract::{Query, State},
    http::{uri::Scheme, StatusCode, Uri},
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

use crate::{
    auth::AuthArgs,
    client::BankDataClient,
    config::{ConfigArg, ProviderState},
};

#[derive(Debug, Parser)]
pub struct Cmd {
    #[clap(flatten)]
    auth: AuthArgs,
    #[clap(flatten)]
    config: ConfigArg,
    #[clap(short = 'p', long = "provider", help = "Provider name")]
    provider: String,
    #[clap(short = 'l', long = "port", help = "HTTP Listener port")]
    port: u16,
}

#[derive(Debug, Serialize)]
struct RequisitionReq {
    institution_id: String,
    redirect: String,
}
#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct Requisition {
    pub(crate) id: Uuid,
    pub(crate) link: String,
    pub(crate) status: RequisitionStatus,
    pub(crate) accounts: Vec<Uuid>,
    #[serde(flatten)]
    pub(crate) other: serde_json::Value,
}

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub(crate) enum RequisitionStatus {
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
    #[instrument("auth", skip_all, fields(provider = %self.provider, institution_id, requisition_id))]
    pub(crate) async fn run(&self) -> Result<()> {
        let config = self.config.load().await?;
        let token = self.auth.load_token().await?;

        let Some(provider_config) = config.provider.get(&self.provider) else {
            return Err(eyre!("Unrecognised provider: {}", self.provider));
        };

        Span::current().record("institution_id", &provider_config.institution_id);

        let client = BankDataClient::new(token);

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
            institution_id: provider_config.institution_id.clone(),
            redirect: base_url.to_string(),
        };

        let requisition = client
            .post::<Requisition>("/api/v2/requisitions/", &req)
            .await?;

        Span::current().record("requisition_id", field::display(&requisition.id));

        debug!(?requisition, "Got requisition");

        let app = Router::new().merge(routes(cnx.clone(), requisition.id));

        println!("Go to link: {}", requisition.link);
        info!("Awaiting response");

        axum::serve(listener, app)
            .with_graceful_shutdown(cnx.clone().cancelled_owned())
            .await
            .context("Running server")?;

        let requisition = client
            .get::<Requisition>(&format!("/api/v2/requisitions/{}/", requisition.id))
            .await?;

        debug!(?requisition, "Got requisition",);

        let state = ProviderState::from_requisition(&requisition);

        provider_config.write_state(&state).await?;

        Ok(())
    }
}

impl Requisition {
    pub(crate) fn is_linked(&self) -> bool {
        self.status == RequisitionStatus::Linked
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

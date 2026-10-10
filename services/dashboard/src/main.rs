// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
mod feeder;
mod registry;

use std::error::Error;
use std::path::{Path as FsPath, PathBuf};

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::{Html, IntoResponse, Response};
use axum::routing::{get, post};
use axum::Json;
use serde::{Deserialize, Serialize};
use tower_http::cors::CorsLayer;

use common_lib::{AgentName, StatePoint};
use feeder::FeederManager;
use registry::{MetricRegistry, RegisterMetricRequest, RegisteredMetric};
use timeline_client::{ClientError, GolemCli, TimelineClient};

#[derive(Clone)]
struct AppState {
    registry: MetricRegistry,
    feeder: FeederManager,
    client: TimelineClient,
}

/// The Golem application directory: `$GOLEM_APP_DIR`, or the workspace root.
fn golem_app_dir() -> PathBuf {
    std::env::var_os("GOLEM_APP_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            let manifest_dir = FsPath::new(env!("CARGO_MANIFEST_DIR"));
            manifest_dir
                .ancestors()
                .nth(2)
                .unwrap_or(manifest_dir)
                .to_path_buf()
        })
}

/// An error rendered as `{"error": "..."}`.
struct ApiError {
    status: StatusCode,
    message: String,
}

impl From<ClientError> for ApiError {
    fn from(e: ClientError) -> Self {
        ApiError {
            status: StatusCode::BAD_GATEWAY,
            message: e.to_string(),
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (
            self.status,
            Json(serde_json::json!({ "error": self.message })),
        )
            .into_response()
    }
}

/// A timeline query result: the state point at the queried time, if any.
#[derive(Serialize)]
struct TimelineResult {
    results: Vec<StatePoint>,
}

impl From<Option<StatePoint>> for TimelineResult {
    fn from(point: Option<StatePoint>) -> Self {
        TimelineResult {
            results: point.into_iter().collect(),
        }
    }
}

/// Path parameters of a point-in-time query: `/{agent}/{t1}`.
#[derive(Deserialize)]
struct PointQuery {
    agent: AgentName,
    t1: u64,
}

/// Runs a blocking client call off the async runtime.
async fn blocking<T: Send + 'static>(
    call: impl FnOnce() -> Result<T, ClientError> + Send + 'static,
) -> Result<T, ApiError> {
    tokio::task::spawn_blocking(call)
        .await
        .map_err(|e| ApiError {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            message: e.to_string(),
        })?
        .map_err(ApiError::from)
}

async fn index() -> Html<&'static str> {
    Html(include_str!("dashboard.html"))
}

async fn aggregation(
    State(state): State<AppState>,
    Path(agent): Path<AgentName>,
) -> Result<Json<common_lib::AggregationResult>, ApiError> {
    let client = state.client;
    blocking(move || client.get_aggregation_result(&agent))
        .await
        .map(Json)
}

async fn leaf_result(
    State(state): State<AppState>,
    Path(query): Path<PointQuery>,
) -> Result<Json<TimelineResult>, ApiError> {
    let client = state.client;
    blocking(move || client.get_leaf_result(&query.agent, query.t1))
        .await
        .map(|point| Json(point.into()))
}

async fn derived_result(
    State(state): State<AppState>,
    Path(query): Path<PointQuery>,
) -> Result<Json<TimelineResult>, ApiError> {
    let client = state.client;
    blocking(move || client.get_derived_result(&query.agent, query.t1))
        .await
        .map(|point| Json(point.into()))
}

async fn register_metric(
    State(state): State<AppState>,
    Json(req): Json<RegisterMetricRequest>,
) -> Result<Json<RegisteredMetric>, ApiError> {
    let registration = state.registry.register(&req).map_err(|message| ApiError {
        status: StatusCode::BAD_REQUEST,
        message,
    })?;
    state.feeder.start_feeder(&registration);
    Ok(Json(registration.metric))
}

async fn list_metrics(
    State(state): State<AppState>,
) -> Result<Json<Vec<RegisteredMetric>>, ApiError> {
    state.registry.list().map(Json).map_err(|message| ApiError {
        status: StatusCode::INTERNAL_SERVER_ERROR,
        message,
    })
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let kafka_broker =
        std::env::var("KAFKA_BROKER").unwrap_or_else(|_| "localhost:9092".to_string());
    let kafka_topic =
        std::env::var("KAFKA_TOPIC").unwrap_or_else(|_| "timeline-events".to_string());

    let client = TimelineClient::new(GolemCli::new(golem_app_dir()));
    let state = AppState {
        registry: MetricRegistry::new(),
        feeder: FeederManager::new(kafka_broker, kafka_topic, client.clone()),
        client,
    };

    let app = axum::Router::new()
        .route("/", get(index))
        .route("/api/aggregation/{agent}", get(aggregation))
        .route("/api/leaf/{agent}/{t1}", get(leaf_result))
        .route("/api/derived/{agent}/{t1}", get(derived_result))
        .route("/api/metrics", post(register_metric))
        .route("/api/metrics", get(list_metrics))
        .layer(CorsLayer::permissive())
        .with_state(state);

    let port: u16 = match std::env::var("DASHBOARD_PORT") {
        Ok(value) => value.parse()?,
        Err(_) => 3000,
    };
    let listener = tokio::net::TcpListener::bind(("0.0.0.0", port)).await?;
    println!("Dashboard running at http://localhost:{port}");
    axum::serve(listener, app).await?;
    Ok(())
}

use axum::{extract::State, response::IntoResponse, routing::get, Json, Router};
use std::sync::Arc;
use tokio::sync::RwLock;
use tower_http::{compression::CompressionLayer, services::ServeDir, cors::CorsLayer};

use crate::types::{DataState, Kpis};

#[derive(Clone)]
pub struct ApiState { pub data: Arc<RwLock<DataState>> }

pub fn router(state: ApiState) -> Router {
    Router::new()
    .route("/health", get(|| async { "ok" }))
    .route("/kpis", get(kpis))
    .route("/map/hex", get(map_hex))
    .fallback_service(ServeDir::new("web"))
    .with_state(state)
    .layer(CorsLayer::permissive())
    .layer(CompressionLayer::new())
}

async fn kpis(State(st): State<ApiState>) -> impl IntoResponse {
    let d = st.data.read().await;
    Json(d.kpis.clone())
}

async fn map_hex(State(st): State<ApiState>) -> impl IntoResponse {
    let d = st.data.read().await;
    let s = d.hex_geojson_str.clone();
    ([("content-type","application/json")], s)
}
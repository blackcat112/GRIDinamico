//! api.rs — Rutas HTTP: /health, /kpis, /map/hex y /orders/filter

use axum::{
    extract::State,
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use axum::body::Body;
use axum::http::{header::CONTENT_TYPE, StatusCode};
use serde::Serialize;
use std::{sync::Arc};
use tokio::sync::RwLock;
use tower_http::{compression::CompressionLayer, services::ServeDir, cors::CorsLayer};

use crate::{clusterizador::global_orders, models::types::DataState};

#[derive(Clone)]
pub struct ApiState {
    pub data: Arc<RwLock<DataState>>,
}

pub fn router(state: ApiState) -> Router {
    Router::new()
        .route("/health", get(|| async { "ok" }))
        .route("/map/hex", get(get_hex_geojson))
        .route("/kpis", get(get_kpis))
        .route("/orders/filter", post(global_orders))
        .fallback_service(ServeDir::new("web"))
        .with_state(state)
        .layer(CorsLayer::permissive())
        .layer(CompressionLayer::new())
}

/// Devuelve el GeoJSON actual con content-type correcto.
async fn get_hex_geojson(State(state): State<ApiState>) -> Response {
    let d = state.data.read().await;
    let body = d.hex_geojson.clone();

    if body.is_empty() {
        // 204 sin cuerpo
        return Response::builder()
            .status(StatusCode::NO_CONTENT)
            .body(Body::empty())
            .unwrap();
    }

    Response::builder()
        .status(StatusCode::OK)
        .header(CONTENT_TYPE, "application/geo+json; charset=utf-8")
        .body(Body::from(body))
        .unwrap()
}

/// KPI sencillos: timestamp y conteo aproximado de features.
#[derive(Serialize)]
struct Kpis {
    snapshot_ts_utc: String,
    features: usize,
    geojson_bytes: usize,
}

async fn get_kpis(State(state): State<ApiState>) -> impl IntoResponse {
    let d = state.data.read().await;
    let gj = d.hex_geojson.as_str();
    // Conteo simple sin parsear: cuenta ocurrencias de `"type":"Feature"`
    let features = gj.matches("\"type\":\"Feature\"").count();
    let out = Kpis {
        snapshot_ts_utc: d.snapshot_ts_utc.clone(),
        features,
        geojson_bytes: gj.len(),
    };
    Json(out)
}

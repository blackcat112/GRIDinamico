//! api.rs
//! Rutas HTTP: /health, /kpis, /map/hex y /routing/cells (ligera para ruteo con H3)

use axum::{extract::{Query, State}, response::IntoResponse, routing::get, Json, Router, http::header};
use std::sync::Arc;
use tokio::sync::RwLock;
use tower_http::{compression::CompressionLayer, services::ServeDir, cors::CorsLayer};
use serde::Deserialize;
use crate::types::{DataState, RoutingCell};

#[derive(Clone)]
pub struct ApiState { pub data: Arc<RwLock<DataState>> }

pub fn router(state: ApiState) -> Router {
    Router::new()
        .route("/health", get(|| async { "ok" }))
        .route("/kpis", get(kpis))
        .route("/map/hex", get(map_hex))
        .route("/routing/cells", get(routing_cells))
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
    let body = {
        let d = st.data.read().await;
        d.hex_geojson.clone()
    };
    ([(header::CONTENT_TYPE, "application/geo+json")], body)
}

/// Query de /routing/cells
#[derive(Debug, Deserialize)]
#[allow(dead_code)]
pub struct RoutingQuery {
    /// Resolución deseada 
    pub res: Option<u8>,
    /// Umbral  de delay para incluir celda (default 1.03)
    pub min_delay: Option<f32>,
    /// bbox=minLon,minLat,maxLon,maxLat  
    pub bbox: Option<String>,
    /// refine=true para usar mix parent/hijos 
    pub refine: Option<bool>,
    /// k (0..2) suavizado k-ring; default 1
    pub k: Option<u32>,
}

/// Export  [{h3, delay}]
async fn routing_cells(State(st): State<ApiState>, Query(_q): Query<RoutingQuery>) -> impl IntoResponse {
    let d = st.data.read().await;
    Json::<Vec<RoutingCell>>(d.routing_cells.clone())
}

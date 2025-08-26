//! api.rs
//! Rutas HTTP: /health, /kpis, /map/hex y /routing/cells (ligera para ruteo con H3).

use axum::{extract::{Query, State}, response::IntoResponse, routing::get, Json, Router};
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
    let d = st.data.read().await;
    let s = d.hex_geojson_str.clone();
    ([("content-type","application/json")], s)
}

/// Query de /routing/cells
#[derive(Debug, Deserialize)]
pub struct RoutingQuery {
    /// Resolución deseada (si recalculamos on-demand). Si omitido, usamos el snapshot cacheado.
    pub res: Option<u8>,
    /// Umbral mínimo de delay para incluir celda (default 1.03)
    pub min_delay: Option<f32>,
    /// bbox=minLon,minLat,maxLon,maxLat  (opcional, recorta la respuesta)
    pub bbox: Option<String>,
    /// refine=true para usar mix parent/hijos (si snapshot lo soporta)
    pub refine: Option<bool>,
    /// k (0..2) suavizado k-ring; si snapshot no se generó así, podemos recalcular on-demand.
    pub k: Option<u32>,
}

/// Export ligero para ruteo: [{h3, delay}]
async fn routing_cells(State(st): State<ApiState>, Query(_q): Query<RoutingQuery>) -> impl IntoResponse {
    // Servimos el snapshot pre-generado (rápido)
    // Si quieres recalcular on-demand con los parámetros, mueve la lógica a aquí usando h3grid::recompute_h3(...)
    let d = st.data.read().await;
    Json::<Vec<RoutingCell>>(d.routing_cells.clone())
}

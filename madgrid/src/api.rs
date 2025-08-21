use axum::{extract::State, response::IntoResponse, routing::get, Json, Router};
use std::sync::Arc;
use tokio::sync::RwLock;
use tower_http::{compression::CompressionLayer, cors::CorsLayer, services::ServeDir};

use geo::CoordsIter;           // para .coords()
use serde::Serialize;          // para serializar structs
use std::collections::HashMap; // id -> delay_factor

use crate::types::{DataState, Kpis};

#[derive(Clone)]
pub struct ApiState {
    pub data: Arc<RwLock<DataState>>,
    pub grid: Arc<crate::grid::GridIndex>,
}

pub fn router(state: ApiState) -> Router {
    Router::new()
        .route("/health", get(|| async { "ok" }))
        .route("/kpis", get(kpis))
        .route("/map/hex", get(map_hex))
        .route("/export/hex-df.json", get(export_hex_df)) // <-- NUEVO
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
    ([("content-type", "application/json")], s)
}

#[derive(Serialize)]
struct HexDf {
    hex_id: u32,
    delay_factor: f32,          // 999.0 si bloqueado/sin dato
    coordinates: Vec<[f64; 2]>, // anillo exterior (lon, lat) WGS84
}

// GET /export/hex-df.json  -> JSON minimalista para optimizador de rutas
// GET /export/hex-df.json  -> SOLO los que se pintan en el mapa
async fn export_hex_df(State(st): State<ApiState>) -> impl IntoResponse {
    let d = st.data.read().await;

    // Umbral igual que el mapa
    let eps = d.delay_cfg.show_eps;

    // id -> delay_factor, filtrando SOLO df > 1 + eps
    let mut df_by_id: std::collections::HashMap<u32, f32> =
        std::collections::HashMap::with_capacity(d.cells_out.len());

    for c in &d.cells_out {
        let df = c.delay_factor.unwrap_or(999.0);
        if df > 1.0 + eps {
            df_by_id.insert(c.id, (df * 100.0).round() / 100.0);
        }
    }

    // Construye salida SOLO para ids filtrados
    let mut out: Vec<HexDf> = Vec::with_capacity(df_by_id.len());
    for cell in &st.grid.cells {
        if let Some(df) = df_by_id.get(&cell.id) {
            let exterior: Vec<[f64; 2]> =
                cell.poly.exterior().coords().map(|c| [c.x, c.y]).collect();
            out.push(HexDf {
                hex_id: cell.id,
                delay_factor: *df,
                coordinates: exterior,
            });
        }
    }

    let body = serde_json::to_string(&out).unwrap_or_else(|_| "[]".into());
    ([("content-type", "application/json")], body)
}


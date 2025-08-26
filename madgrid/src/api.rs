use axum::{extract::State, response::IntoResponse, routing::get, Json, Router};
use std::sync::Arc;
use tokio::sync::RwLock;
use tower_http::{compression::CompressionLayer, services::ServeDir, cors::CorsLayer};
use serde::Serialize;

use crate::types::DataState;

#[derive(Clone)]
pub struct ApiState { pub data: Arc<RwLock<DataState>> }

#[derive(Serialize)]
struct RoutingCellOut {
    h3: String,
    delay: f32,
    // si quieres mandar centro también, descomenta:
    // lat: f32,
    // lon: f32,
}

pub fn router(state: ApiState) -> Router {
    Router::new()
        .route("/health", get(|| async { "ok" }))
        .route("/kpis", get(kpis))
        .route("/map/hex", get(map_hex))
        .route("/routing/cells", get(routing_cells)) // ⬅️ nuevo
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

async fn routing_cells(State(st): State<ApiState>) -> impl IntoResponse {
    let d = st.data.read().await;

    // IMPORTANTE:
    // - Para que esto funcione, en la parte H3 tienes que estar guardando en `cells_out`
    //   el índice H3 en `id` (o cambia `.to_string()` según tu struct).
    // - Si sigues con el grid "antiguo" (u32), entonces no tendrás el H3 aquí;
    //   en ese caso usa la Opción B de abajo.

    let eps = d.delay_cfg.show_eps;
    let items: Vec<RoutingCellOut> = d.cells_out.iter()
        .filter_map(|c| {
            // delay_factor None -> descartar, <= 1+eps -> descartar
            let mut df = c.delay_factor?;
            if c.blocked { df = 999.0; }
            if df <= 1.0 + eps && !c.blocked { return None; }
            // OJO: aquí asumo que `c.id` es el H3 en String (o cambia esto a como lo guardas)
            Some(RoutingCellOut {
                h3: c.id.to_string(), // <-- si `id` ya es String, deja `c.id.clone()`
                delay: ((df * 100.0).round() / 100.0),
            })
        })
        .collect();

    Json(items)
}

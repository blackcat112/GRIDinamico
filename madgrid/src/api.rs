//! api.rs
//! Rutas HTTP: /health, /kpis, /map/hex y /routing/cells (ligera para ruteo con H3)

use axum::{extract::{Query, State}, response::IntoResponse, routing::get, Json, Router};
use std::sync::Arc;
use tokio::sync::RwLock;
use tower_http::{compression::CompressionLayer, services::ServeDir, cors::CorsLayer};
use crate::types::{DataState, RoutingCell};
use crate::types::Kpis;
use once_cell::sync::Lazy;
use std::collections::HashMap;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};
use h3o::CellIndex;
use std::str::FromStr;



static GROUPS: Lazy<RwLock<HashMap<String, i64>>> = Lazy::new(|| RwLock::new(HashMap::new()));

#[derive(Deserialize)]
struct CityQ {
    city: Option<String>,
}

#[derive(Deserialize)]
struct InFeature { properties: Map<String, Value> }

#[derive(Deserialize)]
struct InFC { features: Vec<InFeature> }

#[derive(Serialize)]
struct GroupResp { inserted: usize }

#[derive(Clone)]
pub struct ApiState { pub data: Arc<RwLock<DataState>> }

pub fn router(state: ApiState) -> Router {
    Router::new()
        .route("/health", get(|| async { "ok" }))
        .route("/kpis", get(kpis))
        .route("/map/hex", get(map_hex))
        .route("/routing/cells", get(routing_cells))
        .route("/groups", get(get_groups).post(save_groups))


        // .route("/groups", get(groups)) 
        .fallback_service(ServeDir::new("web"))
        .with_state(state)
        .layer(CorsLayer::permissive())
        .layer(CompressionLayer::new())
}

fn cell_polygon_coords(c: CellIndex) -> Vec<[f64; 2]> {
    let verts = c.boundary();
    let mut coords: Vec<[f64; 2]> = verts
        .iter()
        .map(|ll| [ll.lng(), ll.lat()]) // ya en grados
        .collect();

    if let Some(first) = coords.first().cloned() {
        if coords.last() != Some(&first) {
            coords.push(first);
        }
    }
    coords
}

async fn kpis(State(st): State<ApiState>, Query(q): Query<CityQ>) -> impl IntoResponse {
    if let Some(c) = q.city.as_deref() {
        if c.eq_ignore_ascii_case("zgz") {
            // crea un Kpis vacío
            return Json(Kpis { carga: 0, inc: 0 });
        }
        if c.eq_ignore_ascii_case("lg") {
            // crea un Kpis vacío
            return Json(Kpis { carga: 0, inc: 0 });
        }
        if c.eq_ignore_ascii_case("madC") {
            // crea un Kpis vacío
            return Json(Kpis { carga: 0, inc: 0 });
        }
    }
    let d = st.data.read().await;
    Json(d.kpis.clone())
}


async fn save_groups(
    Query(params): Query<HashMap<String, String>>,
    Json(fc): Json<InFC>,
) -> Json<GroupResp> {
    let city = params.get("city").cloned().unwrap_or_else(|| "zgz".to_string());
    let mut ok = 0usize;
    let mut w = GROUPS.write().await;

    for f in fc.features {
        let h3 = f.properties.get("h3").and_then(|v| v.as_str());
        let g  = f.properties.get("grupo");
        if let (Some(h3id), Some(gv)) = (h3, g) {
            let gnum = gv.as_i64()
                .or_else(|| gv.as_str().and_then(|s| s.parse::<i64>().ok()))
                .unwrap_or(0);
            w.insert(format!("{}:{}", city, h3id), gnum);
            ok += 1;
        }
    }
    Json(GroupResp { inserted: ok })
}

async fn get_groups(Query(params): Query<HashMap<String, String>>) -> Json<Value> {
    let city = params.get("city").cloned().unwrap_or_else(|| "zgz".to_string());
    let r = GROUPS.read().await;

    // H3 -> coordinates (anillo exterior dentro del array de polígonos: [exterior])
    let coords_for = |h3id: &str| -> Option<Value> {
        CellIndex::from_str(h3id).ok().map(|cell| {
            let exterior = cell_polygon_coords(cell); // [[lng,lat], ...] anillo cerrado
            json!([exterior])
        })
    };

    if let Some(h3id) = params.get("h3").cloned() {
        let key = format!("{}:{}", &city, &h3id);
        let grupo = r.get(&key).cloned();      // Option<_>
        let coordinates = coords_for(&h3id);   // Option<Value>
        return Json(json!({
            "city": city,
            "h3": h3id,
            "grupo": grupo,
            "coordinates": coordinates
        }));
    }

    let prefix = format!("{}:", &city);
    let groups: Vec<Value> = r.iter()
        .filter_map(|(k, v)| k.strip_prefix(&prefix).map(|h3| (h3.to_string(), *v)))
        .map(|(h3, gnum)| {
            json!({
                "h3": h3,
                "grupo": gnum,
                "coordinates": coords_for(&h3)
            })
        })
        .collect();

    Json(json!({
        "city": city,
        "count": groups.len(),
        "groups": groups
    }))
}

async fn map_hex(State(st): State<ApiState>, Query(q): Query<CityQ>) -> impl IntoResponse {
    // Zaragoza: SIEMPRE usar H3 (ignoramos grid)
    if q.city.as_deref().map(|c| c.eq_ignore_ascii_case("zgz")).unwrap_or(false) {
        let gj_str = crate::h3grid::geojson_zaragoza_mesh(); // <- tu nueva función H3
        let v: Value = serde_json::from_str(&gj_str).unwrap_or(serde_json::json!({
            "type":"FeatureCollection","features":[]
        }));
        return Json(v);
    }

    if q.city.as_deref().map(|c| c.eq_ignore_ascii_case("lg")).unwrap_or(false) {
        let gj_str = crate::h3grid::geojson_logrono_mesh(); // <- tu nueva función H3
        let v: Value = serde_json::from_str(&gj_str).unwrap_or(serde_json::json!({
            "type":"FeatureCollection","features":[]
        }));
        return Json(v);
    }

    if q.city.as_deref().map(|c| c.eq_ignore_ascii_case("madC")).unwrap_or(false) {
        let gj_str = crate::h3grid::geojson_madC_mesh(); // <- tu nueva función H3
        let v: Value = serde_json::from_str(&gj_str).unwrap_or(serde_json::json!({
            "type":"FeatureCollection","features":[]
        }));
        return Json(v);
    }


    // Madrid (lo que ya tenías)
    let body = {
        let d = st.data.read().await;
        d.hex_geojson.clone()
    };
    let v: Value = serde_json::from_str(&body).unwrap_or(serde_json::json!({
        "type":"FeatureCollection","features":[]
    }));
    Json(v)
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

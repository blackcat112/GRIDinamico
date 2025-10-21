
//! Agrupación de pedidos sobre celdas S2 (sin overlapping), versión compatible con s2 = 0.0.13

use axum::Json;
use serde_json::json;
use std::collections::HashMap;
use s2::cellid::CellID;
use s2::cell::Cell;
use s2::latlng::LatLng;
use s2::point::Point;

use crate::models::types::PedidoPoints;

/// Convierte lat/lon (grados) en una celda S2 con nivel determinado
#[inline]
fn s2_cell(lat: f64, lon: f64, level: u8) -> CellID {
    let ll = LatLng::from_degrees(lat, lon);
    let point = Point::from(&ll);
    let cell = CellID::from(point);
    cell.parent(level as u64)
}

/// Obtiene los vertices del poligono de una celda S2
fn cell_vertices(cell: &CellID) -> Vec<[f64; 2]> {
    let s2cell = Cell::from(cell.clone());
    let mut coords = Vec::new();
    for v in 0..4 {
        let vert = s2cell.vertex(v);
        let ll = LatLng::from(&vert);

        // En s2 0.0.13, ll.lat y ll.lng son de tipo Angle { rad: f64 }
        let lat_deg = ll.lat.rad().to_degrees();
        let lng_deg = ll.lng.rad().to_degrees();
        

        coords.push([lng_deg, lat_deg]);
    }
    if let Some(first) = coords.first().copied() {
        coords.push(first);
    }
    coords
}

/// API: agrupacion de pedidos usando S2 (sin overlapping)
pub async fn global_orders(Json(pedidos): Json<PedidoPoints>) -> Json<serde_json::Value> {
    let l6 = 10u8;  // ~4 km 
    let l7 = 12u8;  // ~1 km
    let l8 = 14u8;  // ~250 m
    let l9 = 16u8;  // ~60 m

    let max_l6 = if pedidos.veh == "bike" { 19 } else if pedidos.veh == "car" { 24 } else { 19 };
    let max_l7 = 24;
    let max_l8 = 24;

    // Conteo inicial en nivel base (l6)
    let mut counts: HashMap<CellID, usize> = HashMap::new();
    for (lon, lat) in &pedidos.points {
        let cell = s2_cell(*lat, *lon, l6);
        *counts.entry(cell).or_insert(0) += 1;
    }

    // Pila de subdivisiones
    let mut pending: Vec<(CellID, u8)> = Vec::new();
    for (&cell, &count) in counts.iter() {
        if count > max_l6 {
            pending.push((cell, l7));
        }
    }

    // Subdivisión jerarquica
    while let Some((parent, level)) = pending.pop() {
        counts.remove(&parent);

        // Contar puntos en hijas
        let mut child_counts: HashMap<CellID, usize> = HashMap::new();
        for (lon, lat) in &pedidos.points {
            let ll = LatLng::from_degrees(*lat, *lon);
            let point = Point::from(&ll);
            let point_cell = CellID::from(point);
            if point_cell.parent((level - 2) as u64) == parent {
                let child = point_cell.parent(level as u64);
                *child_counts.entry(child).or_insert(0) += 1;
            }
        }

        // Evaluar hijas
        for (child, count) in child_counts {
            let next = match level {
                x if x == l7 && count > max_l7 => Some(l8),
                x if x == l8 && count > max_l8 => Some(l9),
                _ => None,
            };
            if let Some(next_level) = next {
                pending.push((child, next_level));
            } else {
                counts.insert(child, count);
            }
        }
    }

    // Construir GeoJSON
    let mut features = Vec::new();
    for (cell, count) in &counts {
        let coords = cell_vertices(cell);
        features.push(json!({
            "type": "Feature",
            "geometry": { "type": "Polygon", "coordinates": [coords] },
            "properties": {
                "s2_cell": cell.to_token(),
                "pedidos": count,
                "vehicle_type": pedidos.veh,
                "level": cell.level(),
            }
        }));
    }

    let gj = json!({
        "type": "FeatureCollection",
        "name": "orders_s2_zones",
        "crs": { "type": "name", "properties": { "name": "EPSG:4326" }},
        "features": features
    });

    Json(gj)
}

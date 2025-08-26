//! grid.rs
//!
//! Maneja el grid hexagonal y los cálculos asociados.
//!
//! Funcionalidad principal:
//! - Cargar un GeoJSON con las celdas hexagonales.
//! - Construir un índice espacial (`RTree`) para localizar rápidamente celdas por coordenadas.
//! - Implementar `cell_for()` para encontrar en qué hexágono cae un punto.
//! - Calcular indicadores de tráfico, incidencias y proximidad de aparcamientos.
//! - Generar `delay_factor` por celda y un GeoJSON coloreado.
//!
//! Este módulo concentra la "lógica de negocio" del proyecto,
//! donde se transforma la información en métricas útiles.



use anyhow::Result;
use geo::{algorithm::centroid::Centroid, Contains, Polygon, BoundingRect};
use geojson::{GeoJson, Geometry, Value};
use rstar::{RTree, RTreeObject, AABB};
use serde_json::json;

use crate::types::{DelayCfg, Incidencia, ParkingZone, SensorTr};


#[derive(Clone)]
pub struct HexCell {
    pub id: u32,
    pub poly: Polygon,            // Polígono en WGS84
    pub centroid: (f32, f32),     // (lat, lon)
    pub aabb: AABB<[f32; 2]>,     // bbox para RTree (lon,lat)
}

#[derive(Clone)]
pub struct GridIndex {
    pub cells: Vec<HexCell>,
    pub(crate) tree: RTree<GridItem>,
}

#[derive(Clone, Copy)]
pub struct GridItem {
    pub id: u32,
    pub aabb: AABB<[f32; 2]>,
}

impl RTreeObject for GridItem {
    type Envelope = AABB<[f32; 2]>;
    fn envelope(&self) -> Self::Envelope { self.aabb }
}

impl GridIndex {
    pub fn from_geojson(path: &str) -> Result<Self> {
        let text = std::fs::read_to_string(path)?;
        let gj: GeoJson = text.parse()?;
        let fc = match gj {
            GeoJson::FeatureCollection(fc) => fc,
            _ => anyhow::bail!("GeoJSON debe ser FeatureCollection"),
        };
        let mut cells: Vec<HexCell> = Vec::new();
        let mut items: Vec<GridItem> = Vec::new();
        for (idx, feat) in fc.features.into_iter().enumerate() {
            if let Some(geom) = feat.geometry {
                if let Some((poly, rect)) = polygon_from_geometry(&geom) {
                    let cent = poly.centroid().map(|p| (p.y() as f32, p.x() as f32)).unwrap_or((0.0, 0.0));
                    let id = idx as u32;
                    let aabb = AABB::from_corners(
                        [rect.min().x as f32, rect.min().y as f32],
                        [rect.max().x as f32, rect.max().y as f32],
                    );
                    cells.push(HexCell { id, poly, centroid: cent, aabb });
                    items.push(GridItem { id, aabb });
                }
            }
        }
        let tree = RTree::bulk_load(items);
        Ok(Self { cells, tree })
    }

    pub fn cell_for(&self, lon: f32, lat: f32) -> Option<u32> {
        let p = [lon, lat];
        for it in self.tree.locate_in_envelope_intersecting(&AABB::from_point(p)) {
            let cell = &self.cells[it.id as usize];
            if cell.poly.contains(&geo::Point::new(lon as f64, lat as f64)) {
                return Some(cell.id);
            }
        }
        None
    }
}

fn polygon_from_geometry(g: &Geometry) -> Option<(Polygon, geo::Rect)> {
    match &g.value {
        Value::Polygon(coords) => {
            let exterior: Vec<_> = coords[0].iter().map(|c| (c[0], c[1])).collect();
            let poly: Polygon = Polygon::new(exterior.into(), vec![]);
            let rect = poly.bounding_rect()?;
            Some((poly, rect))
        }
        Value::MultiPolygon(multi) => {
            if let Some(first) = multi.get(0) {
                let exterior: Vec<_> = first[0].iter().map(|c| (c[0], c[1])).collect();
                let poly: Polygon = Polygon::new(exterior.into(), vec![]);
                let rect = poly.bounding_rect()?;
                Some((poly, rect))
            } else {
                None
            }
        }
        _ => None,
    }
}

#[inline]
fn clamp(x: f32, a: f32, b: f32) -> f32 { x.max(a).min(b) }

#[inline]
fn haversine_m(lat1: f32, lon1: f32, lat2: f32, lon2: f32) -> f32 {
    let r = 6371000.0_f32;
    let dlat = (lat2 - lat1).to_radians();
    let dlon = (lon2 - lon1).to_radians();
    let a = (dlat / 2.0).sin().powi(2)
        + lat1.to_radians().cos() * lat2.to_radians().cos() * (dlon / 2.0).sin().powi(2);
    2.0 * r * a.sqrt().atan2((1.0 - a).sqrt())
}

/// Recalcula KPIs por celda y genera GeoJSON de mapa.
/// - Si `blocked` = true → `delay_factor = 999.0`


fn color_from_norm(x: f32) -> String {
    const RAMP: [&str; 11] = [
        "#e9f7ef", "#d4f2e3", "#bfeacc", "#a9e3b6", "#fff3b0",
        "#ffe08a", "#ffc266", "#ff9f58", "#ff7a55", "#f5544f", "#d73a49",
    ];
    let i = (x.clamp(0.0, 1.0) * ((RAMP.len() - 1) as f32)).floor() as usize;
    RAMP[i].to_string()
}

fn cell_to_geometry(cell: &HexCell) -> serde_json::Value {
    // exterior (lon, lat)
    let exterior: Vec<[f64; 2]> = cell.poly.exterior().coords().map(|c| [c.x, c.y]).collect();
    json!({ "type": "Polygon", "coordinates": [exterior] })
}

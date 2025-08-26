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


use crate::types::{CellOut, DelayCfg, Incidencia, ParkingZone, SensorTr};

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
/// - GeoJSON sólo incluye celdas con `delay > 1 + show_eps` (para pintar el mapa)
pub fn recompute(
    grid: &GridIndex,
    cargas: &[ParkingZone],
    incs: &[Incidencia],
    traf: &[SensorTr],
    cfg: &DelayCfg,
) -> (Vec<CellOut>, serde_json::Value) {
    use std::collections::HashMap;

    // indexar por celda
    let mut inc_by: HashMap<u32, Vec<&Incidencia>> = HashMap::new();
    let mut tr_by: HashMap<u32, Vec<&SensorTr>> = HashMap::new();

    for inc in incs {
        if let Some(id) = grid.cell_for(inc.lon, inc.lat) { inc_by.entry(id).or_default().push(inc); }
    }
    for s in traf {
        if let Some(id) = grid.cell_for(s.lon, s.lat) { tr_by.entry(id).or_default().push(s); }
    }

    let mut outs: Vec<CellOut> = Vec::with_capacity(grid.cells.len());
    let mut features = Vec::new();

    for cell in &grid.cells {
        let incs_c = inc_by.get(&cell.id).map(|v| v.as_slice()).unwrap_or(&[]);
        let tr_c = tr_by.get(&cell.id).map(|v| v.as_slice()).unwrap_or(&[]);

        // Trafico: medias/medianas simples
        let n = tr_c.len() as f32;
        let (mut carga_avg, mut nivel_avg, mut ocup_avg, mut vel_med) = (0.0, 0.0, 0.0, 0.0);
        if n > 0.0 {
            let mut vels: Vec<f32> = Vec::new();
            for s in tr_c {
                if let Some(v) = s.carga { carga_avg += v; }
                if let Some(v) = s.nivel { nivel_avg += v; }
                if let Some(v) = s.ocupacion { ocup_avg += v; }
                if let Some(v) = s.vel { vels.push(v); }
            }
            carga_avg /= n.max(1.0);
            nivel_avg /= n.max(1.0);
            ocup_avg /= n.max(1.0);
            if !vels.is_empty() {
                vels.sort_by(|a, b| a.partial_cmp(b).unwrap());
                let m = vels.len() / 2;
                vel_med = if vels.len() % 2 == 1 { vels[m] } else { (vels[m - 1] + vels[m]) / 2.0 };
            }
        }

        let carga01 = clamp(carga_avg / 100.0, 0.0, 1.0);
        let nivel01 = clamp(nivel_avg / 3.0, 0.0, 1.0);
        let occ01   = clamp(ocup_avg / cfg.ocup_sat, 0.0, 1.0);
        let vel01   = clamp(vel_med / cfg.vel_free, 0.0, 1.0);
        let velinv  = 1.0 - vel01;

        let mut conf = 1.0;
        if tr_c.is_empty() { conf = 0.0; }
        else if (tr_c.len() as u8) < cfg.min_sens_ok {
            let f = ((tr_c.len() as i32 - cfg.min_sens_any as i32) as f32)
                / ((cfg.min_sens_ok - cfg.min_sens_any) as f32);
            conf = clamp(f, 0.4, 1.0);
        }

        let traffic_contrib = conf
            * (cfg.w_carga * carga01 + cfg.w_nivel * nivel01 + cfg.w_velinv * velinv + cfg.w_ocup * occ01);

        // Incidencias → penalización
        let mut blocked = false;
        let mut pen: f32 = 0.0;
        for inc in incs_c {
            let t = inc.tipo.to_lowercase();
            if t.contains("corte total") || t.contains("cerrad") { blocked = true; break; }
            else if t.contains("obra") { pen += 0.40; }
            else if t.contains("desvío") || t.contains("desvio") { pen += 0.30; }
            else if t.contains("restric") || t.contains("carril") { pen += 0.25; }
            else if t.contains("manifest") || t.contains("evento") { pen += 0.20; }
            else { pen += 0.15; }
        }
        if !blocked { pen = pen.min(cfg.inc_cap); }

        // DF: 999 si bloqueado
        let delay = if blocked {
            Some(999.0)
        } else {
            Some(clamp(1.0 + traffic_contrib + pen, cfg.delay_min, cfg.delay_max))
        };

        // Parking cercano (conteo + mínima distancia)
        let (clat, clon) = cell.centroid;
        let mut count = 0usize; let mut dmin = f32::INFINITY;
        for z in cargas {
            let d = haversine_m(clat, clon, z.lat, z.lon);
            if d <= cfg.park_radius_m { count += 1; if d < dmin { dmin = d; } }
        }
        if !dmin.is_finite() { dmin = cfg.park_radius_m; }
        let count01 = clamp(count as f32 / cfg.park_count_norm, 0.0, 1.0);
        let dist01 = 1.0 - clamp(dmin / cfg.park_radius_m, 0.0, 1.0);
        let parking_score = clamp(cfg.park_w_count * count01 + cfg.park_w_dist * dist01, 0.0, 1.0);
        let park_min = cfg.park_base_min * (1.0 - cfg.park_gain * parking_score);

        // GeoJSON del MAPA: sólo pinta si df > 1 + eps
        if let Some(df) = delay {
            if df > 1.0 + cfg.show_eps {
                let norm = ((df - 1.0) / (cfg.delay_max - 1.0)).clamp(0.0, 1.0);
                let color = color_from_norm(norm);
                let geom = cell_to_geometry(cell);
                let feat = json!({
                    "type": "Feature",
                    "geometry": geom,
                    "properties": {
                        "id": cell.id,
                        "delay_factor": (df * 100.0).round() / 100.0,
                        "blocked": blocked,
                        "incidencias": incs_c.len(),
                        "carga_near_count": count,
                        "style": {
                            "fill": true,
                            "fill-color": color, "fill-opacity": 0.75,
                            "stroke": color, "stroke-opacity": 1.0, "stroke-width": 1,
                            "fillColor": color, "fillOpacity": 0.75, "color": color, "opacity": 1.0, "weight": 1
                        }
                    }
                });
                features.push(feat);
            }
        }

        outs.push(CellOut {
            id: cell.id,
            delay_factor: delay.map(|d| (d * 100.0).round() / 100.0),
            blocked,
            incidencias: incs_c.len(),
            carga_near_count: count,
            carga_min_dist_m: (park_min * 100.0).round() / 100.0, // aquí guardas park_min 
            parking_score01: ((parking_score * 100.0).round() / 100.0),
        });
    }

    let fc = json!({ "type": "FeatureCollection", "features": features });
    (outs, fc)
}

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

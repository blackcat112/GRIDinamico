//! h3grid.rs
//! Agregacion y export sobre celdas H3: multi-res, refinado por hotspots
//! suavizado k-ring, y construcción de GeoJSON de hexágonos coloreados

use anyhow::Result;
use chrono::{SecondsFormat, Utc};
use geojson::GeoJson;
use h3o::{CellIndex, Resolution, LatLng};
use serde_json::json;
use std::collections::HashMap;

use crate::types::{DelayCfg, Incidencia, ParkingZone, RoutingCell, SensorTr};



// -------------------------------
// Métricas internas por celda H3
// -------------------------------
#[derive(Clone, Debug, Default)]
pub struct Metrics {
    pub n_traf: usize,
    pub carga_avg: f32,
    pub nivel_avg: f32,
    pub vel_med: f32,
    pub ocup_avg: f32,
    pub incidencias: usize,
    pub blocked: bool,
    pub delay_prom: f32,   // media del delay válido
    pub n_delay_ok: usize, // cuenta para la media
}

// --------------
// Utilidades
// --------------
#[inline]
fn clamp(x: f32, a: f32, b: f32) -> f32 { x.max(a).min(b) }

fn color_from_norm(x: f32) -> &'static str {
    const R: [&str; 11] = [
        "#e9f7ef","#d4f2e3","#bfeacc","#a9e3b6","#fff3b0",
        "#ffe08a","#ffc266","#ff9f58","#ff7a55","#f5544f","#d73a49"
    ];
    let i = (x.clamp(0.0,1.0) * 10.0).floor() as usize;
    R[i]
}

#[inline]
fn cell_center_deg(c: CellIndex) -> (f64, f64) {
    let verts: Vec<_> = c.boundary().to_vec();
    let mut sx = 0.0;
    let mut sy = 0.0;

    for ll in &verts {
        sx += ll.lng().to_degrees();
        sy += ll.lat().to_degrees();
    }

    let n = verts.len().max(1) as f64;
    (sx / n, sy / n) // (lon, lat)
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



fn delay_from_parts(
    carga01: f32, nivel01: f32, vel01: f32, occ01: f32,
    incidencias: usize, blocked: bool, cfg: &DelayCfg, conf: f32,
) -> Option<f32> {
    if blocked { return Some(999.0); }
    let velinv = 1.0 - vel01;
    // incidencias -> penalización simple por conteo (capada)f
    let mut pen = (incidencias as f32) * 0.20;
    pen = pen.min(cfg.inc_cap);

    let contrib = conf*(cfg.w_carga*carga01 + cfg.w_nivel*nivel01 + cfg.w_velinv*velinv + cfg.w_ocup*occ01);
    let d = 1.0 + contrib + pen;
    Some(clamp(d, cfg.delay_min, cfg.delay_max))
}

/// Indexa un par lat/lon en H3
#[inline]
fn h3_cell(lat: f32, lon: f32, res: u8) -> Option<CellIndex> {
    let r = Resolution::try_from(res).ok()?;
    let ll = LatLng::new(lat as f64, lon as f64).ok()?; // en h3o 0.5 es `new`
    Some(ll.to_cell(r))
}



// -------------------------------------------------------
// 1) Agregación a una resolucion base (p.ej. res=9)
// -------------------------------------------------------
pub fn aggregate_at_res(
    _cargas: &[ParkingZone],
    incs: &[Incidencia],
    traf: &[SensorTr],
    cfg: &DelayCfg,
    res: u8,
) -> HashMap<CellIndex, Metrics> {
    let mut map: HashMap<CellIndex, Metrics> = HashMap::new();

    // Tráfico
    for s in traf {
        if let Some(c) = h3_cell(s.lat, s.lon, res) {
            let e = map.entry(c).or_default();
            e.n_traf += 1;
            if let Some(v) = s.carga { e.carga_avg += v; }
            if let Some(v) = s.nivel { e.nivel_avg += v; }
            if let Some(v) = s.vel   { e.vel_med   += v; }
            if let Some(v) = s.ocupacion { e.ocup_avg += v; }
        }
    }
    for (_c, e) in map.iter_mut() {
        if e.n_traf > 0 {
            let n = e.n_traf as f32;
            e.carga_avg /= n;
            e.nivel_avg /= n;
            e.vel_med   /= n;
            e.ocup_avg  /= n;
        }
    }

    // Incidencias
    for inc in incs {
        if let Some(c) = h3_cell(inc.lat, inc.lon, res) {
            let e = map.entry(c).or_default();
            e.incidencias += 1;
            let t = inc.tipo.to_lowercase();
            if t.contains("corte total") || t.contains("cerrad") { e.blocked = true; }
        }
    }

    // Delay por celda
    for (_c, e) in map.iter_mut() {
        let carga01 = clamp(e.carga_avg/100.0, 0.0, 1.0);
        let nivel01 = clamp(e.nivel_avg/3.0, 0.0, 1.0);
        let vel01   = clamp(e.vel_med/cfg.vel_free, 0.0, 1.0);
        let occ01   = clamp(e.ocup_avg/cfg.ocup_sat, 0.0, 1.0);

        // confianza por nº sensores
        let conf = if e.n_traf == 0 { 0.0 } else if (e.n_traf as u8) >= cfg.min_sens_ok { 1.0 }
                   else {
                       let f = ((e.n_traf as i32 - cfg.min_sens_any as i32) as f32)
                             / ((cfg.min_sens_ok - cfg.min_sens_any) as f32);
                       clamp(f, 0.4, 1.0)
                   };

        if let Some(d) = delay_from_parts(carga01,nivel01,vel01,occ01,e.incidencias,e.blocked,cfg,conf) {
            e.delay_prom = d;
            e.n_delay_ok = 1;
        }
    }

    map
}

// -------------------------------------------------------
// 2) Downsample a parent (hex “grandes”)
// -------------------------------------------------------
pub fn downsample_to_parent(
    child_metrics: &HashMap<CellIndex, Metrics>,
    parent_res: u8,
) -> Result<HashMap<CellIndex, Metrics>> {
    let r = Resolution::try_from(parent_res)?;
    let mut acc: HashMap<CellIndex, Metrics> = HashMap::new();

    for (cell, m) in child_metrics {
        let p = cell.parent(r).ok_or_else(|| anyhow::anyhow!("no parent"))?;
        let e = acc.entry(p).or_default();

        e.n_traf      += m.n_traf;
        e.incidencias += m.incidencias;
        e.carga_avg   += m.carga_avg;
        e.nivel_avg   += m.nivel_avg;
        e.vel_med     += m.vel_med;
        e.ocup_avg    += m.ocup_avg;
        e.n_delay_ok  += m.n_delay_ok;
        e.delay_prom  += m.delay_prom;
        e.blocked     |= m.blocked;
    }

    for (_p, e) in acc.iter_mut() {
        if e.n_traf > 0 {
            let n = e.n_traf as f32;
            e.carga_avg /= n; e.nivel_avg /= n; e.vel_med /= n; e.ocup_avg /= n;
        }
        if e.n_delay_ok > 0 {
            e.delay_prom /= e.n_delay_ok as f32;
        }
    }
    Ok(acc)
}

// -------------------------------------------------------
// 3) Refinado de hotspots (AMR)
// -------------------------------------------------------
pub fn refine_hotspots(
    parent: &HashMap<CellIndex, Metrics>,
    base_child: &HashMap<CellIndex, Metrics>,
    child_res: u8,
    delay_thr: f32,
) -> Vec<CellIndex> {
    let r_child = Resolution::try_from(child_res).unwrap();
    let mut out = Vec::new();
    for (&p, m) in parent {
        if m.blocked || m.delay_prom > delay_thr {
            for ch in p.children(r_child) { out.push(ch); }
        } else {
            out.push(p);
        }
    }
    out.retain(|c| parent.contains_key(c) || base_child.contains_key(c));
    out
}

// -------------------------------------------------------
// 4) Suavizado k-ring (opcional) no esta en uso
// -------------------------------------------------------
pub fn smooth_with_kring(
    metrics: &std::collections::HashMap<CellIndex, Metrics>,
    _k: u32,
) -> std::collections::HashMap<CellIndex, Metrics> {
    metrics.clone()
}

// -------------------------------------------------------
// 5) Export ligero: [{h3, delay}], filtros min_delay y bbox
// -------------------------------------------------------
pub fn export_routing_cells(
    metrics: &HashMap<CellIndex, Metrics>,
    _min_delay: f32,
    bbox: Option<(f64,f64,f64,f64)>, // (minLon,minLat,maxLon,maxLat)
) -> Vec<RoutingCell> {
    let mut v = Vec::with_capacity(metrics.len());
    for (c, m) in metrics {
        let d = m.delay_prom;
        if let Some((minx, miny, maxx, maxy)) = bbox {
            let (lon, lat) = cell_center_deg(*c);
            if lon < minx || lon > maxx || lat < miny || lat > maxy {
                continue;
            }
        }
        
        v.push(RoutingCell { h3: c.to_string(), delay: d });
    }
    v
}

// -------------------------------------------------------
// 6) GeoJSON para UI (pintamos solo delay > 1+eps)
// -------------------------------------------------------
pub fn to_geojson(
    metrics: &HashMap<CellIndex, Metrics>,
    cfg: &DelayCfg,
) -> String {
    let mut features = Vec::new();
    for (c, m) in metrics {
        let d = m.delay_prom;
        if d <= 1.0 + cfg.show_eps && d != 999.0 { continue; }
        // color
        let norm = if d == 999.0 { 1.0 } else { ((d - 1.0)/(cfg.delay_max - 1.0)).clamp(0.0, 1.0) };
        let col = color_from_norm(norm);

        // polígono H3
        let exterior= cell_polygon_coords(*c);

        let feat = json!({
            "type": "Feature",
            "geometry": { "type":"Polygon", "coordinates": [exterior] },
            "properties": {
                "h3": c.to_string(),
                "delay_factor": ((d*100.0).round()/100.0),
                "blocked": (d==999.0),
                "style": {
                    "fill": true, "fill-color": col, "fill-opacity": 0.75,
                    "stroke": col, "stroke-width": 1, "stroke-opacity": 1.0
                }
            }
        });
        features.push(feat);
    }

    let gj = json!({
        "type":"FeatureCollection",
        "name":"hex_delay_h3",
        "crs": { "type":"name","properties":{"name":"EPSG:4326"} },
        "ts_utc": Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true),
        "features": features
    });
    GeoJson::from_json_value(gj).unwrap_or(GeoJson::FeatureCollection(Default::default())).to_string()
}

// -------------------------------------------------------
// 7) Orquestador
// -------------------------------------------------------
pub struct RecomputeOut {
    pub routing: Vec<RoutingCell>,
    pub geojson: String,
    pub ts_utc: String,
}

pub fn recompute_h3(
    cargas: &[ParkingZone],
    incs: &[Incidencia],
    traf: &[SensorTr],
    cfg: &DelayCfg,
    base_res: u8,             
    refine: Option<(u8,f32)>, 
    k_smooth: u32,            
    min_delay_export: f32,    
) -> RecomputeOut {
    let base = aggregate_at_res(cargas, incs, traf, cfg, base_res);

  
    let mut metrics_to_show = base.clone();


    if let Some((parent_res, thr)) = refine {
        if let Ok(parent_map) = downsample_to_parent(&base, parent_res) {
            let ids = refine_hotspots(&parent_map, &base, base_res, thr);
            let mut mixed = HashMap::new();
            for id in ids {
                if let Some(m) = base.get(&id) {
                    mixed.insert(id, m.clone());
                } else if let Some(m) = parent_map.get(&id) {
                    mixed.insert(id, m.clone());
                }
            }
            metrics_to_show = mixed;
        }
    }

    // suavizado
    let metrics_smoothed = if k_smooth > 0 {
        smooth_with_kring(&metrics_to_show, k_smooth)
    } else { metrics_to_show };

    let ts = Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true);
    let geojson = to_geojson(&metrics_smoothed, cfg);
    let routing = export_routing_cells(&metrics_smoothed, min_delay_export, None);

    RecomputeOut { routing, geojson, ts_utc: ts }
}

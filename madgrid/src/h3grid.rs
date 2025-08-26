// src/h3grid.rs
//! Agregado por celdas H3, generación GeoJSON, y export JSON para ruteo.
//! - base_res: resolución H3 principal (p.ej. 9 ~ 170-200 m).
//! - refine: si true, hace AMR simple (res+1) en celdas con df alto.
//!
//! Entradas: cargas (zonas de carga), incs (incidencias), traf (sensores).
//! Salidas: FeatureCollection (polígonos hex), JSON ligero {h3, df, blocked}.

use std::collections::{HashMap, HashSet};
use serde_json::json;
use anyhow::Result;
use h3o::CellIndex;

use h3o::{CellIndex, LatLng, Resolution};
use crate::types::{ParkingZone, Incidencia, SensorTr, DelayCfg};

#[inline]
fn clamp(x: f32, a: f32, b: f32) -> f32 { x.max(a).min(b) }

#[inline]
fn haversine_m(lat1: f32, lon1: f32, lat2: f32, lon2: f32) -> f32 {
    let r = 6_371_000.0_f32;
    let dlat = (lat2 - lat1).to_radians();
    let dlon = (lon2 - lon1).to_radians();
    let a = (dlat/2.0).sin().powi(2)
      + lat1.to_radians().cos()*lat2.to_radians().cos()*(dlon/2.0).sin().powi(2);
    2.0*r*a.sqrt().atan2((1.0-a).sqrt())
}

fn color_from_norm(x: f32) -> &'static str {
    const RAMP: [&str; 11] = [
        "#e9f7ef","#d4f2e3","#bfeacc","#a9e3b6","#fff3b0",
        "#ffe08a","#ffc266","#ff9f58","#ff7a55","#f5544f","#d73a49"
    ];
    let i = (x.clamp(0.0,1.0) * ((RAMP.len()-1) as f32)).floor() as usize;
    RAMP[i]
}

fn h3_of(lat: f32, lon: f32, res: Resolution) -> CellIndex {
    LatLng::new(lat as f64, lon as f64).unwrap().to_cell(res)
}

fn poly_of(cell: CellIndex) -> Vec<[f64;2]> {
    // Coordenadas [lon,lat] del borde H3 (para GeoJSON)
    cell.boundary()
        .iter()
        .map(|c| [c.lng().to_degrees(), c.lat().to_degrees()])
        .collect()
}

#[derive(Default, Clone)]
struct Agg {
    n_sens: usize,
    carga_sum: f32,
    nivel_sum: f32,
    ocup_sum: f32,
    vels: Vec<f32>,
    inc_count: usize,
    inc_pen: f32,
    blocked: bool,
    // parking (lo calculamos al final con el centro de celda)
}

fn fold_incidence(a: &mut Agg, inc: &Incidencia, cfg: &DelayCfg) {
    a.inc_count += 1;
    let t = inc.tipo.to_lowercase();
    if t.contains("corte total") || t.contains("cerrad") { a.blocked = true; }
    else if t.contains("obra") { a.inc_pen += 0.40; }
    else if t.contains("desvío") || t.contains("desvio") { a.inc_pen += 0.30; }
    else if t.contains("restric") || t.contains("carril") { a.inc_pen += 0.25; }
    else if t.contains("manifest") || t.contains("evento") { a.inc_pen += 0.20; }
    else { a.inc_pen += 0.15; }
    if !a.blocked { a.inc_pen = a.inc_pen.min(cfg.inc_cap); }
}

fn fold_sensor(a: &mut Agg, s: &SensorTr) {
    a.n_sens += 1;
    if let Some(v) = s.carga { a.carga_sum += v; }
    if let Some(v) = s.nivel { a.nivel_sum += v; }
    if let Some(v) = s.ocupacion { a.ocup_sum += v; }
    if let Some(v) = s.vel { a.vels.push(v); }
}

fn finalize_df(a: &Agg, cfg: &DelayCfg) -> Option<f32> {
    if a.blocked { return Some(999.0); } // bloqueado → marcador 999
    let n = a.n_sens as f32;
    let (mut carga_avg, mut nivel_avg, mut ocup_avg, mut vel_med) = (0.0,0.0,0.0,0.0);
    if n > 0.0 {
        carga_avg = a.carga_sum / n;
        nivel_avg = a.nivel_sum / n;
        ocup_avg  = a.ocup_sum  / n;
        if !a.vels.is_empty() {
            let mut v = a.vels.clone();
            v.sort_by(|x,y| x.partial_cmp(y).unwrap());
            let m = v.len()/2;
            vel_med = if v.len() % 2 == 1 { v[m] } else { (v[m-1]+v[m])/2.0 };
        }
    }
    let carga01 = clamp(carga_avg/100.0, 0.0, 1.0);
    let nivel01 = clamp(nivel_avg/3.0, 0.0, 1.0);
    let occ01   = clamp(ocup_avg/cfg.ocup_sat, 0.0, 1.0);
    let vel01   = clamp(vel_med/cfg.vel_free, 0.0, 1.0);
    let velinv  = 1.0 - vel01;

    // confianza por nº sensores
    let mut conf = 1.0;
    if a.n_sens == 0 { conf = 0.0; }
    else if (a.n_sens as u8) < cfg.min_sens_ok {
        let f = ((a.n_sens as i32 - cfg.min_sens_any as i32) as f32)
              / ((cfg.min_sens_ok - cfg.min_sens_any) as f32);
        conf = clamp(f, 0.4, 1.0);
    }

    let traffic = conf*(cfg.w_carga*carga01 + cfg.w_nivel*nivel01 + cfg.w_velinv*velinv + cfg.w_ocup*occ01);
    let df = 1.0 + traffic + a.inc_pen;
    Some(clamp(df, cfg.delay_min, cfg.delay_max))
}

fn cell_center(cell: CellIndex) -> (f32, f32) {
    let c = cell.lat_lng();
    (c.lat().to_degrees() as f32, c.lng().to_degrees() as f32) 
}

/// Calcula parking_score aproximado: cuenta zonas de carga en radio y mínima distancia.
/// (Lineal O(#cargas) por celda; suficiente con pocos miles.)
fn parking_for_cell(cell: CellIndex, cargas: &[ParkingZone], cfg: &DelayCfg) -> (usize, f32, f32) {
    let (clat, clon) = cell_center(cell);
    let mut count = 0usize;
    let mut dmin = f32::INFINITY;
    for z in cargas {
        let d = haversine_m(clat, clon, z.lat, z.lon);
        if d <= cfg.park_radius_m { count += 1; if d < dmin { dmin = d; } }
    }
    if !dmin.is_finite() { dmin = cfg.park_radius_m; }
    let count01 = clamp(count as f32 / cfg.park_count_norm, 0.0, 1.0);
    let dist01 = 1.0 - clamp(dmin / cfg.park_radius_m, 0.0, 1.0);
    let score = clamp(cfg.park_w_count*count01 + cfg.park_w_dist*dist01, 0.0, 1.0);
    (count, dmin, score)
}

/// Agrega entradas a nivel H3 para una resolución dada.
fn aggregate_once(
    res: Resolution,
    cargas: &[ParkingZone],
    incs: &[Incidencia],
    traf: &[SensorTr],
    cfg: &DelayCfg,
) -> HashMap<CellIndex, (Agg, f32 /*df*/)> {

    let mut map: HashMap<CellIndex, Agg> = HashMap::new();

    for inc in incs {
        let h = h3_of(inc.lat, inc.lon, res);
        fold_incidence(map.entry(h).or_default(), inc, cfg);
    }
    for s in traf {
        let h = h3_of(s.lat, s.lon, res);
        fold_sensor(map.entry(h).or_default(), s);
    }

    // Finalizar df por celda
    let mut out: HashMap<CellIndex, (Agg, f32)> = HashMap::new();
    for (h, a) in map.into_iter() {
        let mut a = a;
        let df = finalize_df(&a, cfg).unwrap_or(1.0);
        out.insert(h, (a.clone(), df));
    }
    out
}

/// Simple AMR: para celdas con df alto, recalcula en res+1 y sustituye.
fn refine_amr(
    base: &mut HashMap<CellIndex, (Agg, f32)>,
    res: Resolution,
    cargas: &[ParkingZone],
    incs: &[Incidencia],
    traf: &[SensorTr],
    cfg: &DelayCfg,
    df_threshold: f32,
) {
    let finer = res.succ().unwrap_or(res);
    if finer == res { return; }

    let mut to_refine: Vec<CellIndex> = Vec::new();
    for (h, (_a, df)) in base.iter() {
        if *df > df_threshold { to_refine.push(*h); }
    }
    if to_refine.is_empty() { return; }

    // Reagregamos SOLO los hijos de esos padres
    let mut replace: HashMap<CellIndex, HashMap<CellIndex,(Agg,f32)>> = HashMap::new();

    for parent in to_refine {
        // colecciones de entradas en el parent
        let mut incs_child: HashMap<CellIndex, Vec<&Incidencia>> = HashMap::new();
        let mut traf_child: HashMap<CellIndex, Vec<&SensorTr>> = HashMap::new();

        for inc in incs {
            let hp = h3_of(inc.lat, inc.lon, res);
            if hp == parent {
                let hc = h3_of(inc.lat, inc.lon, finer);
                incs_child.entry(hc).or_default().push(inc);
            }
        }
        for s in traf {
            let hp = h3_of(s.lat, s.lon, res);
            if hp == parent {
                let hc = h3_of(s.lat, s.lon, finer);
                traf_child.entry(hc).or_default().push(s);
            }
        }

        if incs_child.is_empty() && traf_child.is_empty() { continue; }

        // Calcula df por hijo
        let mut finer_map: HashMap<CellIndex,(Agg,f32)> = HashMap::new();
        let child_keys: HashSet<CellIndex> =
            incs_child.keys().chain(traf_child.keys()).copied().collect();
        for hc in child_keys.into_iter() {
            let mut a = Agg::default();
            if let Some(v) = incs_child.get(&hc) { for inc in v { fold_incidence(&mut a, inc, cfg); } }
            if let Some(v) = traf_child.get(&hc) { for s in v { fold_sensor(&mut a, s); } }
            let df = finalize_df(&a, cfg).unwrap_or(1.0);
            finer_map.insert(hc, (a, df));
        }
        replace.insert(parent, finer_map);
    }

    // Sustituimos parent por hijos
    for (parent, children) in replace.into_iter() {
        base.remove(&parent);
        for (hc, pair) in children.into_iter() {
            base.insert(hc, pair);
        }
    }
}

/// Recompute completo H3 -> (GeoJSON mapa, JSON ruteo)
pub fn recompute_h3(
    cargas: &[ParkingZone],
    incs: &[Incidencia],
    traf: &[SensorTr],
    cfg: &DelayCfg,
    base_res: u8,   // p.ej. 9
    refine: bool,   // AMR on/off
) -> Result<(serde_json::Value /*geojson*/, serde_json::Value /*routing*/)> {

    let res = Resolution::try_from(base_res).unwrap_or(Resolution::Nine);

    let mut agg = aggregate_once(res, cargas, incs, traf, cfg);

    if refine {
        // refina si df > 1.15f 
        refine_amr(&mut agg, res, cargas, incs, traf, cfg, 1.15);
    }

    // Construir FeatureCollection (pintamos sólo df > 1+eps)
    let mut features = Vec::new();
    let mut cells_export = Vec::new();

    for (h, (_a, df)) in agg.into_iter() {
        if df <= 1.0 + cfg.show_eps { continue; }

        // style/color
        let norm = ((df - 1.0)/ (cfg.delay_max - 1.0)).clamp(0.0, 1.0);
        let color = if (df - 999.0).abs() < f32::EPSILON {
            "#d73a49"
        } else {
            color_from_norm(norm)
        };

        // parking (opcional; si no lo quieres, coméntalo)
        let (count, _dmin, _score) = parking_for_cell(h, cargas, cfg);

        let coords = poly_of(h);
        let feat = json!({
            "type":"Feature",
            "geometry": { "type":"Polygon", "coordinates":[ coords ] },
            "properties": {
                "h3": h.to_string(),
                "delay_factor": ((df*100.0).round()/100.0),
                "blocked": (df >= 999.0),
                "carga_near_count": count,
                "style":{
                    "fill": true,
                    "fill-color": color, "fill-opacity": 0.75,
                    "stroke": color, "stroke-width": 1, "stroke-opacity": 1.0
                }
            }
        });
        features.push(feat);

        // export ligero para ruteo
        cells_export.push(json!({"h3": h.to_string(), "df": ((df*100.0).round()/100.0), "blocked": (df>=999.0)}));
    }

    let fc = json!({"type":"FeatureCollection","features": features});
    let export = json!({
        "ts_utc": chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
        "res": base_res,
        "cells": cells_export
    });

    Ok((fc, export))
}

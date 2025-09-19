//! h3grid.rs
//! Agregacion y export sobre celdas H3: multi-res, refinado por hotspots
//! suavizado k-ring, y construcción de GeoJSON de hexágonos coloreados

use anyhow::Result;
use chrono::{SecondsFormat, Utc};
use geojson::GeoJson;
use h3o::{CellIndex, Resolution, LatLng};
use serde_json::json;
use std::collections::{HashMap, HashSet};

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
fn haversine_km(lat1: f64, lon1: f64, lat2: f64, lon2: f64) -> f64 {
    let r = 6371.0_f64;
    let dlat = (lat2 - lat1).to_radians();
    let dlon = (lon2 - lon1).to_radians();
    let a = (dlat / 2.0).sin().powi(2)
        + lat1.to_radians().cos() * lat2.to_radians().cos() * (dlon / 2.0).sin().powi(2);
    2.0 * r * a.sqrt().atan2((1.0 - a).sqrt())
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

/// ¿Están presentes *todas* las hijas a `r_base` bajo el padre `p`?
#[allow(dead_code)]
fn all_children_present(
    p: CellIndex,
    r_base: Resolution,
    base: &HashMap<CellIndex, Metrics>,
) -> bool {
    for ch in p.children(r_base) {
        if !base.contains_key(&ch) {
            return false;
        }
    }
    true
}

fn coverage_ratio(
    p: CellIndex,
    r_base: Resolution,
    base: &HashMap<CellIndex, Metrics>,
) -> f32 {
    let mut have = 0usize;
    let mut total = 0usize;
    for ch in p.children(r_base) {
        total += 1;
        if base.contains_key(&ch) {
            have += 1;
        }
    }
    if total == 0 { 0.0 } else { have as f32 / total as f32 }
}



fn delay_from_parts(
    carga01: f32,     // 0-1 normalizado
    nivel01: f32,     // 0-1 (nivel de servicio, 1 = malo)
    vel01: f32,       // velocidad actual / velocidad libre (0-1)
    occ01: f32,       // ocupación 0-1
    incidencias: usize,
    blocked: bool,
    cfg: &DelayCfg,
    conf: f32,        // confianza (0-1)
) -> Option<f32> {
    if blocked { return Some(cfg.delay_max); }

    // Inverso de velocidad respecto a la velocidad libre
    let velinv = 1.0 - vel01;

    // Penalización por incidencias (capada)
    let mut pen = (incidencias as f32) * 0.5;
    pen = pen.min(cfg.inc_cap);

    // Cálculo de contribución (ponderación Google/TomTom-style)
    // Idea: delay es combinación lineal ponderada de factores de congestión
    let contrib = conf * (
        cfg.w_carga * carga01 +
        cfg.w_nivel * nivel01 +
        cfg.w_velinv * velinv +
        cfg.w_ocup * occ01
    );

    // Delay = relación sobre tiempo libre
    // Se asegura un mínimo >1 para no dar delays negativos
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

/// Compacta padres "fríos" (delay bajo y sin bloqueo) a `parent_res`
/// y mantiene detalle en hotspots (o cuando faltan hijas).
///
/// - `base`: métricas a `base_res` (salida de `aggregate_at_res`)
/// - `parent_map`: métricas agregadas a `parent_res` (salida de `downsample_to_parent`)
/// - `delay_thr`: umbral; > thr = hotspot
/// - `require_full_coverage`: si `true`, sólo compacta si están **todas** las hijas de `base_res`
///
/// Devuelve un mapa mixto (algunas celdas en `base_res`, otras en `parent_res`).
pub fn selective_compact_by_delay(
    base: &HashMap<CellIndex, Metrics>,
    parent_map: &HashMap<CellIndex, Metrics>,
    base_res: u8,
    parent_res: u8,
    delay_thr: f32,
    min_coverage: f32, // 0..1 (ej: 0.7 → 5/7 hijas)
) -> HashMap<CellIndex, Metrics> {
    let r_base   = Resolution::try_from(base_res).expect("base_res inválida");
    let _r_parent = Resolution::try_from(parent_res).expect("parent_res inválida");

    let mut compactables = HashSet::new();
    let mut hotspots     = HashSet::new();

    for (&p, m) in parent_map {
        if m.blocked || m.delay_prom > delay_thr {
            hotspots.insert(p);
        } else {
            compactables.insert(p);
        }
    }

    let mut out: HashMap<CellIndex, Metrics> = HashMap::new();

    // Hotspots: mantener detalle
    for p in &hotspots {
        for ch in p.children(r_base) {
            if let Some(m) = base.get(&ch) {
                out.insert(ch, m.clone());
            }
        }
    }

    // Zonas frías: compactar si cobertura suficiente, si no dejar detalle
    for p in &compactables {
        let cov = coverage_ratio(*p, r_base, base);
        if cov >= min_coverage {
            if let Some(m) = parent_map.get(p) {
                out.insert(*p, m.clone());
            }
        } else {
            for ch in p.children(r_base) {
                if let Some(m) = base.get(&ch) {
                    out.insert(ch, m.clone());
                }
            }
        }
    }

    out
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
#[allow(dead_code)]
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
    bbox: Option<(f64,f64,f64,f64)>,
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

        let exterior = cell_polygon_coords(*c);

        v.push(RoutingCell {
            h3: c.to_string(),
            delay: (d * 100.0).round() / 100.0, // redondeo como en tu json!
            coordinates: exterior,
        });
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
                "geometry": { "type":"Polygon", "coordinates": [exterior] },
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


    // --- AMR por compacción selectiva ---
    if let Some((parent_res, thr)) = refine {
        if let Ok(parent_map) = downsample_to_parent(&base, parent_res) {
            // compactamos zonas frías (<= thr), mantenemos detalle en hotspots (> thr o blocked)
            let mixed = selective_compact_by_delay(
                &base,
                &parent_map,
                base_res,
                parent_res,
                thr,
                0.5,
            );
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
pub fn geojson_zaragoza_mesh() -> String {
    // Punto de referencia: Plaza del Pilar aprox.
    let lat_c = 41.65606_f64;
    let lon_c = -0.87734_f64;
    let center = LatLng::new(lat_c, lon_c).unwrap();

    // resoluciones
    let r5 = Resolution::try_from(6u8).unwrap(); // ~8 km
    let r6 = Resolution::try_from(10u8).unwrap(); // ~3 km

    // padre r5 que contiene el centro
    let c5_center: CellIndex = center.to_cell(r5);

    // extensión r5 alrededor (solo para pintar periferia)
    let k_outer: u32 = 8;
    let mut r5_disk = std::collections::HashSet::new();
    for k in 0..=k_outer {
        for c in c5_center.grid_disk::<Vec<CellIndex>>(k) {
            r5_disk.insert(c);
        }
    }

    // --- elegimos los 2 vecinos r5 que COMPARTEN el vértice más cercano al punto ---
    // 1) vértice de c5_center más cercano a (lat_c, lon_c)
    let mut best_lon = 0.0f64;
    let mut best_lat = 0.0f64;
    let mut best_d   = f64::MAX;
    let verts = c5_center.boundary();
    for ll in verts.iter() {
        let lon = ll.lng(); // en grados
        let lat = ll.lat();
        let d = haversine_km(lat, lon, lat_c, lon_c);
        if d < best_d {
            best_d = d;
            best_lon = lon;
            best_lat = lat;
        }
    }

    // 2) vecinos ring=1 (6 vecinos del r5 central)
    let ring1: Vec<CellIndex> = c5_center
        .grid_disk::<Vec<CellIndex>>(1)
        .into_iter()
        .filter(|c| *c != c5_center)
        .collect();

    // 3) busca los DOS vecinos que comparten ese vértice (coincidencia por coordenada con tolerancia)
    let eps = 1e-9_f64;
    let mut neigh_at_vertex: Vec<CellIndex> = Vec::new();
    for n in &ring1 {
        let vb = n.boundary();
        let shares = vb.iter().any(|ll| (ll.lng() - best_lon).abs() < eps && (ll.lat() - best_lat).abs() < eps);
        if shares {
            neigh_at_vertex.push(*n);
        }
    }

    // fallback por si algo raro: si no se encuentran 2, cogemos los 2 más cercanos por centro
    if neigh_at_vertex.len() < 2 {
        let mut ring1_sorted = ring1.clone();
        ring1_sorted.sort_by(|&a, &b| {
            let (lon_a, lat_a) = cell_center_deg(a);
            let (lon_b, lat_b) = cell_center_deg(b);
            let da = haversine_km(lat_a, lon_a, lat_c, lon_c);
            let db = haversine_km(lat_b, lon_b, lat_c, lon_c);
            da.partial_cmp(&db).unwrap_or(std::cmp::Ordering::Equal)
        });
        neigh_at_vertex = ring1_sorted.into_iter().take(2).collect();
    } else if neigh_at_vertex.len() > 2 {
        // en teoría no debería pasar; si pasa, ordena por distancia al punto y quédate con 2
        neigh_at_vertex.sort_by(|&a, &b| {
            let (lon_a, lat_a) = cell_center_deg(a);
            let (lon_b, lat_b) = cell_center_deg(b);
            let da = haversine_km(lat_a, lon_a, lat_c, lon_c);
            let db = haversine_km(lat_b, lon_b, lat_c, lon_c);
            da.partial_cmp(&db).unwrap_or(std::cmp::Ordering::Equal)
        });
        neigh_at_vertex.truncate(2);
    }

    // padres a expandir a r6: central + 2 vecinos que tocan el vértice más cercano
    let mut expand_parents: std::collections::HashSet<CellIndex> = std::collections::HashSet::new();
    expand_parents.insert(c5_center);
    for n in neigh_at_vertex.iter() {
        expand_parents.insert(*n);
    }

    // periferia r5 = todo el disco menos los 3 padres que expandimos
    let mut outer_r5: Vec<CellIndex> = r5_disk
        .into_iter()
        .filter(|c| !expand_parents.contains(c))
        .collect();

    // hijas r6 de los 3 padres seleccionados
    let mut inner_r6: Vec<CellIndex> = Vec::new();
    for p in &expand_parents {
        for ch in p.children(r6) {
            inner_r6.push(ch);
        }
    }

    // estilos
    let inner_style = json!({
        "fill": true, "fill-color": "#06b6d4", "fill-opacity": 0.55,
        "stroke": "#22d3ee", "stroke-width": 1.2, "stroke-opacity": 0.9
    });
    let outer_style = json!({
        "fill": true, "fill-color": "#8b5cf6", "fill-opacity": 0.45,
        "stroke": "#a78bfa", "stroke-width": 1.0, "stroke-opacity": 0.85
    });

    // debug
    let mut dbg_parents = Vec::new();
    for p in &expand_parents {
        let (lon, lat) = cell_center_deg(*p);
        let d = haversine_km(lat, lon, lat_c, lon_c);
        dbg_parents.push(format!("{} (d={:.2}km)", p, d));
    }

    // geojson
    let mut features = Vec::new();

    // periferia r5
    for c in outer_r5.drain(..) {
        let exterior = cell_polygon_coords(c);
        features.push(json!({
            "type":"Feature",
            "geometry": { "type":"Polygon", "coordinates":[exterior] },
            "properties": {
                "h3": c.to_string(),
                "zona": "periphery_r5",
                "delay_factor": 1.0,
                "style": outer_style
            }
        }));
    }

    // centro r6 (hijas de los 3 padres seleccionados)
    for c in inner_r6.drain(..) {
        let exterior = cell_polygon_coords(c);
        features.push(json!({
            "type":"Feature",
            "geometry": { "type":"Polygon", "coordinates":[exterior] },
            "properties": {
                "h3": c.to_string(),
                "zona": "center_r6",
                "delay_factor": 1.0,
                "style": inner_style
            }
        }));
    }

    let gj = json!({
        "type":"FeatureCollection",
        "name":"zgz_mesh_center_r5_vertex_triplet_to_r6",
        "crs": { "type":"name","properties":{"name":"EPSG:4326"} },
        "features": features
    });
    GeoJson::from_json_value(gj).unwrap().to_string()
}

//! h3grid.rs — O/D + fallback TomTom + históricos (Orion-LD / JSONL)
//! Versión corregida (serde_with, Default de CellIndex, Result alias, FromStr)

use anyhow::{anyhow, Context, Result};
use async_trait::async_trait;
use chrono::{NaiveDate, SecondsFormat, Utc};
use futures::{stream, StreamExt};
use geojson::GeoJson;
use h3o::{CellIndex, LatLng, Resolution};
use serde_json::json;
use std::collections::HashMap;
use std::str::FromStr;
use std::time::Duration;
use tracing::{debug, info, warn};

use crate::models::h3types::*;

// ===============================
// Configuración y tipos de dominio

impl Default for DelayCfg {
    fn default() -> Self {
        Self {
            res: 7,
            alpha_vol: 0.8,
            beta_truck_mix: 0.4,
            delay_min: 1.0,
            delay_max: 6.0,
            min_conf_for_pure_orange: 0.65,
            max_concurrent_calls: 16,
            truck_factor: 1.4,
            car_factor: 1.0,
            show_eps: 0.02,
        // --- NUEVO: parámetros BPR-like ---
            bpr_a: 0.15,              // intensidad de congestión
            bpr_b: 4.0,               // curvatura
            truck_gamma: 0.4,         // sensibilidad a camiones (0.2–0.6 típico)
            capacity_percentile: 0.9, // percentil para estimar c (0.85–0.95 habitual)
            capacity_floor: 10.0,     // suelo para evitar c muy bajo (ajústalo a tu escala)
            vc_cap: 2.0,              // tope para (v/c) antes de elevar a b (numericamente estable)
        }
    }
}

impl H3Metrics {
    pub fn new(cell: CellIndex) -> Self {
        Self {
            cell,
            trips_total: 0.0,
            trips_trucks: 0.0,
            trips_cars: 0.0,
            conf_sum: 0.0,
            conf_weight: 0.0,
            delay_orange: 1.0,
            delay_tomtom: 0.0,
            delay_final: 1.0,
            truck_share: 0.0,
            vol_norm: 0.0,
        }
    }
    pub fn conf_cell(&self) -> f32 {
        if self.conf_weight > 0.0 {
            (self.conf_sum / self.conf_weight).clamp(0.0, 1.0)
        } else {
            1.0
        }
    }
}


// ===============================
// Proveedor de tráfico (TomTom, etc.)
// ===============================

#[async_trait]
pub trait TrafficProvider: Send + Sync {
    async fn delay_for_cell(&self, cell: CellIndex) -> anyhow::Result<Option<(f32, f32)>>;
}

impl TomTomClient {
    pub fn new(api_key: impl Into<String>) -> Self {
        Self {
            http: reqwest::Client::builder()
                .gzip(true)
                .brotli(true)
                .deflate(true)
                .timeout(Duration::from_secs(8))
                .build()
                .expect("reqwest::Client"),
            api_key: api_key.into(),
            base_url_absolute: "https://api.tomtom.com/traffic/services/4/flowSegmentData/absolute/10/json".to_string(),
            timeout: Duration::from_secs(8),
        }
    }
}

#[async_trait]
impl TrafficProvider for TomTomClient {
    async fn delay_for_cell(&self, cell: CellIndex) -> anyhow::Result<Option<(f32, f32)>> {
        let ll: LatLng = cell.into();
        let lat = ll.lat();
        let lon = ll.lng();

        let url = reqwest::Url::parse_with_params(
            &self.base_url_absolute,
            &[
                ("point", format!("{lat},{lon}")),
                ("unit", "kmph".to_string()),
                ("key", self.api_key.clone()),
            ],
        )?;

        let resp = self
            .http
            .get(url)
            .timeout(self.timeout)
            .send()
            .await
            .context("TomTom request failed")?;

        if resp.status().is_success() {
            let v: serde_json::Value = resp.json().await.context("TomTom JSON parse")?;
            let cs = v["flowSegmentData"]["currentSpeed"].as_f64();
            let fs = v["flowSegmentData"]["freeFlowSpeed"].as_f64();
            let conf = v["flowSegmentData"]["confidence"].as_f64();

            match (cs, fs) {
                (Some(curr), Some(free)) if curr > 1e-6 && free > 1e-6 => {
                    let delay = (free / curr) as f32;
                    let confidence = conf.unwrap_or(1.0) as f32;
                    Ok(Some((delay.clamp(1.0, 10.0), confidence.clamp(0.0, 1.0))))
                }
                _ => Ok(None),
            }
        } else if resp.status().as_u16() == 404 {
            Ok(None)
        } else {
            warn!("TomTom non-success: {}", resp.status());
            Ok(None)
        }
    }
}

// ===============================
// Persistencia de históricos
// ===============================

#[async_trait]
pub trait HistorySink: Send + Sync {
    async fn persist(&self, rows: &[H3DailyRow]) -> anyhow::Result<()>;
}

pub struct JsonlSink {
    pub path: std::path::PathBuf,
}

impl JsonlSink {
    pub fn new(path: impl Into<std::path::PathBuf>) -> Self {
        Self { path: path.into() }
    }
}

#[async_trait]
impl HistorySink for JsonlSink {
    async fn persist(&self, rows: &[H3DailyRow]) -> anyhow::Result<()> {
        use tokio::io::AsyncWriteExt;

        if let Some(parent) = self.path.parent() {
            tokio::fs::create_dir_all(parent).await.ok();
        }

        let mut f = tokio::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)
            .await?;
        for r in rows {
            let line = serde_json::to_string(r)? + "\n";
            f.write_all(line.as_bytes()).await?;
        }
        Ok(())
    }
}

pub struct OrionLdSink {
    pub base_url: String,
    pub tenant: Option<String>,
    pub token: Option<String>,
    http: reqwest::Client,
}

impl OrionLdSink {
    pub fn new(base_url: impl Into<String>, tenant: Option<String>, token: Option<String>) -> Self {
        Self {
            base_url: base_url.into(),
            tenant,
            token,
            http: reqwest::Client::new(),
        }
    }

    fn build_entities_payload(&self, rows: &[H3DailyRow]) -> serde_json::Value {
        let entities: Vec<serde_json::Value> = rows
            .iter()
            .map(|r| {
                let id = format!("urn:ngsi-ld:H3Delay:{}:{}", r.date, r.h3);
                json!({
                  "id": id,
                  "type": "H3Delay",
                  "date": { "type":"Property", "value": r.date.to_string() },
                  "h3": { "type":"Property", "value": r.h3.to_string() },
                  "res": { "type":"Property", "value": r.res },
                  "tripsTotal": { "type":"Property", "value": r.trips_total },
                  "tripsTrucks": { "type":"Property", "value": r.trips_trucks },
                  "tripsCars": { "type":"Property", "value": r.trips_cars },
                  "truckShare": { "type":"Property", "value": r.truck_share },
                  "volNorm": { "type":"Property", "value": r.vol_norm },
                  "conf": { "type":"Property", "value": r.conf_cell },
                  "delayOrange": { "type":"Property", "value": r.delay_orange },
                  "delayTomTom": { "type":"Property", "value": r.delay_tomtom },
                  "delayFinal": { "type":"Property", "value": r.delay_final },
                })
            })
            .collect();

        json!({ "actionType": "append_strict", "entities": entities })
    }
}

#[async_trait]
impl HistorySink for OrionLdSink {
    async fn persist(&self, rows: &[H3DailyRow]) -> anyhow::Result<()> {
        if rows.is_empty() { return Ok(()); }
        let url = format!("{}/ngsi-ld/v1/entityOperations/upsert", self.base_url.trim_end_matches('/'));
        let payload = self.build_entities_payload(rows);

        let mut req = self.http.post(&url)
            .header("Content-Type", "application/ld+json");
        if let Some(t) = &self.tenant {
            req = req.header("NGSILD-Tenant", t);
        }
        if let Some(tok) = &self.token {
            req = req.bearer_auth(tok);
        }

        let resp = req.json(&payload).send().await?;
        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(anyhow!("Orion-LD upsert failed: {} — {}", status, body));
        }
        Ok(())
    }
}

// ===============================
// Utilidades geom/geojson
// ===============================

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
// fn cell_center_deg(c: CellIndex) -> (f64, f64) {
//     let ll: LatLng = c.into();
//     (ll.lng(), ll.lat())
// }

pub fn cell_polygon_coords(c: CellIndex) -> Vec<[f64; 2]> {
    let verts = c.boundary();
    let mut coords: Vec<[f64; 2]> = verts.iter().map(|ll| [ll.lng(), ll.lat()]).collect();
    if let Some(first) = coords.first().cloned() {
        if coords.last() != Some(&first) {
            coords.push(first);
        }
    }
    coords
}

// ===============================
// Núcleo: agregación O/D y delay
// ===============================

pub fn aggregate_od_to_h3(records: &[ODRecord], cfg: &DelayCfg) -> Result<HashMap<CellIndex, H3Metrics>> {
    let mut map: HashMap<CellIndex, H3Metrics> = HashMap::new();
    let res = Resolution::try_from(cfg.res)?;

    for r in records {
        let o = CellIndex::from_str(&r.origin_h3).context("origin_h3 inválido")?;
        let d = CellIndex::from_str(&r.dest_h3).context("dest_h3 inválido")?;

        if o.resolution() != res || d.resolution() != res {
            return Err(anyhow!("OD record con resolución distinta a cfg.res"));
        }

        let vol = r.n_trucks * cfg.truck_factor + r.n_cars * cfg.car_factor;
        let conf = r.conf.unwrap_or(1.0).clamp(0.0, 1.0);
        let w = vol.max(1.0);

        // Origen
        {
            let e = map.entry(o).or_insert_with(|| H3Metrics::new(o));
            e.trips_total += vol;
            e.trips_trucks += r.n_trucks;
            e.trips_cars += r.n_cars;
            e.conf_sum += conf * w;
            e.conf_weight += w;
        }
        // Destino
        {
            let e = map.entry(d).or_insert_with(|| H3Metrics::new(d));
            e.trips_total += vol;
            e.trips_trucks += r.n_trucks;
            e.trips_cars += r.n_cars;
            e.conf_sum += conf * w;
            e.conf_weight += w;
        }
    }

    Ok(map)
}


// ===============================
// Calculo delay orange
// ===============================

pub fn compute_delay_orange(metrics: &mut HashMap<CellIndex, H3Metrics>, cfg: &DelayCfg) {
    let eps = 1e-6_f32;

    // --- 1) Estadísticos base por ciudad/día ---
    //   a) vector de volúmenes por celda (trips_total ya pondera trucks según cfg.*_factor)
    let mut vols: Vec<f32> = metrics.values().map(|m| m.trips_total.max(0.0)).collect();
    let n = vols.len().max(1) as f32;

    // media para vol_norm (puro display/colores como ya usabas)
    let mean_vol = {
        let sum: f32 = vols.iter().copied().sum();
        (sum / n).max(eps)
    };

    //   b) capacidad c ≈ percentil P de la distribución de volúmenes
    //      (mantiene robustez frente a outliers y evita depender de red vial)
    let c = {
        vols.sort_by(|a,b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        let p = cfg.capacity_percentile.clamp(0.5, 0.999); // no dejes <0.5 para no subestimar capacidad
        let idx = ((vols.len().saturating_sub(1) as f32) * p).round() as usize;
        let perc = vols.get(idx).copied().unwrap_or(mean_vol);
        perc.max(cfg.capacity_floor).max(eps)
    };

    // --- 2) Cálculo por celda ---
    for m in metrics.values_mut() {
        // señales descriptivas que ya usabas (útiles para inspección/estilo)
        let total  = m.trips_total.max(eps);
        let trucks = m.trips_trucks.max(0.0);

        m.truck_share = (trucks / total).clamp(0.0, 1.0);
        m.vol_norm    = (total / mean_vol).clamp(0.0, 20.0);

        // --- 3) BPR-like ---
        // v/c acotado para estabilidad numérica
        let vc = (total / c).clamp(0.0, cfg.vc_cap);

        // factor de mezcla por camiones (penaliza capacidad efectiva)
        let hv_factor = 1.0 + cfg.truck_gamma * m.truck_share;

        // delay = 1 + a * (v/c)^b * (1 + γ * truck_share)
        let bpr_term = if vc > 0.0 { cfg.bpr_a * vc.powf(cfg.bpr_b) } else { 0.0 };
        let delay = 1.0 + bpr_term * hv_factor;

        m.delay_orange = clamp(delay, cfg.delay_min, cfg.delay_max);

        // inicializa delay_final con Orange; luego blending/override lo ajustará si hay TomTom
        m.delay_final = m.delay_orange;
    }
}

// ===============================
// Calculo delay mixto con proveedor externo
// ===============================


pub async fn enrich_with_traffic_provider(
    metrics: &mut HashMap<CellIndex, H3Metrics>,
    cfg: &DelayCfg,
    provider: &dyn TrafficProvider,
) -> Result<()> {
    let targets: Vec<CellIndex> = metrics
        .values()
        .filter(|m| m.conf_cell() < cfg.min_conf_for_pure_orange)
        .map(|m| m.cell)
        .collect();

    if targets.is_empty() {
        info!("No hay celdas de baja confianza; no se consulta provider.");
        return Ok(());
    }

    let sem = std::sync::Arc::new(tokio::sync::Semaphore::new(cfg.max_concurrent_calls.max(1)));
    let provider = std::sync::Arc::new(provider);

    let results = stream::iter(targets.into_iter().map(|cell| {
        let sem = sem.clone();
        let provider = provider.clone();
        async move {
            let _permit = sem.acquire().await.unwrap();
            let r = provider.delay_for_cell(cell).await;
            (cell, r)
        }
    }))
    .buffer_unordered(cfg.max_concurrent_calls.max(1))
    .collect::<Vec<_>>()
    .await;

    for (cell, r) in results {
        match r {
            Ok(Some((delay_tt, conf_tt))) => {
                if let Some(m) = metrics.get_mut(&cell) {
                    m.delay_tomtom = delay_tt.clamp(1.0, cfg.delay_max * 2.0);
                    let w_orange = m.conf_cell();
                    let w_tomtom = (1.0 - w_orange).clamp(0.0, 1.0);
                    let blended_base = w_orange * m.delay_orange + w_tomtom * m.delay_tomtom;
                    let blended = 0.5 * blended_base + 0.5 * (w_orange * m.delay_orange + w_tomtom * (m.delay_tomtom * conf_tt.max(0.5)));
                    m.delay_final = clamp(blended, cfg.delay_min, cfg.delay_max);
                }
            }
            Ok(None) => {
                debug!("Sin cobertura provider para {}", cell);
            }
            Err(e) => warn!("Provider error en {}: {}", cell, e),
        }
    }

    Ok(())
}

// ===============================
// Export: GeoJSON y rutina principal
// ===============================

pub fn to_geojson(metrics: &HashMap<CellIndex, H3Metrics>, cfg: &DelayCfg) -> String {
    let mut features = Vec::new();
    for (c, m) in metrics {
        let d = m.delay_final;
        //if d <= 1.0 + cfg.show_eps { continue; }

        let norm = ((d - 1.0) / (cfg.delay_max - 1.0)).clamp(0.0, 1.0);
        let col = color_from_norm(norm);
        let exterior = cell_polygon_coords(*c);

        let feat = json!({
            "type": "Feature",
            "geometry": { "type":"Polygon", "coordinates": [exterior] },
            "properties": {
                "h3": c.to_string(),
                "delay_final": ((d*100.0).round()/100.0),
                "delay_orange": ((m.delay_orange*100.0).round()/100.0),
                "delay_tomtom": ((m.delay_tomtom*100.0).round()/100.0),
                "vol_norm": ((m.vol_norm*100.0).round()/100.0),
                "truck_share": ((m.truck_share*100.0).round()/100.0),
                "used_tomtom": (m.delay_tomtom > 0.0),
                "conf": ((m.conf_cell()*100.0).round()/100.0),
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

pub async fn compute_day(
    date: NaiveDate,
    od: &[ODRecord],
    cfg: &DelayCfg,
    traffic: Option<&dyn TrafficProvider>,
    sink: Option<&dyn HistorySink>,
) -> Result<(HashMap<CellIndex, H3Metrics>, String)> {
    // 1) Agregación
    let mut map = aggregate_od_to_h3(od, cfg)?;

    // 2) Delay Orange
    compute_delay_orange(&mut map, cfg);

    // 3) Enriquecimiento Traffic Provider
    if let Some(tp) = traffic {
        enrich_with_traffic_provider(&mut map, cfg, tp).await?;
    }

    // 4) Persistencia histórica
    if let Some(s) = sink {
        let rows: Vec<H3DailyRow> = map
            .values()
            .map(|m| H3DailyRow {
                date,
                h3: m.cell,
                res: cfg.res,
                trips_total: m.trips_total,
                trips_trucks: m.trips_trucks,
                trips_cars: m.trips_cars,
                truck_share: m.truck_share,
                vol_norm: m.vol_norm,
                conf_cell: m.conf_cell(),
                delay_orange: m.delay_orange,
                delay_tomtom: m.delay_tomtom,
                delay_final: m.delay_final,
            })
            .collect();
        s.persist(&rows).await?;
    }

    // 5) GeoJSON
    let gj = to_geojson(&map, cfg);
    Ok((map, gj))
}

// ===============================
// Tests rápidos
// ===============================

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;

    #[tokio::test]
    async fn smoke_pipeline() -> Result<()> {
        let res = 7;
        let center = LatLng::new(42.4627, -2.44498).unwrap();
        let c: CellIndex = center.to_cell(Resolution::try_from(res).unwrap());
        let h = c.to_string();

        let od = vec![
            ODRecord {
                date: NaiveDate::from_ymd_opt(2025, 10, 27).unwrap(),
                origin_h3: h.clone(),
                dest_h3: h.clone(),
                n_trucks: 120.0,
                n_cars: 800.0,
                conf: Some(0.8),
            }
        ];
        let mut cfg = DelayCfg::default();
        cfg.res = res;
        let (_map, gj) = compute_day(od[0].date, &od, &cfg, None, None).await?;
        assert!(gj.contains("FeatureCollection"));
        Ok(())
    }
}
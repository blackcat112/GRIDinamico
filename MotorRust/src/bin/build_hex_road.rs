//! build_hex_road_map.rs
//! Genera "hex_road_map_logrono.csv" con la relación H3 (res configurable) ↔ vías OSM (Overpass).
//! Columnas: h3_cell, road_count, total_length_m, avg_lat, avg_lon, primary_ratio
//! Uso: cargo run --bin build_hex_road_map

use anyhow::{Context, Result};
use geo::{HaversineLength, LineString};
use h3o::{CellIndex, LatLng, Resolution};
use reqwest::blocking::Client;
use serde_json::Value;
use std::collections::HashMap;
use std::fs::File;
use std::io::Write;

// ================== Config ==================
const H3_RES: u8 = 7; // ~1 km
// south,west,north,east (Logroño aprox)
const CITY_BBOX: &str = "42.448,-2.509,42.490,-2.417";
const OUT_CSV: &str = "hex_road_map_logrono.csv";

// ================== Aux =====================
#[derive(Default)]
struct RoadStats {
    count: usize,
    total_len_m: f64,
    sum_lat: f64,
    sum_lon: f64,
    primaries: usize,
}
impl RoadStats {
    fn add_segment(&mut self, length_m: f64, lat: f64, lon: f64, is_primary: bool) {
        self.count += 1;
        self.total_len_m += length_m;
        self.sum_lat += lat;
        self.sum_lon += lon;
        if is_primary {
            self.primaries += 1;
        }
    }
    fn avg_latlon(&self) -> (f64, f64) {
        if self.count == 0 {
            (0.0, 0.0)
        } else {
            (self.sum_lat / self.count as f64, self.sum_lon / self.count as f64)
        }
    }
}

// ================== Main ====================
fn main() -> Result<()> {
    let client = Client::builder()
        .timeout(std::time::Duration::from_secs(180))
        .build()
        .context("No se pudo construir el cliente HTTP")?;

    println!("📡 Descargando red vial (OSM/Overpass) para Logroño…");

    // 1) Overpass query
    let parts: Vec<&str> = CITY_BBOX.split(',').collect();
    let (south, west, north, east) = (parts[0], parts[1], parts[2], parts[3]);
    let query = format!(
        r#"[out:json][timeout:180];
        (
          way["highway"](bbox:{south},{west},{north},{east});
        );
        out geom tags;"#,
    );
    let resp = client
        .post("https://overpass-api.de/api/interpreter")
        .form(&[("data", &query)])
        .send()
        .context("Fallo al llamar a Overpass")?;
    let v: Value = resp.json().context("JSON inválido de Overpass")?;

    // 2) Asociar vías a celdas H3
    println!("🧮 Calculando celdas H3 y acumulando métricas…");
    let res: Resolution = Resolution::try_from(H3_RES)
        .context("H3_RES inválida para h3o::Resolution")?;
    let mut hex_stats: HashMap<CellIndex, RoadStats> = HashMap::new();

    if let Some(elements) = v["elements"].as_array() {
        for el in elements {
            if el["type"] != "way" {
                continue;
            }

            let highway = el["tags"]["highway"].as_str().unwrap_or("");
            let is_primary = matches!(
                highway,
                "primary" | "secondary" | "tertiary" | "trunk" | "motorway"
            );

            let Some(geom) = el["geometry"].as_array() else { continue };

            // Overpass devuelve pares {lat, lon}. Construimos coords (lon,lat) para geo::LineString
            let coords_lonlat: Vec<(f64, f64)> = geom
                .iter()
                .filter_map(|n| Some((n["lon"].as_f64()?, n["lat"].as_f64()?)))
                .collect();
            if coords_lonlat.len() < 2 {
                continue;
            }

            // Longitud del tramo
            let line = LineString::from(coords_lonlat.clone());
            let length_m: f64 = line.haversine_length();

            // Para cada punto del tramo, obtenemos su celda H3 y acumulamos
            for (lon, lat) in coords_lonlat {
                let ll = LatLng::new(lat, lon).expect("LatLng válido");
                let cell: CellIndex = ll.to_cell(res); // ✅ API correcta de h3o
                hex_stats
                    .entry(cell)
                    .or_default()
                    .add_segment(length_m, lat, lon, is_primary);
            }
        }
    }

    println!("✅ {} celdas con vías procesadas.", hex_stats.len());

    // 3) Export CSV
    let mut f = File::create(OUT_CSV).context("No se pudo crear el CSV de salida")?;
    writeln!(
        f,
        "h3_cell,road_count,total_length_m,avg_lat,avg_lon,primary_ratio"
    )?;
    for (cell, s) in &hex_stats {
        let (avg_lat, avg_lon) = s.avg_latlon();
        let primary_ratio = if s.count == 0 {
            0.0
        } else {
            s.primaries as f64 / s.count as f64
        };
        writeln!(
            f,
            "{},{},{:.2},{:.6},{:.6},{:.2}",
            cell,
            s.count,
            s.total_len_m,
            avg_lat,
            avg_lon,
            primary_ratio
        )?;
    }

    println!("💾 Guardado en {}", OUT_CSV);
    Ok(())
}

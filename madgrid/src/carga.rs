use crate::types::ParkingZone;
use crate::utm::utm30_to_wgs84;

pub fn parse_carga_csv(raw: &str) -> Vec<ParkingZone> {
// El CSV viene con ';' y a veces trae un header con "Gis_X" o similar.
let mut out = Vec::new();
for (i, line) in raw.lines().enumerate() {
let line = line.trim();
if line.is_empty() { continue; }
// detectar header por la primera palabra
if i == 0 && (line.contains("Gis_X") || line.to_lowercase().contains("gis_x")) { continue; }
let parts: Vec<&str> = line.split(';').collect();
if parts.len() < 7 { continue; }
let estado = parts[0].trim().to_string();
let x = parts[1].trim().replace(',', ".");
let y = parts[2].trim().replace(',', ".");
let distrito = parts[4].trim().to_string();
let barrio = parts[5].trim().to_string();
let calle = parts[6].trim().to_string();
let x: f64 = x.parse().unwrap_or(f64::NAN);
let y: f64 = y.parse().unwrap_or(f64::NAN);
if !x.is_finite() || !y.is_finite() { continue; }
let (lat, lon) = utm30_to_wgs84(x, y);
out.push(ParkingZone { lat: lat as f32, lon: lon as f32, calle, distrito, barrio, estado });
}
out
}
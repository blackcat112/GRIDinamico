//! utm.rs
//!
//! Conversión de coordenadas UTM 30N ↔ WGS84 (lat/lon).
//!
//! - Necesario porque los datos del Ayuntamiento de Madrid
//!   suelen venir en EPSG:25830 (UTM huso 30N).
//! - Aquí se implementan funciones de transformación a EPSG:4326.
//!
//! Permite unificar todas las fuentes de datos a coordenadas
//! compatibles con Leaflet y GeoJSON.


pub fn utm30_to_wgs84(x: f64, y: f64) -> (f64, f64) {
    let a = 6378137.0_f64;
    let e = 0.08181919084262149_f64;
    let k0 = 0.9996_f64;
    let zone = 30.0_f64;
    let long_origin = (zone - 1.0) * 6.0 - 180.0 + 3.0;
    
    let x = x - 500000.0;
    let m = y / k0;
    let mu = m / (a * (1.0 - e * e / 4.0 - 3.0 * e.powi(4) / 64.0 - 5.0 * e.powi(6) / 256.0));
    let e1 = (1.0 - (1.0 - e * e).sqrt()) / (1.0 + (1.0 - e * e).sqrt());
    let j1 = 3.0 * e1 / 2.0 - 27.0 * e1.powi(3) / 32.0;
    let j2 = 21.0 * e1.powi(2) / 16.0 - 55.0 * e1.powi(4) / 32.0;
    let j3 = 151.0 * e1.powi(3) / 96.0;
    let j4 = 1097.0 * e1.powi(4) / 512.0;
    let fp = mu + j1 * (2.0 * mu).sin() + j2 * (4.0 * mu).sin() + j3 * (6.0 * mu).sin() + j4 * (8.0 * mu).sin();
    let e2 = e * e / (1.0 - e * e);
    let c1 = e2 * fp.cos().powi(2);
    let t1 = fp.tan().powi(2);
    let r1 = a * (1.0 - e * e) / (1.0 - (e * fp.sin()).powi(2)).powf(1.5);
    let n1 = a / (1.0 - (e * fp.sin()).powi(2)).sqrt();
    let d = x / (n1 * k0);
    
    let lat = fp - (n1 * fp.tan() / r1)
    * (d * d / 2.0 - (5.0 + 3.0 * t1 + 10.0 * c1 - 4.0 * c1 * c1 - 9.0 * e2) * d.powi(4) / 24.0
    + (61.0 + 90.0 * t1 + 298.0 * c1 + 45.0 * t1 * t1 - 252.0 * e2 - 3.0 * c1 * c1) * d.powi(6) / 720.0);
    let lon = (d - (1.0 + 2.0 * t1 + c1) * d.powi(3) / 6.0
    + (5.0 - 2.0 * c1 + 28.0 * t1 - 3.0 * c1 * c1 + 8.0 * e2 + 24.0 * t1 * t1) * d.powi(5) / 120.0) / fp.cos();
    
    (lat.to_degrees(), long_origin + lon.to_degrees())
    }
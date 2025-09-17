//! types.rs
//! Modelos de datos compartidos por el servicio: entradas (sensores/incidencias)
//! configuración del calculo, KPIs y salidas 

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ParkingZone {
    pub lat: f32,
    pub lon: f32,
    pub calle: String,
    pub distrito: String,
    pub barrio: String,
    pub estado: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Incidencia {
    pub lat: f32,
    pub lon: f32,
    pub estado: String,
    pub inicio: Option<String>,
    pub fin: Option<String>,
    pub tipo: String,
    pub descripcion: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SensorTr {
    pub id: u32,
    pub lat: f32,
    pub lon: f32,
    pub intensidad: Option<f32>,
    pub ocupacion: Option<f32>,
    pub carga: Option<f32>,
    pub nivel: Option<f32>,
    pub vel: Option<f32>,
    pub ts_ms: i64,
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct Kpis { pub carga: usize, pub inc: usize }

#[derive(Clone, Debug, Serialize)]
pub struct DelayCfg {
    pub w_carga: f32,
    pub w_nivel: f32,
    pub w_velinv: f32,
    pub w_ocup: f32,
    pub vel_free: f32,
    pub ocup_sat: f32,
    pub min_sens_ok: u8,
    pub min_sens_any: u8,
    pub delay_min: f32,
    pub delay_max: f32,
    pub inc_cap: f32,
    pub show_eps: f32,
    pub park_radius_m: f32,
    pub park_count_norm: f32,
    pub park_w_count: f32,
    pub park_w_dist: f32,
    pub park_base_min: f32,
    pub park_gain: f32,
}

impl Default for DelayCfg {
    fn default() -> Self {
        Self {
            w_carga: 0.35,
            w_nivel: 0.25,
            w_velinv: 0.30,
            w_ocup: 0.10,
            vel_free: 40.0,
            ocup_sat: 85.0,
            min_sens_ok: 3,
            min_sens_any: 1,
            delay_min: 0.90,
            delay_max: 2.50,
            inc_cap: 0.80,
            show_eps: 0.03,
            park_radius_m: 150.0,
            park_count_norm: 4.0,
            park_w_count: 0.7,
            park_w_dist: 0.3,
            park_base_min: 8.0,
            park_gain: 0.75,
        }
    }
}

#[derive(Clone, Debug)]
pub struct AppCfg {
    pub bind: String, 
    pub url_carga: String,
    pub url_incid: String,
    pub url_trafico: String,
    pub t_carga_s: u64,
    pub t_incid_s: u64,
    pub t_trafico_s: u64,
}

impl Default for AppCfg {
    fn default() -> Self {
        Self {
            bind: "0.0.0.0:8080".into(),
            url_carga: "https://datos.madrid.es/egobfiles/MANUAL/208072/carga_descarga_2025.csv".into(),
            url_incid: "https://informo.madrid.es/informo/tmadrid/incid_aytomadrid.xml".into(),
            url_trafico: "https://informo.madrid.es/informo/tmadrid/pm.xml".into(),
            t_carga_s: 3600,
            t_incid_s: 300,
            t_trafico_s: 300,
        }
    }
}


#[derive(Clone, Debug, Serialize)]
pub struct RoutingCell {
    pub h3: String,  
    pub delay: f32, 
    pub coordinates: Vec<[f64; 2]>, 
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct DataState {
    pub cargas: Vec<ParkingZone>,
    pub incs: Vec<Incidencia>,
    pub traf: Vec<SensorTr>,
    pub kpis: Kpis,

    pub hex_geojson: String,

    pub routing_cells: Vec<RoutingCell>,

    pub delay_cfg: DelayCfg,

    pub snapshot_ts_utc: String,
}

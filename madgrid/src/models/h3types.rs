use serde::{Deserialize, Serialize};
use serde_with::{serde_as, DisplayFromStr};
use chrono::NaiveDate;
use h3o::CellIndex;
use std::time::Duration;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DelayCfg {
    /// Resolución H3 de trabajo (p.ej. 7 ~ 1 km²)
    pub res: u8,

    /// Peso del volumen normalizado en el delay_orange
    pub alpha_vol: f32,
    /// Peso del mix de camiones en el delay_orange
    pub beta_truck_mix: f32,

    /// Mínimo y máximo del delay
    pub delay_min: f32,
    pub delay_max: f32,

    /// Umbral de confianza por debajo del cual activamos fallback TomTom
    pub min_conf_for_pure_orange: f32,

    /// Concurrencia máx. para llamadas externas (TomTom)
    pub max_concurrent_calls: usize,

    /// Opcional: factor de ponderación de camiones frente a coches (para volumen)
    pub truck_factor: f32,
    pub car_factor: f32,

    /// Mostrar solo delays > 1 + eps en GeoJSON
    pub show_eps: f32,

     // --- nuevo ---
     pub bpr_a: f32,
     pub bpr_b: f32,
     pub truck_gamma: f32,
     pub capacity_percentile: f32,
     pub capacity_floor: f32,
     pub vc_cap: f32,
}

/// Registro de O/D para un día (csv/parquet)
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ODRecord {
    /// Fecha del día (naive, sin TZ) — recomendado: "YYYY-MM-DD"
    pub date: NaiveDate,
    /// Celda origen (ID H3 en texto). Se asume a la misma resolución que `cfg.res`
    pub origin_h3: String,
    /// Celda destino (ID H3 en texto).
    pub dest_h3: String,
    /// Conteos diarios
    pub n_trucks: f32,
    pub n_cars: f32,
    /// Confianza 0..1 asociada al dato (si no llega, usar 1.0 por defecto)
    pub conf: Option<f32>,
}

#[derive(Clone, Debug)]
pub struct H3Metrics {
    pub cell: CellIndex,

    // volumen
    pub trips_total: f32,
    pub trips_trucks: f32,
    pub trips_cars: f32,

    // confianza (media ponderada por volumen)
    pub conf_sum: f32,
    pub conf_weight: f32,

    // señales de salida
    pub delay_orange: f32,
    pub delay_tomtom: f32,
    pub delay_final: f32,

    // auxiliares
    pub truck_share: f32,
    pub vol_norm: f32,
}

/// Fila histórica por celda (para sinks)

#[serde_as]
#[derive(Clone, Debug, Serialize)]
pub struct H3DailyRow {
    pub date: NaiveDate,
    #[serde_as(as = "DisplayFromStr")]
    pub h3: CellIndex,
    pub res: u8,
    pub trips_total: f32,
    pub trips_trucks: f32,
    pub trips_cars: f32,
    pub truck_share: f32,
    pub vol_norm: f32,
    pub conf_cell: f32,
    pub delay_orange: f32,
    pub delay_tomtom: f32,
    pub delay_final: f32,
}


/// Cliente TomTom Flow Segment Data (FSD)
pub struct TomTomClient {
    pub(crate) http: reqwest::Client,
    pub api_key: String,
    /// Endpoint base, p.ej: "https://api.tomtom.com/traffic/services/4/flowSegmentData/absolute/10/json"
    pub base_url_absolute: String,
    /// Timeout para cada request
    pub timeout: Duration,
}

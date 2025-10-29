//! main.rs — Pipeline O/D + TomTom + históricos (sin ENV)

mod models;
mod server;
mod h3grid;
mod clusterizador;


use anyhow::{Context, Result};
use once_cell::sync::Lazy;
use reqwest::Client;
use std::{sync::Arc, time::Duration};
use tokio::{signal, sync::RwLock, time::sleep};
use tracing::{info, warn, Level};

use chrono::NaiveDate;
use models::types::{AppCfg, DataState, DelayCfg};
use models::h3types::{ DelayCfg as ODDelayCfg,ODRecord,TomTomClient};
use h3grid::{
    compute_day, HistorySink, JsonlSink, OrionLdSink,
    TrafficProvider,load_roadmap_csv
};

#[allow(dead_code)]
static CFG: Lazy<DelayCfg> = Lazy::new(|| DelayCfg::default());

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter("info")
        .with_max_level(Level::INFO)
        .init();

    // Carga config por defecto desde tu models::types::AppCfg::default()
    let cfg = AppCfg::default();

    // Estado compartido para la API
    let data = Arc::new(RwLock::new(DataState {
        delay_cfg: DelayCfg::default(), // si tu DataState lo sigue usando para algo
        ..Default::default()
    }));

    // HTTP client con compresion
    let client = Client::builder().brotli(true).gzip(true).deflate(true).build()?;

    // Lanza el loop de O/D -> compute_day -> actualizar estado
    {
        let data_c = data.clone();
        let client_c = client.clone();
        let cfg_c = cfg.clone();
        tokio::spawn(async move { fetch_loop_od(client_c, data_c, cfg_c).await; });
    }

    // API
    let app = server::api::router(server::api::ApiState { data: data.clone() });
    info!("Escuchando en http://{}", cfg.bind);
    let listener = tokio::net::TcpListener::bind(&cfg.bind).await?;
    let serve = axum::serve(listener, app);
    tokio::select! {
        r = serve => { r?; },
        _ = signal::ctrl_c() => { info!("Señal de salida recibida"); }
    }

    Ok(())
}

// --------------------------------------
// Loop OD: descarga -> parse -> compute_day -> estado
// --------------------------------------
async fn fetch_loop_od(client: Client, data: Arc<RwLock<DataState>>, cfg: AppCfg) {
    let mut cache = server::fetch::CacheCtl::default();

    // Mapear AppCfg -> DelayCfg del h3grid
    let mut od_cfg = ODDelayCfg::default();
    od_cfg.res = cfg.h3_res;
    od_cfg.min_conf_for_pure_orange = cfg.min_conf_orange;
    od_cfg.max_concurrent_calls = cfg.max_concurrent;

    // 1) Cargar roadmap CSV (una vez)
    let road_map = load_roadmap_csv("data/hex_road_map_logrono.csv").ok();
    // Provider TomTom (opcional)
    let tomtom: Option<TomTomClient> = cfg
    .tomtom_key
    .clone()
    .map(|key| TomTomClient::new(key, road_map.clone()));

    // Sinks (opcional): prioriza Orion si está, si no JSONL
    let orion = cfg
        .orion_url
        .as_ref()
        .map(|url| OrionLdSink::new(url.clone(), cfg.orion_tenant.clone(), None));
    let jsonl = cfg.jsonl_out.as_ref().map(|p| JsonlSink::new(p));

    loop {
        if let Err(e) = async {
            // 1) DESCARGA O/D (CSV)
            let od_url = &cfg.od_url;
            if let Some(bytes) =
                server::fetch::get_with_cache(&client, od_url, &mut cache).await?
            {
                // 2) PARSE CSV -> Vec<ODRecord>
                let mut rdr =
                    csv::ReaderBuilder::new().has_headers(true).from_reader(&*bytes);
                let mut od_rows: Vec<ODRecord> = Vec::new();

                for rec in rdr.deserialize::<ODRecord>() {
                    let mut r = rec.context("OD CSV parse")?;
                    if !valid_date(&r.date) {
                        r.date = chrono::Utc::now().date_naive();
                    }
                    od_rows.push(r);
                }

                // 3) EXEC COMPUTE-DAY
                let date: NaiveDate = od_rows
                    .get(0)
                    .map(|r| r.date)
                    .unwrap_or_else(|| chrono::Utc::now().date_naive());

                let provider_ref: Option<&dyn TrafficProvider> =
                    tomtom.as_ref().map(|t| t as &dyn TrafficProvider);
                let sink_orion: Option<&dyn HistorySink> =
                    orion.as_ref().map(|o| o as &dyn HistorySink);
                let sink_jsonl: Option<&dyn HistorySink> =
                    jsonl.as_ref().map(|j| j as &dyn HistorySink);
                let sink = sink_orion.or(sink_jsonl);

                let (_map, geojson) =
                    compute_day(date, &od_rows, &od_cfg, provider_ref, sink)
                        .await
                        .context("compute_day failed")?;

                // 4) ACTUALIZA ESTADO COMPARTIDO PARA LA API
                {
                    let mut d = data.write().await;
                    d.hex_geojson = geojson;
                    d.snapshot_ts_utc = chrono::Utc::now().to_rfc3339();
                }
                info!("OD recompute OK: date={date}, cells actualizadas");
            }
            Ok::<_, anyhow::Error>(())
        }
        .await
        {
            warn!("od_loop: {e:?}");
        }

        // 5) ESPERA
        sleep(Duration::from_secs(cfg.t_od_s)).await;
    }
}

#[inline]
fn valid_date(d: &NaiveDate) -> bool {
    d >= &NaiveDate::from_ymd_opt(1971, 1, 1).unwrap()
}

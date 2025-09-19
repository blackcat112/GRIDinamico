//! main.rs
//! Arranque, loops de fetch, recompute H3, API y cache de salidas.

mod types;
mod utm;       
mod fetch;
mod carga;
mod incid;
mod trafico;
mod api;
mod h3grid;
//mod s2grid;  


use anyhow::Result;
use reqwest::Client;
use std::{env, sync::Arc, time::Duration};
use tokio::{signal, sync::RwLock, time::sleep};
use tracing::{info, Level};
use once_cell::sync::Lazy;

use types::{AppCfg, DataState, DelayCfg};
#[allow(dead_code)]
static CFG: Lazy<DelayCfg> = Lazy::new(|| DelayCfg::default());

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt().with_env_filter("info").with_max_level(Level::INFO).init();

    let cfg = app_cfg_from_env();
    let data = Arc::new(RwLock::new(DataState {
        delay_cfg: DelayCfg::default(),
        ..Default::default()
    }));

    // HTTP client con compresión
    let client = Client::builder().brotli(true).gzip(true).deflate(true).build()?;

    // Lanzar fetchers
    let data_c = data.clone(); let client_c = client.clone(); let cfg_c = cfg.clone();
    tokio::spawn(async move { fetch_loop_carga(client_c, data_c, cfg_c).await; });

    let data_i = data.clone(); let client_i = client.clone(); let cfg_i = cfg.clone();
    tokio::spawn(async move { fetch_loop_incid(client_i, data_i, cfg_i).await; });

    let data_t = data.clone(); let client_t = client.clone(); let cfg_t = cfg.clone();
    tokio::spawn(async move { fetch_loop_trafico(client_t, data_t, cfg_t).await; });

    // API
    let app = api::router(api::ApiState { data: data.clone() });
    info!("Escuchando en http://{}", cfg.bind);
    let listener = tokio::net::TcpListener::bind(&cfg.bind).await?;
    let serve = axum::serve(listener, app);
    tokio::select! {
        r = serve => { r?; },
        _ = signal::ctrl_c() => { info!("Señal de salida recibida"); }
    }

    Ok(())
}

fn app_cfg_from_env() -> AppCfg {
    let mut c = AppCfg::default();
    if let Ok(v) = env::var("BIND") { c.bind = v; }
    if let Ok(v) = env::var("URL_CARGA") { c.url_carga = v; }
    if let Ok(v) = env::var("URL_INCID") { c.url_incid = v; }
    if let Ok(v) = env::var("URL_TRAFICO") { c.url_trafico = v; }
    if let Ok(v) = env::var("T_CARGA_S") { c.t_carga_s = v.parse().unwrap_or(c.t_carga_s); }
    if let Ok(v) = env::var("T_INCID_S") { c.t_incid_s = v.parse().unwrap_or(c.t_incid_s); }
    if let Ok(v) = env::var("T_TRAFICO_S") { c.t_trafico_s = v.parse().unwrap_or(c.t_trafico_s); }
    c
}

// ------------------------
// Fetch loops + recompute
// ------------------------
async fn fetch_loop_carga(client: Client, data: Arc<RwLock<DataState>>, cfg: AppCfg) {
    let mut cache = fetch::CacheCtl::default();
    loop {
        if let Err(e) = async {
            if let Some(bytes) = fetch::get_with_cache(&client, &cfg.url_carga, &mut cache).await? {
                let txt = String::from_utf8_lossy(&bytes);
                let zonas = carga::parse_carga_csv(&txt);
                {
                    let mut d = data.write().await; d.cargas = zonas; d.kpis.carga = d.cargas.len();
                }
                recompute_all(&data).await;
            }
            Ok::<_, anyhow::Error>(())
        }.await { tracing::warn!("carga: {e:?}"); }
        sleep(Duration::from_secs(cfg.t_carga_s)).await;
    }
}

async fn fetch_loop_incid(client: Client, data: Arc<RwLock<DataState>>, cfg: AppCfg) {
    let mut cache = fetch::CacheCtl::default();
    loop {
        if let Err(e) = async {
            if let Some(bytes) = fetch::get_with_cache(&client, &cfg.url_incid, &mut cache).await? {
                let incs = incid::parse_incidencias_xml(&bytes);
                {
                    let mut d = data.write().await; d.incs = incs; d.kpis.inc = d.incs.len();
                }
                recompute_all(&data).await;
            }
            Ok::<_, anyhow::Error>(())
        }.await { tracing::warn!("incid: {e:?}"); }
        sleep(Duration::from_secs(cfg.t_incid_s)).await;
    }
}

async fn fetch_loop_trafico(client: Client, data: Arc<RwLock<DataState>>, cfg: AppCfg) {
    let mut cache = fetch::CacheCtl::default();
    loop {
        if let Err(e) = async {
            if let Some(bytes) = fetch::get_with_cache(&client, &cfg.url_trafico, &mut cache).await? {
                let sensores = trafico::parse_trafico_xml(&bytes);
                {
                    let mut d = data.write().await; d.traf = sensores;
                }
                recompute_all(&data).await;
            }
            Ok::<_, anyhow::Error>(())
        }.await { tracing::warn!("trafico: {e:?}"); }
        sleep(Duration::from_secs(cfg.t_trafico_s)).await;
    }
}

async fn recompute_all(data: &Arc<RwLock<DataState>>) {
    let (cargas, incs, traf, cfg) = {
        let d = data.read().await;
        (d.cargas.clone(), d.incs.clone(), d.traf.clone(), d.delay_cfg.clone())
    };
    let base_res: u8 = 9;
    let refine = Some((7, 1.15));   // parent res=7 y refinamos hijos cuando delay>1.15 o blocked
    let k_smooth = 1;               
    let min_delay_export = 1.03;    

    let out = h3grid::recompute_h3(&cargas, &incs, &traf, &cfg, base_res, refine, k_smooth, min_delay_export);

    let mut d = data.write().await;
    d.hex_geojson = out.geojson;
    d.routing_cells = out.routing;
    d.snapshot_ts_utc = out.ts_utc;
}

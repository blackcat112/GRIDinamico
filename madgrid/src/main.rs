mod types; mod utm; mod fetch; mod carga; mod incid; mod trafico; mod grid; mod api;
// Si tienes h3grid.rs, su mod aquí: mod h3grid;

use anyhow::Result;
use reqwest::Client;
use std::{env, sync::Arc, time::Duration};
use tokio::{signal, sync::RwLock, time::sleep};
use tracing::{info, Level};
use once_cell::sync::Lazy;

use types::{AppCfg, DataState, DelayCfg};

static CFG: Lazy<types::DelayCfg> = Lazy::new(|| types::DelayCfg::default());

#[tokio::main]
async fn main() -> Result<()> {
    // Logs
    tracing_subscriber::fmt().with_env_filter("info").with_max_level(Level::INFO).init();

    // ⚙️ Config desde env (OJO: renombrado a app_cfg para no chocar con macro `cfg`)
    let app_cfg = app_cfg_from_env();
    let _delay_cfg = DelayCfg::default();

    // Carga grid clásico (si sigues usando grid.rs para algo)
    info!("Cargando grid: {}", app_cfg.grid_path);
    let _grid = grid::GridIndex::from_geojson(&app_cfg.grid_path)?;
    let data = Arc::new(RwLock::new(DataState::default()));

    // HTTP client con compresión
    let client = Client::builder().brotli(true).gzip(true).deflate(true).build()?;

    // Lanzar fetchers
    {
        let data_c = data.clone(); let client_c = client.clone(); let cfg_c = app_cfg.clone();
        tokio::spawn(async move { fetch_loop_carga(client_c, data_c, cfg_c).await; });
    }
    {
        let data_i = data.clone(); let client_i = client.clone(); let cfg_i = app_cfg.clone();
        tokio::spawn(async move { fetch_loop_incid(client_i, data_i, cfg_i).await; });
    }
    {
        let data_t = data.clone(); let client_t = client.clone(); let cfg_t = app_cfg.clone();
        tokio::spawn(async move { fetch_loop_trafico(client_t, data_t, cfg_t).await; });
    }

    // API
    let app = api::router(api::ApiState { data: data.clone() });
    info!("Escuchando en http://{}", app_cfg.bind);
    let listener = tokio::net::TcpListener::bind(&app_cfg.bind).await?;
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
    if let Ok(v) = env::var("HEX_GRID_PATH") { c.grid_path = v; }
    if let Ok(v) = env::var("URL_CARGA") { c.url_carga = v; }
    if let Ok(v) = env::var("URL_INCID") { c.url_incid = v; }
    if let Ok(v) = env::var("URL_TRAFICO") { c.url_trafico = v; }
    if let Ok(v) = env::var("T_CARGA_S") { c.t_carga_s = v.parse().unwrap_or(c.t_carga_s); }
    if let Ok(v) = env::var("T_INCID_S") { c.t_incid_s = v.parse().unwrap_or(c.t_incid_s); }
    if let Ok(v) = env::var("T_TRAFICO_S") { c.t_trafico_s = v.parse().unwrap_or(c.t_trafico_s); }
    c
}

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

// Recalcula usando lo que tengas debajo (grid clásico o H3 dentro de recompute_all)
async fn recompute_all(data: &Arc<RwLock<DataState>>) {
    let (cargas, incs, traf) = {
        let d = data.read().await;
        (d.cargas.clone(), d.incs.clone(), d.traf.clone())
    };

    // Si usas H3: llama a tu recompute_h3 aquí y rellena d.hex_geojson_str y d.cells_out
    // Ejemplo:
    // let (_cells, fc) = h3grid::recompute_h3(&cargas, &incs, &traf, &CFG, base_res, refine);
    // let mut d = data.write().await;
    // d.cells_out = _cells;
    // d.hex_geojson_str = serde_json::to_string(&fc).unwrap_or("{\"type\":\"FeatureCollection\",\"features\":[]}".into());

    // Si mientras tanto sigues con el grid clásico:
    let (_outs, fc) = grid::recompute(&GRID, &cargas, &incs, &traf, &CFG);
    let mut d = data.write().await;
    d.cells_out = _outs;
    d.hex_geojson_str = serde_json::to_string(&fc).unwrap_or("{\"type\":\"FeatureCollection\",\"features\":[]}".into());
}

// Si sigues usando GRID estático (para grid clásico). Si no, elimina esto.
use once_cell::sync::Lazy as _;
static GRID: Lazy<grid::GridIndex> = Lazy::new(|| {
    let p = std::env::var("HEX_GRID_PATH").unwrap_or_else(|_| "data/hex_grid_madrid_300m.geojson".into());
    grid::GridIndex::from_geojson(&p).expect("Cargar grid")
});

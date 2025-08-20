use anyhow::Result;
use reqwest::{Client, StatusCode};
use bytes::Bytes; // ⬅️ añade esto

#[derive(Default, Clone)]
pub struct CacheCtl { pub etag: Option<String>, pub last_mod: Option<String> }

pub async fn get_with_cache(client: &Client, url: &str, cache: &mut CacheCtl) -> Result<Option<bytes::Bytes>> {
let mut req = client.get(url);
if let Some(et) = &cache.etag { req = req.header("If-None-Match", et); }
if let Some(lm) = &cache.last_mod { req = req.header("If-Modified-Since", lm); }
let resp = req.send().await?;
match resp.status() {
StatusCode::NOT_MODIFIED => Ok(None),
StatusCode::OK => {
cache.etag = resp.headers().get("etag").map(|v| v.to_str().ok().unwrap_or("").to_string());
cache.last_mod = resp.headers().get("last-modified").map(|v| v.to_str().ok().unwrap_or("").to_string());
let bytes = resp.bytes().await?;
Ok(Some(bytes))
}
s => anyhow::bail!("HTTP {} en {}", s, url),
}
}
//! trafico.rs
//!
//! Parser de sensores de trafico (XML `pm.xml` de informo.madrid.es
//!
//! - Convierte cada sensor en un `SensorTr` con intensidad ocupacion
//!   carga, nivel, velocidad media y timestamp
//! - Los datos agregados por celda sirven para calcular
//!   el componente de tráfico en el `delay_factor`
//!
//! Este modulo conecta directamente con la red de sensores urbano

use crate::models::types::SensorTr;
use quick_xml::events::Event;
use quick_xml::Reader;
use std::time::{SystemTime, UNIX_EPOCH};

fn fix_num(s: &str) -> Option<f32> {
    if s.trim().is_empty() { return None; }
    let mut v = s.trim().to_string();
    if v.contains(',') && v.contains('.') { v = v.replace('.', "").replace(',', "."); }
    else { v = v.replace(',', "."); }
    v.parse::<f32>().ok()
}

pub fn parse_trafico_xml(xml: &[u8]) -> Vec<SensorTr> {
    let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_millis() as i64;
    let mut out = Vec::new();

    let mut r = Reader::from_reader(xml);
    r.trim_text(true);
    let mut buf = Vec::new();

    let mut in_pm = false;
    let mut tag: Option<String> = None;
    let mut cur = Tmp::default();

    loop {
        match r.read_event_into(&mut buf) {
            Ok(Event::Start(e)) => {
                let name = e.name().as_ref().to_ascii_lowercase();
                if name == b"pm".as_slice() { in_pm = true; cur = Tmp::default(); }
                tag = Some(String::from_utf8_lossy(&name).into_owned());
            }
            Ok(Event::End(e)) => {
                let name = e.name().as_ref().to_ascii_lowercase();
                if name == b"pm".as_slice() && in_pm {
                    if let (Some(id), Some(x), Some(y)) = (cur.idelem, cur.st_x, cur.st_y) {
                        if cur.error.as_deref() == Some("N") {
                            let (lat, lon) = crate::tools::utm::utm30_to_wgs84(x as f64, y as f64);
                            out.push(SensorTr {
                                id: id as u32,
                                lat: lat as f32,
                                lon: lon as f32,
                                intensidad: cur.intensidad,
                                ocupacion: cur.ocupacion,
                                carga: cur.carga,
                                nivel: cur.nivel,
                                vel: cur.velocidad,
                                ts_ms: now,
                            });
                        }
                    }
                    in_pm = false;
                }
                tag = None;
            }
            Ok(Event::Text(t)) => {
                if !in_pm { continue; }
                if let Some(name) = &tag {
                    let txt = t.unescape().unwrap_or_default().to_string();
                    match name.as_str() {
                        "idelem" => cur.idelem = txt.parse::<i64>().ok(),
                        "st_x" => cur.st_x = fix_num(&txt),
                        "st_y" => cur.st_y = fix_num(&txt),
                        "intensidad" => cur.intensidad = fix_num(&txt),
                        "ocupacion" => cur.ocupacion = fix_num(&txt),
                        "carga" => cur.carga = fix_num(&txt),
                        "niveleservicio" => cur.nivel = fix_num(&txt),
                        "velocidad" => cur.velocidad = fix_num(&txt),
                        "error" => cur.error = Some(txt),
                        _ => {}
                    }
                }
            }
            Ok(Event::Eof) => break,
            Err(_) => break,
            _ => {}
        }
        buf.clear();
    }

    use std::collections::HashMap;
    let mut map: HashMap<u32, SensorTr> = HashMap::new();
    for s in out { map.insert(s.id, s); }
    map.into_values().collect()
}

#[derive(Default)]
struct Tmp {
    idelem: Option<i64>,
    st_x: Option<f32>, st_y: Option<f32>,
    intensidad: Option<f32>, ocupacion: Option<f32>, carga: Option<f32>, nivel: Option<f32>, velocidad: Option<f32>,
    error: Option<String>,
}
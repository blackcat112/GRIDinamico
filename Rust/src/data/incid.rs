//! incid.rs
//!
//! Parser de incidencias de trafico (XML de informo.madrid.es)
//!
//! - Convierte cada incidencia en un struct `Incidencia` con:
//!   lat/lon, estado, tipo, descripción, inicio y fin
//! - Se usa para marcar hexágonos bloqueados o penalizados
//!
//! Las incidencias influyen directamente en el `delay_factor`



use crate::models::types::Incidencia;
use quick_xml::events::Event;
use quick_xml::Reader;

pub fn parse_incidencias_xml(xml: &[u8]) -> Vec<Incidencia> {
    let mut out = Vec::new();
    let mut r = Reader::from_reader(xml);
    r.trim_text(true);

    let mut buf = Vec::new();
    let mut in_item = false;
    let mut cur = IncTmp::default();

    loop {
        match r.read_event_into(&mut buf) {
            Ok(Event::Start(e)) => {
                let name = e.name().as_ref().to_ascii_lowercase();
                if name == b"incidencia".as_slice() { in_item = true; cur = IncTmp::default(); }
                cur.tag = Some(String::from_utf8_lossy(&name).into_owned());
            }
            Ok(Event::End(e)) => {
                let name = e.name().as_ref().to_ascii_lowercase();
                if name == b"incidencia".as_slice() && in_item {
                    if let (Some(lat), Some(lon)) = (cur.lat, cur.lon) {
                        out.push(Incidencia {
                            lat: lat as f32,
                            lon: lon as f32,
                            estado: cur.estado.take().unwrap_or_default(),
                            inicio: cur.inicio.take(),
                            fin: cur.fin.take(),
                            tipo: cur.tipo.take().unwrap_or_default(),
                            descripcion: cur.descripcion.take().unwrap_or_default(),
                        });
                    }
                    in_item = false;
                }
                cur.tag = None;
            }
            Ok(Event::Text(t)) => {
                if !in_item { continue; }
                if let Some(tag) = &cur.tag {
                    let txt = t.unescape().unwrap_or_default().to_string();
                    match tag.as_str() {
                        "latitud" => cur.lat = txt.parse::<f64>().ok(),
                        "longitud" => cur.lon = txt.parse::<f64>().ok(),
                        "incid_estado" => cur.estado = Some(txt),
                        "fh_inicio" => cur.inicio = Some(txt),
                        "fh_final" => cur.fin = Some(txt),
                        "nom_tipo_incidencia" => cur.tipo = Some(txt),
                        "descripcion" => cur.descripcion = Some(txt),
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

    out
}

#[derive(Default)]
struct IncTmp { tag: Option<String>, lat: Option<f64>, lon: Option<f64>, estado: Option<String>, inicio: Option<String>, fin: Option<String>, tipo: Option<String>, descripcion: Option<String> }
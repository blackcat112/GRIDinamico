// // src/s2grid.rs
// //! Mallado S2 para Zaragoza: centro con celdas pequeñas (level 10 ≈ ~3 km aprox.)
// //! y anillo con celdas grandes (level 9 ≈ ~6 km, o level 8 ≈ ~12 km).
// //!
// //! Endpoint sugerido: /map/hex?city=zgz&grid=s2  (ver api.rs)
// //! Front: ENDPOINTS.zaragoza.map = "/map/hex?city=zgz&grid=s2"

// use geojson::GeoJson;
// use serde_json::json;

// use s2::cell::Cell;
// use s2::cellid::CellID;
// use s2::cap::Cap;
// use s2::latlng::LatLng;
// use s2::point::Point;
// use s2::region::RegionCoverer;

// // --- util ---

// #[inline]
// fn km_to_angle_rad(km: f64) -> f64 {
//     km / 6371.0_f64
// }

// #[inline]
// fn cap_from_center_km(lat_deg: f64, lon_deg: f64, radius_km: f64) -> Cap {
//     let center_ll = LatLng::from_degrees(lat_deg, lon_deg);
//     let axis: Point = Point::from(center_ll);
//     let ang = km_to_angle_rad(radius_km);
//     // h = 1 - cos(theta)
//     let h = 1.0 - ang.cos();
//     // En s2 0.0.12: from_center_height(&Point, f64)
//     Cap::from_center_height(&axis, h)
// }

// #[inline]
// fn cell_polygon(cell_id: CellID) -> Vec<[f64; 2]> {
//     // Celda S2 -> 4 vértices
//     let cell = Cell::from(cell_id);
//     let mut coords = Vec::with_capacity(5);
//     for i in 0..4 {
//         let v = cell.vertex(i);
//         let ll = LatLng::from(v);
//         // OJO: ll.lng/ll.lat son Angle (campos); usa .deg()
//         coords.push([ll.lng.deg(), ll.lat.deg()]);
//     }
//     // cerrar polígono
//     coords.push(coords[0]);
//     coords
// }

// fn cells_to_geojson(
//     cells: impl IntoIterator<Item = CellID>,
//     style: serde_json::Value,
//     zona: &str,
// ) -> Vec<serde_json::Value> {
//     let mut feats = Vec::new();
//     for cid in cells {
//         let exterior = cell_polygon(cid);
//         feats.push(json!({
//             "type": "Feature",
//             "geometry": { "type":"Polygon", "coordinates":[exterior] },
//             "properties": {
//                 "s2": cid.0,        // CellID es tuple struct; el u64 va en .0
//                 "zona": zona,
//                 "style": style
//             }
//         }));
//     }
//     feats
// }

// /// Cubre un Cap S2 con celdas a un level fijo (min=max=level).
// fn cover_cap_fixed_level(cap: &Cap, level: i32, max_cells_hint: usize) -> Vec<CellID> {
//     // En s2 0.0.12 no hay ::default(); se construye literal
//     let coverer = RegionCoverer {
//         min_level: level as u8,
//         max_level: level as u8,
//         level_mod: 1,          // usar todos los niveles (branching completo)
//         max_cells: max_cells_hint,
//     };
//     // covering() devuelve CellUnion (tuple struct) -> .0 es Vec<CellID>
//     coverer.covering(cap).0
// }

// /// True si el centro de la celda cae dentro del Cap.
// fn cell_center_in_cap(cid: CellID, cap: &Cap) -> bool {
//     let cell = Cell::from(cid);
//     let center = cell.center(); // S2Point
//     cap.contains_point(&center)
// }

// /// Genera mallado S2 de Zaragoza:
// /// - Centro: level_center (≈3 km si 10)
// /// - Anillo: level_ring   (≈6 km si 9; ≈12 km si 8)
// /// - Radios en km para círculo interior/exterior
// pub fn geojson_zaragoza_s2(
//     level_center: i32,
//     level_ring: i32,
//     inner_radius_km: f64,
//     outer_radius_km: f64,
// ) -> String {
//     // Plaza del Pilar aprox. (centro)
//     let lat_c = 41.65606_f64;
//     let lon_c = -0.87734_f64;

//     // Caps (círculos geodésicos sobre la esfera)
//     let cap_inner = cap_from_center_km(lat_c, lon_c, inner_radius_km);
//     let cap_outer = cap_from_center_km(lat_c, lon_c, outer_radius_km);

//     // Centro: cubrimos el cap interior a level_center
//     let center_cells = cover_cap_fixed_level(&cap_inner, level_center, 2048);

//     // Anillo: cubrimos el cap exterior a level_ring y filtramos celdas cuyo
//     // centro caiga fuera del cap interior (sin solape). Si quieres solape, quita el filtro.
//     let ring_raw = cover_cap_fixed_level(&cap_outer, level_ring, 4096);
//     let ring_cells: Vec<CellID> = ring_raw
//         .into_iter()
//         .filter(|cid| !cell_center_in_cap(*cid, &cap_inner))
//         .collect();

//     // Estilos (coinciden con tu paleta del front)
//     let inner_style = json!({
//         "fill": true, "fill-color": "#06b6d4", "fill-opacity": 0.55,
//         "stroke": "#22d3ee", "stroke-width": 1.2, "stroke-opacity": 0.9
//     });
//     let outer_style = json!({
//         "fill": true, "fill-color": "#8b5cf6", "fill-opacity": 0.45,
//         "stroke": "#a78bfa", "stroke-width": 1.0, "stroke-opacity": 0.85
//     });

//     // Debug opcional
//     println!(
//         "[ZGZ·S2] center(level={}) cells={}, ring(level={}) cells={}",
//         level_center, center_cells.len(), level_ring, ring_cells.len()
//     );

//     // GeoJSON
//     let mut features = Vec::new();
//     features.extend(cells_to_geojson(ring_cells, outer_style, "periphery_s2"));
//     features.extend(cells_to_geojson(center_cells, inner_style, "center_s2"));

//     let gj = json!({
//         "type":"FeatureCollection",
//         "name":"zgz_s2_mesh",
//         "crs": { "type":"name","properties":{"name":"EPSG:4326"} },
//         "features": features
//     });
//     GeoJson::from_json_value(gj).unwrap().to_string()
// }

// /// Valores por defecto razonables:
// /// - level_center = 10 (~3 km)
// /// - level_ring   = 9  (~6 km)   // pon 8 si prefieres ~12 km
// /// - inner_radius_km = 9
// /// - outer_radius_km = 32
// pub fn geojson_zaragoza_s2_default() -> String {
//     geojson_zaragoza_s2(10, 9, 9.0, 32.0)
// }

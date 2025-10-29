use chrono::NaiveDate;
use h3o::{LatLng, Resolution};
use std::fs;

fn h(lat: f64, lon: f64) -> String {
    let c = LatLng::new(lat, lon).unwrap().to_cell(Resolution::try_from(7u8).unwrap());
    c.to_string()
}

fn main() -> anyhow::Result<()> {
    // Puntos de Logroño (centro urbano)
    let ayto      = h(42.4627, -2.44498); // Ayuntamiento
    let estacion  = h(42.4590, -2.4455);  // Estación
    let granvia   = h(42.4667, -2.4496);  // Gran Vía
    let hospital  = h(42.4539, -2.4549);  // San Pedro
    let pol_ind   = h(42.4320, -2.4485);  // Polígono
    let cc_berceo = h(42.4467, -2.4584);  // CC Berceo
    let puente    = h(42.4700, -2.4360);  // Puente de Piedra

    let date = NaiveDate::from_ymd_opt(2025,10,28).unwrap();

    // Registros inventados y coherentes
    // Formato: date,origin_h3,dest_h3,n_trucks,n_cars,conf
    let mut rows = vec![
        (ayto.clone(), granvia.clone(),  40.0,  900.0, 0.90),
        (granvia.clone(), ayto.clone(),  35.0, 1100.0, 0.92),
        (estacion.clone(), ayto.clone(), 20.0,  800.0, 0.82),
        (ayto.clone(), estacion.clone(), 25.0,  750.0, 0.80),

        (pol_ind.clone(), ayto.clone(), 130.0, 300.0, 0.55), // baja conf → debe activar TomTom
        (ayto.clone(), pol_ind.clone(), 120.0, 260.0, 0.58), // baja conf → debe activar TomTom

        (cc_berceo.clone(), granvia.clone(), 10.0,  500.0, 0.88),
        (granvia.clone(), cc_berceo.clone(),  8.0,  520.0, 0.87),

        (puente.clone(), ayto.clone(), 12.0, 600.0, 0.84),
        (ayto.clone(), puente.clone(), 15.0, 580.0, 0.83),

        (hospital.clone(), granvia.clone(), 30.0, 650.0, 0.86),
        (granvia.clone(), hospital.clone(), 28.0, 640.0, 0.86),
    ];

    // Además, algunos intra-hex (origen=destino) para simular retenciones locales:
    rows.push((granvia.clone(), granvia.clone(), 5.0,  400.0, 0.90));
    rows.push((ayto.clone(),    ayto.clone(),    3.0,  350.0, 0.92));

    // Escribe CSV en data/od_today.csv
    fs::create_dir_all("data").ok();
    let mut w = csv::Writer::from_path("data/od_today.csv")?;
    w.write_record(&["date","origin_h3","dest_h3","n_trucks","n_cars","conf"])?;
    for (o,d,nt,nc,conf) in rows {
        w.write_record(&[
            date.to_string(),
            o, d,
            format!("{:.0}", nt),
            format!("{:.0}", nc),
            format!("{:.2}", conf),
        ])?;
    }
    w.flush()?;
    println!("OK -> data/od_today.csv");
    Ok(())
}

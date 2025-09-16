# Madrid Grid Dinámico (Rust)

Servicio **ultra-ligero** en Rust que:

- descarga datos públicos del Ayuntamiento de Madrid (zonas de carga/descarga, incidencias y tráfico),
- transforma UTM 30N → WGS84,
- agrega por celdas hexagonales,
- calcula un **delay factor** por hexágono,
- expone una API (`/kpis`, `/map/hex`) y una UI en Leaflet para visualizarlo.

> Sustituye a un flujo equivalente en Node-RED con mucho menor consumo de CPU/RAM.

---

## 🧭 Estructura del proyecto

```bash 
madgrid/
├─ Cargo.toml
├─ data/
│   └─ hex_grid_madrid_300m.geojson    
├─ src/
│   ├─ main.rs        # arranque, tareas de fetch, API
│   ├─ api.rs         # rutas /health, /kpis, /map/hex, /export/hex-df.json
│   ├─ grid.rs        # índice espacial + cálculo delay_factor + GeoJSON
│   ├─ utm.rs         # conversión UTM 30N -> WGS84
│   ├─ fetch.rs       # HTTP con ETag/If-Modified-Since
│   ├─ carga.rs       # parser CSV zonas carga/descarga
│   ├─ incid.rs       # parser XML incidencias
│   └─ trafico.rs     # parser XML tráfico (pm.xml)
└─ web/
    └─ index.html     # UI Leaflet con HUD de detalle

```

---

## ⚙️ Requisitos

- **Rust 1.70+** (edición 2021). Instalar: [https://rustup.rs](https://rustup.rs)
- Acceso a los endpoints públicos de datos de Madrid.
- Un fichero **GeoJSON** con el grid hexagonal (ej. 300 m) en `data/hex_grid_madrid_300m.geojson`.

---

## 🚀 Quick Start

```bash
# 1) Clona el repo y entra
git clone https://github.com/tu-org/madgrid.git
cd madgrid

# 2) Compila
cargo build --release


# 3) Arranca en desarrollo
cargo run
# abre http://localhost:8080

from h3 import h3

cell_id = "89390ca36cbffff"
centroid = h3.h3_to_geo(cell_id)         # (lat, lon)
boundary = h3.h3_to_geo_boundary(cell_id) # [(lat1, lon1), ...] polygon
res = h3.h3_get_resolution(cell_id)      # resolución (int)
parent = h3.h3_to_parent(cell_id, res-1) # padre (id)
is_pentagon = h3.h3_is_pentagon(cell_id) # bool

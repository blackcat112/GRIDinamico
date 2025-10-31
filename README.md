# HERRAMIENTA QUE GENERA MALLADOS DINAMICOS  EN BASE A DATOS

## Checklist de tareas

- [ ] Mallado S2 para clusterizacion 
   - [x] Creacion mallado S2
   - [x] Api recibe pedidos y tipo de vehiculo y devuelve geojson S2 inteligente
   - [ ] Testing 
- [ ] Mallado H3
   - [x] Creacion mallado H3
   - [x] H3 se nutre de csv de Telco (Orange)
   - [x] H3 se nutre de FCD llamada api externa en caso de que confianza Telco baja (C<0.65)
   - [x] Llamada de FCD con sentido usando OSM para calcular el punto mas optimo de llamda a a la API TomTom 
   - [ ] HotSpots
      - [ ] Funcionamiento de dividir en hijas correcto [Si es HotSpot se activa]
      - [ ] Formula de HotSpot
   - [ ] Hacer mas preciso Delay Factor 
   - [ ] Testear para reducir death zones
   - [ ] ???????? 
- [ ] Escribir documentación final

---

## 🧭 Estructura del proyecto





---

## ⚙️ Requisitos

- **Rust 1.70+** (edición 2021). Instalar: [https://rustup.rs](https://rustup.rs)

---

## API de malla S2 , S2 nos evita overlapping y mejor precision en puntos (pedidios) sin death zones 

Esta API expone rutas HTTP para monitorización, consulta de KPIs, exportación de mallas H3, ruteo y gestión de grupos.  
Implementación en Rust con [Axum](https://github.com/tokio-rs/axum)

### 🚦 Endpoints utiles ahora mismo (1)

Mandar pedidos segun formato de Alberto es decir [[lon, lat], [lon, lat], ...]
```bash
curl -X POST http://localhost:1616/orders/filter \
  -H "Content-Type: application/json" \
  -d {
     "points":[
    [-0.87734, 41.65606],
    [-0.87750, 41.65580],
    [-1.00000, 41.70000],
    [0.20000, 42.20000],
    ],
     "veh": "bike"
     }'

```
Recibes de respuesta los hexagonos de Aragon con una resolucion de 9 y el numero de pedidos dentro de el. 
```json
{
  "crs": {
    "properties": {
      "name": "EPSG:4326"
    },
    "type": "name"
  },
  "features": [
    {
      "geometry": {
        "coordinates": [
          [
            [
              -0.9247500742020962,
              41.63738555051186
            ],
            [
              -0.9056622899625406,
              41.63753696250685
            ],
            [
              -0.9056622899625406,
              41.657480806805964
            ],
            [
              -0.9247500742020964,
              41.65732938241821
            ],
            [
              -0.9247500742020962,
              41.63738555051186
            ]
          ]
        ],
        "type": "Polygon"
      },
      "properties": {
        "level": 12,
        "pedidos": 13,
        "s2_cell": "0d596b3",
        "vehicle_type": "bike"
      },
      "type": "Feature"
    },

```

### 🚦 Endpoints disponibels 

### 1. Health check
Verifica que el servicio está activo

```bash
curl http://localhost:1616/health
```

---
# 🧭 Módulo `h3grid.rs`

### 📦 Parte del proyecto **RustMalladoH3**
Versión: `v2.0 – O/D + TomTom + Históricos (Orion-LD / JSONL)`  
Autor: *Capillar IT || Nicolas BEcas *

---

## 🧠 Descripción general

El módulo `h3grid.rs` implementa la **malla dinámica de tráfico urbano** basada en celdas hexagonales H3.  
Integra múltiples fuentes de datos —telco (Orange), tráfico en tiempo real (TomTom) y red vial (OpenStreetMap)— para generar un mapa de **delays normalizados y confiables** por zona.

El resultado se exporta como `GeoJSON`, listo para visualización y análisis, y también se persiste en `Orion-LD` o `JSONL` para históricos.

---

## ⚙️ Funcionalidades principales

### 🔹 1. Agregación O/D
- Lee registros `ODRecord` (Origen–Destino) con volúmenes y confianza.
- Asigna cada punto a celdas H3 (`CellIndex`).
- Combina datos de origen y destino ponderando por volumen y tipo de vehículo.
- Calcula confianza media (`conf_cell`) y volumen normalizado (`vol_norm`).

### 🔹 2. Modelo BPR-like (delay teórico)
- Aplica una versión suavizada del modelo **BPR (Bureau of Public Roads)**:
 $$
  \mathrm{delay} = 1 + a \cdot \left(\frac{v}{c}\right)^{b} \cdot \left(1 + \gamma \cdot \mathrm{truck\_share}\right)
 $$
- Donde:
  - `a, b`: controlan la intensidad de congestión.
  - `c`: capacidad estimada por percentil de tráfico (`capacity_percentile`).
  - `γ`: sensibilidad a camiones.
- Resulta en `delay_orange` (delay teórico base).

### 🔹 3. Integración con TomTom (API Flow Segment Data)
- Llama a la API: https://api.tomtom.com/traffic/services/4/flowSegmentData/absolute/10/json

- Usa coordenadas **viales reales** en lugar del centro geométrico H3.
- Obtiene `currentSpeed`, `freeFlowSpeed` y `confidence`.
- Calcula `delay_tomtom = freeFlowSpeed / currentSpeed`.

### 🔹 4. Mapa vial (road_map)
- Cargado desde CSV con `load_roadmap_csv(path)`.
- Contiene, por cada celda H3:
- `road_count`: nº de segmentos viales.
- `total_len_m`: longitud total de vías.
- `avg_lat`, `avg_lon`: punto vial representativo.
- `primary_ratio`: proporción de vías principales.
- Mejora la precisión al seleccionar el punto vial más relevante por celda.

### 🔹 5. Blending inteligente (Orange + TomTom)
- Se aplica solo a celdas con baja confianza (`conf_cell < min_conf_for_pure_orange`).
- Combina ambos delays según:
- La confianza del dato Orange.
- La confianza del dato TomTom (`confidence`).
- Resultado final: `delay_final`.

### 🔹 6. Export a GeoJSON
- Crea un `FeatureCollection` con cada celda H3 como polígono.
- Incluye propiedades:
- `delay_final`, `delay_orange`, `delay_tomtom`
- `truck_share`, `vol_norm`, `conf`
- `used_tomtom` (booleano)
- Colores normalizados (`color_from_norm`) para visualización inmediata en Leaflet o Kepler.gl.

### 🔹 7. Persistencia histórica
- **JsonlSink:** guarda cada fila en formato JSONL (historial local).
- **OrionLdSink:** inserta o actualiza entidades NGSI-LD (`H3Delay`) en FIWARE Orion-LD.

### 🔹 8. Concurrencia optimizada
- Llamadas a TomTom en paralelo mediante `tokio::Semaphore` con `max_concurrent_calls`.
- Gestión robusta de errores y `timeout` por solicitud (8 s).

---

## 🧩 Flujo de datos completo

```
flowchart TD
  A[OD CSV / Parquet] --> B[aggregate_od_to_h3()]
  B --> C[compute_delay_orange()]
  C --> D{conf < threshold?}
  D -- Sí --> E[TomTomClient::delay_for_cell()]
  D -- No --> F[Delay Orange puro]
  E --> G[enrich_with_traffic_provider()]
  F --> G
  G --> H[to_geojson()]
  G --> I[JsonlSink / OrionLdSink]
  H --> J[GeoJSON visualizable]
  I --> K[Históricos]

```
---

# 📈 Cálculo de *Delay Factor* (TTI) en la malla H3

Este módulo estima, por hexágono H3, un **delay factor** (≈ *Travel Time Index, TTI*) que refleja cuánto se alarga el viaje respecto al *free-flow*. Trabaja con dos fuentes:

- **Telco O/D** (base “Orange”): volumen relativo y mezcla de vehículos (turismos/camiones).
- **Proveedor de tráfico** (p. ej., **TomTom**): velocidades observadas vs. *free-flow* y un `confidence`.

La salida principal por celda es `delay_final`, junto con métricas de apoyo (volumen normalizado, cuota de camiones, etc.).

---

## 🧠 Conceptos clave

- **Travel Time Index (TTI)**

$$
\mathrm{TTI}=\frac{t_{\mathrm{obs}}}{t_{\mathrm{free}}}\;\equiv\;\frac{V_{\mathrm{free}}}{V_{\mathrm{obs}}}
$$

Es el índice operativo estándar: compara el tiempo (o velocidad) observado con el de flujo libre.

- **Funciones volumen–retardo (BPR)** para planificación

$$
\text{delay}=1+a\,(v/c)^b
$$

donde \(v\) es el volumen y \(c\) la capacidad. Capturan la **no linealidad** de la congestión cerca de saturación.

- **Fiabilidad (opcional)**  
Con series intradía pueden derivarse *Buffer Index* y *Planning Time Index* a partir de percentiles del tiempo de viaje.

---

## 🔢 Fórmulas que usamos

### 1) Delay del proveedor (cuando hay velocidades)

A partir de *Traffic Flow* del proveedor:

$$
\boxed{\mathrm{delay}_{\mathrm{tt}}=\frac{V_{\mathrm{free}}}{V_{\mathrm{obs}}}}
$$

- `currentSpeed` y `freeFlowSpeed` → cálculo directo de TTI.
- Se acompaña de `confidence` por segmento/celda.

### 2) Delay “Orange” (fallback robusto cuando **no** hay proveedor)

Usamos una variante **BPR-like** basada **solo** en O/D:

**Capacidad aproximada por ciudad/día**

$$
c=\mathrm{Perc}_{P}\big(\mathrm{trips\_total}\big)\quad P\in[0.85,\,0.95]
$$

(y un suelo mínimo configurable). Motivo: robusto a *outliers*, independiente de cartografía detallada y aproxima la “saturación típica”.

**Fórmula por celda**

$$
\boxed{\mathrm{delay}_{\mathrm{orange}}=1+a\cdot(v/c)^b\cdot\bigl(1+\gamma\cdot\mathrm{truck\_share}\bigr)}
$$
- v = `trips_total` (pondera camiones vía `truck_factor`).  
- truck_share = `trips_trucks / trips_total`.  
- Parámetros por defecto típicos: a = 0.15, b = 4, γ ∈ [0.2, 0.6].  
- Se **clampa** a `[delay_min, delay_max]`.


> **Por qué no lineal:** cerca de capacidad, pequeñas subidas de volumen generan grandes retardos; la BPR lo captura, una forma lineal no.

### 3) Blending (si hay proveedor **y** confianza válida)

Si la celda tiene confianza telco baja y hay dato del proveedor, combinamos:

$$
\boxed{\mathrm{delay}_{\mathrm{final}}=(1-\lambda)\cdot\mathrm{delay}_{\mathrm{orange}}+\lambda\cdot\mathrm{delay}_{\mathrm{tt}}}
$$

- \(\lambda\) crece cuando **baja la confianza telco** y/o **sube** la `confidence` del proveedor.  
- **Objetivo:** dar más peso a la fuente más fiable en cada celda.

> Si no hay proveedor o no aplica el blending, entonces `delay_final = delay_orange`.

---
## ⚙️ Parámetros (resumen práctico)

- `bpr_a` (≈ 0.15) y `bpr_b` (≈ 4.0): intensidad/curvatura de congestión (estándar BPR/HCM).  
- `capacity_percentile` (0.85–0.95): percentil para estimar \(c\).  
- `capacity_floor`: suelo mínimo para \(c\).  
- `truck_gamma` (0.2–0.6): sensibilidad a camiones (eleva retardo en celdas con alto tráfico pesado).  
- `vc_cap`: tope para \(v/c\) por estabilidad numérica.  
- `delay_min`, `delay_max`: acotan el rango del delay.

**Calibración recomendada:** en días con buena cobertura del proveedor, ajusta \((a,b,\gamma)\) minimizando el error entre `delay\_orange` y `delay\_tt` **solo** en celdas con `confidence` alta. Así el fallback Orange queda alineado con la “verdad terreno” cuando falte proveedor.

---

## 🧩 Señales exportadas por celda

- `delay_orange`, `delay_tomtom`, `delay_final`  
- `vol_norm`, `truck_share`, `conf` (telco)  
- `used_tomtom` y/o `used_external` (booleanos) para auditar si entró una fuente externa.


## 📦 Pipeline (pseudocódigo)

```text
1) Aggregate O/D to H3:
   trips_total, trips_trucks, trips_cars, conf (ponderado)

2) Orange (BPR-like):
   c = percentile(trips_total, P=0.90) with floor
   truck_share = trips_trucks / trips_total
   vc = clamp(trips_total / c, 0, vc_cap)
   delay_orange = clamp(1 + a * vc^b * (1 + gamma * truck_share), delay_min, delay_max)

3) Provider (si conf_telco < umbral):
   delay_tt = freeFlowSpeed / currentSpeed
   λ = f(conf_telco, confidence_provider)

4) Blending:
   delay_final = (1-λ)*delay_orange + λ*delay_tt
   used_tomtom = (delay_tt disponible)

5) Export:
   GeoJSON / Orion-LD con métricas y flags

```
## 📚 Referencias

### Índices de fiabilidad (FHWA)
- FHWA — *Travel Time Reliability: Making It There On Time, All The Time* (definiciones de **Planning Time Index**, **Buffer Index**).  
  https://ops.fhwa.dot.gov/publications/tt_reliability/ttr_report.htm
- FHWA — *Travel Time Reliability Reference Guide* (resumen de métricas de fiabilidad, definiciones operativas).  
  https://ops.fhwa.dot.gov/publications/fhwahop21015/fhwahop21015.pdf
- FHWA — *Travel Time Reliability Brochure* (explicación didáctica del **Buffer Index**).  
  https://ops.fhwa.dot.gov/publications/tt_reliability/brochure/ttr_brochure.pdf

### Funciones volumen–retardo (BPR/HCM)
- Bureau of Public Roads (1964) — *Traffic Assignment Manual for Application with a Large, High Speed Computer* (origen clásico de \(1 + a(v/c)^b\)).  
  https://libraryarchives.metro.net/dpgtl/us-department-of-commerce/1964-traffic-assignment-manual-for-application-with-a-large-high-speed-computer.pdf
- (Contexto histórico) BPR Manual (vista en Google Books).  
  https://books.google.com/books/about/Traffic_Assignment_Manual_for_Applicatio.html?id=AvNUR_O_JEcC
- (Lectura moderna) *Modified Bureau of Public Roads (MBPR) Link Function* — discusión y extensiones a la BPR.  
  https://mediatum.ub.tum.de/doc/1714671/document.pdf


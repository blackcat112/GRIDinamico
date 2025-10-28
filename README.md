# HERRAMIENTA QUE GENERA MALLADOS DINAMICOS  EN BASE A DATOS


Motor **ultra-ligero** en Rust que:

- descarga datos públicos del Ayuntamiento de Madrid (zonas de carga/descarga, incidencias y tráfico),
- transforma UTM 30N → WGS84,
- agrega por celdas hexagonales,
- calcula un **delay factor** por hexágono,
- expone una APIs
- clusteriza en base a json de pedidios 

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
  -d '[
    [-0.87734, 41.65606],
    [-0.87750, 41.65580],
    [-1.00000, 41.70000],
    [0.20000, 42.20000]
  ]'

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
              -1.0009306649596441,
              41.700461268873916
            ],
            [
              -1.0027256903714825,
              41.699315066183765
            ],
            [
              -1.0023277304433202,
              41.697479676708845
            ],
            [
              -1.000134861209602,
              41.696790513021696
            ],
            [
              -0.9983399027562924,
              41.697936691208504
            ],
            [
              -0.998737746578201,
              41.699772057584774
            ],
            [
              -1.0009306649596441,
              41.700461268873916
            ]
          ]
        ],
        "type": "Polygon"
      },
      "properties": {
        "h3": "893970880b3ffff",
        "pedidos": 1
      },
      "type": "Feature"
    },
    {
      "geometry": {
        "coordinates": [
          [
            [
              0.1994052736451236,
              42.20348919133427
            ],
            [
              0.19759938953989833,
              42.20237456951714
            ],
            [
              0.19796430083377586,
              42.200547801264
            ],
            [
              0.20013497963605628,
              42.19983568029518
            ],
            [
              0.20194079239704452,
              42.20095027969117
            ],
            [
              0.20157599769968082,
              42.20277702247614
            ],
            [
              0.1994052736451236,
              42.20348919133427
            ]
          ]
        ],
        "type": "Polygon"
      },
      "properties": {
        "h3": "8939752836bffff",
        "pedidos": 1
      },
      "type": "Feature"
    },
    {
      "geometry": {
        "coordinates": [
          [
            [
              -0.8774764286353776,
              41.6581520972018
            ],
            [
              -0.8792702332495692,
              41.65700799417719
            ],
            [
              -0.8788761820845147,
              41.65517313865687
            ],
            [
              -0.8766884421646315,
              41.654482409513065
            ],
            [
              -0.8748947047865993,
              41.655626488188524
            ],
            [
              -0.8752886400923735,
              41.65746132035594
            ],
            [
              -0.8774764286353776,
              41.6581520972018
            ]
          ]
        ],
        "type": "Polygon"
      },
      "properties": {
        "h3": "8939708d667ffff",
        "pedidos": 2
      },
      "type": "Feature"
    }
  ],
  "name": "aragon_orders_r9",
  "type": "FeatureCollection"
}
```




### 🚦 Endpoints disponibels 

### 1. Health check
Verifica que el servicio está activo

```bash
curl http://localhost:1616/health
```

### 2. KPIs
Devuelve indicadores básicos de una ciudad
Parámetros
city (opcional, string):
zgz → Zaragoza
lg → Logroño
madC → Madrid Combinado
(otro valor → KPIs del estado en memoria)

```bash
curl "http://localhost:1616/kpis?city=zgz"   
```

### 3. H3 mallado 
Exporta una malla de hexágonos en formato GeoJSON.
Parámetros
city (opcional, string):
zgz → Zaragoza
lg → Logroño
madC → Madrid Combinado
(sin parámetro → Madrid por defecto)
```bash
curl "http://localhost:1616/map/hex?city=madC" 
```

### 4. Zonas/grupos
POST 
```bash
curl -X POST "http://localhost:1616/groups?city=zgz" \
  -H "Content-Type: application/json" \
  -d '{
    "features": [
      { "properties": { "h3": "8928308280fffff", "grupo": 1 } },
      { "properties": { "h3": "8928308280bffff", "grupo": 2 } }
    ]
  }'
 
```

GET 
```bash
curl "http://localhost:1616/groups?city=zgz"
```




---

## 📊 Testing

### Explicación del script `bench.sh`

Este script automatiza las pruebas de rendimiento para la aplicación `madgrid`, centrándose en el tamaño del binario, el uso de recursos y el desempeño de los endpoints HTTP.

### Explicación del script `bench_summary.sh`

Este script resume los resultados de las pruebas HTTP realizadas con la herramienta `hey` para la aplicación `madgrid`. Recoge estadísticas de rendimiento y latencia (solicitudes/segundo, p50, p95, p99) de múltiples archivos de salida y genera un resumen consolidado en formato CSV.

### Explicación del script `plot_bench.py`

Este script visualiza los resultados de las pruebas HTTP a partir de un archivo CSV de resumen. Lee los datos resumidos, extrae la resolución y los ajustes de refinamiento desde los nombres de archivo, y grafica las solicitudes por segundo (`Requests/sec`) según los niveles de resolución H3. El gráfico resultante compara el rendimiento con y sin refinamiento, guardando la visualización.

![Comparación de benchmarks](testing/bench_comparison.png)

### Observaciones sobre el gráfico

> **Interpretación del gráfico:**
>
> - Cuando usamos hexágonos grandes (`res7`), **es más rápido no refinar** porque el mapa ya está simplificado.
> - Pero en resoluciones más finas (`res9–res10`), **refinar multiplica el rendimiento** porque evitamos calcular todas las celdas, sólo las que importan.
>
> **En otras palabras:**  
> El refinamiento es una estrategia para escalar a mayor detalle sin perder rendimiento.  
> Si queremos mapas urbanos muy precisos, **debemos usarlo**.


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
\boxed{\text{delay\_tt}=\frac{V_{\mathrm{free}}}{V_{\mathrm{obs}}}}
$$

- `currentSpeed` y `freeFlowSpeed` → cálculo directo de TTI.
- Se acompaña de `confidence` por segmento/celda.

### 2) Delay “Orange” (fallback robusto cuando **no** hay proveedor)

Usamos una variante **BPR-like** basada **solo** en O/D:

**Capacidad aproximada por ciudad/día**

$$
c=\mathrm{Perc}_P\big(\texttt{trips\_total}\big)\quad\text{con }P\in[0.85,\,0.95]
$$

(y un suelo mínimo configurable). Motivo: robusto a *outliers*, independiente de cartografía detallada y aproxima la “saturación típica”.

**Fórmula por celda**

$$
\boxed{\text{delay\_orange}=1+a\cdot(v/c)^b\cdot\bigl(1+\gamma\cdot\text{truck\_share}\bigr)}
$$

- \(v=\texttt{trips\_total}\) (pondera camiones vía `truck_factor`).  
- \(\text{truck\_share}=\texttt{trips\_trucks}/\texttt{trips\_total}\).  
- Parámetros por defecto típicos: \(a=0.15,\; b=4,\; \gamma\in[0.2,0.6]\).  
- Se **clampa** a `[delay_min, delay_max]`.

> **Por qué no lineal:** cerca de capacidad, pequeñas subidas de volumen generan grandes retardos; la BPR lo captura, una forma lineal no.

### 3) Blending (si hay proveedor **y** confianza válida)

Si la celda tiene confianza telco baja y hay dato del proveedor, combinamos:

$$
\boxed{\text{delay\_final}=(1-\lambda)\cdot\text{delay\_orange}+\lambda\cdot\text{delay\_tt}}
$$

- \(\lambda\) crece cuando **baja la confianza telco** y/o **sube** la `confidence` del proveedor.  
- **Objetivo:** dar más peso a la fuente más fiable en cada celda.

> Si no hay proveedor o no aplica el blending, entonces `delay_final = delay_orange`.

---

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
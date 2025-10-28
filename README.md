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
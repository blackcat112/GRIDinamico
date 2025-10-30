#!/usr/bin/env python3
import json
import random
import time
import requests

# -----------------------------
# CONFIGURACIÓN DEL TEST
# -----------------------------
URL = "http://localhost:1616/orders/aragon"
VEH = "bike"
N_POINTS = 2000

# Centro aproximado de Zaragoza
lon_c, lat_c = -0.878, 41.658

# -----------------------------
# GENERACIÓN DE PUNTOS
# -----------------------------
points = []
for i in range(N_POINTS):
    # Tres anillos de densidad para probar todos los niveles
    if i < 1200:  # núcleo urbano muy denso (radio ~1km)
        lon = lon_c + random.uniform(-0.010, 0.010)
        lat = lat_c + random.uniform(-0.010, 0.010)
    elif i < 1700:  # anillo medio (~3km)
        lon = lon_c + random.uniform(-0.030, 0.030)
        lat = lat_c + random.uniform(-0.030, 0.030)
    else:  # periferia amplia (~7km)
        lon = lon_c + random.uniform(-0.070, 0.070)
        lat = lat_c + random.uniform(-0.070, 0.070)
    points.append([lon, lat])

payload = {"points": points, "veh": VEH}

# -----------------------------
# ENVÍO AL ENDPOINT
# -----------------------------
print(f"🚀 Enviando {N_POINTS} pedidos al endpoint {URL}...\n")

start = time.time()
response = requests.post(URL, json=payload)
elapsed = time.time() - start

# -----------------------------
# RESULTADOS
# -----------------------------
if response.status_code == 200:
    data = response.json()
    print(f"✅ Respuesta OK ({response.status_code}) en {elapsed:.3f}s")
    print(f"→ Nº de celdas generadas: {len(data.get('features', []))}")
    print(f"→ CRS: {data.get('crs', {}).get('properties', {}).get('name', 'unknown')}")
    print("\nGuardando salida en 's2_orders_2000.geojson' ...")
    
    with open("s2_orders_2000.geojson", "w", encoding="utf-8") as f:
        json.dump(data, f, ensure_ascii=False, indent=2)
    
    print("📂 Archivo guardado correctamente. Puedes visualizarlo en https://geojson.io")
else:
    print(f"❌ Error HTTP {response.status_code}: {response.text}")

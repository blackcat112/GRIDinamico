#!/usr/bin/env bash
set -euo pipefail

# ▼ Pega tu API key aquí (o export API_KEY=... antes de ejecutar)
API_KEY="${API_KEY:-iHC6Mqg1RZQ7LNpJFm23dV4QKNRi28wl}"

# ▼ Coordenadas de Logroño (centro). Puedes sobreescribir con: LAT=... LON=... ./script.sh
LAT="${LAT:-42.46272}"
LON="${LON:--2.44499}"

[[ -n "$API_KEY" && "$API_KEY" != "TU_API_KEY_AQUI" ]] || { echo "Set API_KEY (edita el script o exporta API_KEY)"; exit 1; }

BASE="https://api.tomtom.com/traffic/services/4/flowSegmentData/absolute/10/json"

echo "---------- Traffic Flow Segment Data (TomTom) -----------"
echo "Coordenadas: ${LAT},${LON} (Logroño)"
RESP="$(mktemp)"; trap 'rm -f "$RESP"' EXIT

HTTP=$(curl -sS --get "$BASE" \
  --data-urlencode "point=${LAT},${LON}" \
  --data "key=${API_KEY}&unit=kmph&openLr=false" \
  -H "Accept: application/json" \
  --compressed \
  -w "%{http_code}" -o "$RESP") || HTTP="000"

if [[ "$HTTP" == "200" ]]; then
  echo "---- JSON crudo ----"
  jq . "$RESP"

  echo
  echo "---- Resumen ----"
  jq -r '
    if .flowSegmentData then
      .flowSegmentData as $f |
      " Velocidad actual: \($f.currentSpeed // "N/A") km/h\n" +
      " Velocidad libre:  \($f.freeFlowSpeed // "N/A") km/h\n" +
      " Tiempo actual:    \($f.currentTravelTime // "N/A") s\n" +
      " Tiempo libre:     \($f.freeFlowTravelTime // "N/A") s\n" +
      " TTI:              " +
        (if ($f.currentTravelTime and $f.freeFlowTravelTime and ($f.freeFlowTravelTime != 0))
         then ((($f.currentTravelTime) / ($f.freeFlowTravelTime))|tostring)
         else "N/A" end) + "\n" +
      " Confianza:        \($f.confidence // "N/A")\n" +
      " Cierre de vía:    \($f.roadClosure // false)"
    else
      "Respuesta inesperada (no existe flowSegmentData)"
    end
  ' "$RESP"
else
  echo "Error HTTP $HTTP"
  if jq . "$RESP" >/dev/null 2>&1; then
    jq . "$RESP"
  else
    cat "$RESP"
  fi
  exit 1
fi

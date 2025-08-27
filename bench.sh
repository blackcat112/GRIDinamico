#!/usr/bin/env bash
set -euo pipefail

# =========================
# Config por defecto
# =========================
URL="${URL:-http://127.0.0.1:8080}"
DURATION="${DURATION:-30s}"   
CONCURRENCY="${CONCURRENCY:-50}"
RESOLUTIONS=(7 9 10)          # resoluciones H3 a probar
REFINE_THR="1.15"

OUTDIR="bench_out_$(date +%Y%m%d_%H%M%S)"
mkdir -p "$OUTDIR"

# =========================
# Helpers macOS
# =========================
bin_size() {
  stat -f "%N %z bytes" "$1"
}

pid_cpu_mem_line() {
  local pid="$1"
  local now; now="$(date '+%Y-%m-%d %H:%M:%S')"
  ps -o pid= -o pcpu= -o pmem= -o rss= -o etime= -o command= -p "$pid" | \
    awk -v t="$now" '{print t, $1, $2, $3, $4, $5, substr($0, index($0,$6))}'
}

vm_summary() {
  echo "--- vm_stat (pages) ---"
  vm_stat | sed 's/^[ ]*//'
}

# =========================
# 1) Binario size
# =========================
echo "===> Binario size"
for BIN in madgrid; do
  if [ -f "target/release/$BIN" ]; then
    bin_size "target/release/$BIN" | tee -a "$OUTDIR/sizes.txt"
  fi
done

# =========================
# 2) Resolviendo PIDs
# =========================
echo "===> Resolviendo PIDs"
pgrep -a madgrid | tee "$OUTDIR/pids.txt" || true

# =========================
# 3) Idle sampling (10s)
# =========================
echo "===> Idle sampling (10s)"
PIDS="$(pgrep madgrid || true)"
if [ -n "$PIDS" ]; then
  for PID in $PIDS; do
    echo "--- PID $PID" | tee -a "$OUTDIR/idle.txt"
    for i in $(seq 1 10); do
      pid_cpu_mem_line "$PID" | tee -a "$OUTDIR/idle.txt"
      sleep 1
    done
    echo "--- memoria del sistema" | tee -a "$OUTDIR/idle.txt"
    vm_summary | tee -a "$OUTDIR/idle.txt"
  done
fi

# =========================
# 4) Warm-up
# =========================
echo "===> Warm-up"
curl -s "${URL}/health" || true
sleep 2

# =========================
# 5) Bench con distintos parámetros
# =========================
if ! command -v hey >/dev/null 2>&1; then
  echo "hey no está instalado. En macOS: brew install hey"
  exit 1
fi

for RES in "${RESOLUTIONS[@]}"; do
  echo "===> Bench res=$RES sin refine"
  hey -z "$DURATION" -c "$CONCURRENCY" "${URL}/routing/cells?res=$RES" \
    | tee "$OUTDIR/hey_res${RES}.txt"

  echo "===> Bench res=$RES con refine (thr=$REFINE_THR)"
  hey -z "$DURATION" -c "$CONCURRENCY" "${URL}/routing/cells?res=$RES&refine_res=$((RES-2))&refine_thr=$REFINE_THR" \
    | tee "$OUTDIR/hey_res${RES}_refine.txt"
done

# =========================
# 6) Curl timings
# =========================
echo "===> Curl timings"
{
  for RES in "${RESOLUTIONS[@]}"; do
    echo "# res=$RES"
    curl -w "ttfb=%{time_starttransfer} total=%{time_total}\n" -s -o /dev/null \
      "${URL}/routing/cells?res=$RES"
  done
} | tee "$OUTDIR/curl_timings.txt"

echo "===> Fin. Mira carpeta $OUTDIR"

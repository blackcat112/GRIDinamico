#!/usr/bin/env bash
set -euo pipefail

OUTDIR="${1:-bench_out_*}"
SUMMARY="$OUTDIR/summary.txt"

echo "===> Generando resumen para $OUTDIR"
echo "File,Requests/sec,p50,p95,p99" > "$SUMMARY"

for f in "$OUTDIR"/hey_*.txt; do
  if [ -f "$f" ]; then
    reqs=$(grep "Requests/sec:" "$f" | awk '{print $2}')
    p50=$(grep "  50%" "$f" | awk '{print $2}')
    p95=$(grep "  95%" "$f" | awk '{print $2}')
    p99=$(grep "  99%" "$f" | awk '{print $2}')
    echo "$(basename "$f"),$reqs,$p50,$p95,$p99" >> "$SUMMARY"
  fi
done

echo "===> Resumen generado en $SUMMARY"
column -t -s, "$SUMMARY"

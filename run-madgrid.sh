#!/bin/bash
set -euo pipefail

APP_DIR="/data/MallaInteligente/motorRust"
PID_FILE="$APP_DIR/madgrid.pid"
LOG_FILE="$APP_DIR/madgrid.log"

cd "$APP_DIR"

# ¿ya está corriendo?
if [ -f "$PID_FILE" ] && ps -p "$(cat "$PID_FILE")" > /dev/null 2>&1; then
  echo "madgrid ya está en marcha (PID $(cat "$PID_FILE"))."
  exit 0
fi

echo "Arrancando madgrid (cargo run --release)..."
# Ejecuta una sola vez y se queda vivo. nohup para desligar de la sesión.
nohup bash -lc 'exec cargo run --release' >> "$LOG_FILE" 2>&1 &
echo $! > "$PID_FILE" 
echo "OK, PID $(cat "$PID_FILE"). Logs: $LOG_FILE"

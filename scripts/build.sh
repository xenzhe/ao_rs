#!/bin/sh
# Compila el servidor en release y arma la carpeta de despliegue (por defecto ./dist).
# Uso: scripts/build.sh [carpeta]
# No pisa config/ ni data/ si ya existen en el destino (conserva tu configuracion y tu base).
set -eu
OUT="${1:-dist}"
cd "$(dirname "$0")/.."

cargo build --release -p server

mkdir -p "$OUT/config" "$OUT/data"
cp target/release/server target/release/ao-ops "$OUT/"
for d in config data; do
    for f in "$d"/*.toml "$d"/*.ini; do
        [ -f "$f" ] || continue
        [ -e "$OUT/$f" ] || cp "$f" "$OUT/$f"
    done
done
cp scripts/*.sh scripts/ao-server.service "$OUT/" 2>/dev/null || true
echo "Listo: $OUT (server, ao-ops, config/, data/). Arranca con: cd $OUT && ./server"

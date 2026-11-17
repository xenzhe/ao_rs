#!/bin/sh
# Copia de seguridad de la base AHORA (segura con el servidor en marcha) y rotacion.
# Uso: scripts/backup.sh [config/server.toml]   (ejecutar desde la carpeta del servidor)
set -eu
HERE="$(cd "$(dirname "$0")" && pwd)"
for t in "$HERE/ao-ops" "$HERE/../dist/ao-ops" "$HERE/../target/release/ao-ops"; do
    [ -x "$t" ] && exec "$t" backup "${1:-config/server.toml}"
done
echo "no encuentro ao-ops: compila con scripts/build.sh" >&2
exit 1

#!/bin/sh
# Restaura la base desde una copia. PARA EL SERVIDOR ANTES.
# Uso: scripts/restore.sh [latest|ruta/ao-AAAAMMDD-HHMMSS.db] [config/server.toml]
# Sin argumentos lista las copias. La base actual queda al lado como ao.db.antes-<fecha>.
set -eu
HERE="$(cd "$(dirname "$0")" && pwd)"
CFG="${2:-config/server.toml}"
TOOL=""
for t in "$HERE/ao-ops" "$HERE/../dist/ao-ops" "$HERE/../target/release/ao-ops"; do
    [ -x "$t" ] && TOOL="$t" && break
done
[ -n "$TOOL" ] || { echo "no encuentro ao-ops: compila con scripts/build.sh" >&2; exit 1; }
if [ $# -eq 0 ]; then
    "$TOOL" backups "$CFG"
    echo
    echo "Restaurar: scripts/restore.sh latest   (o la ruta de una copia)"
    exit 0
fi
exec "$TOOL" restore "$1" "$CFG"

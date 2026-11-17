#!/bin/sh
# Instala el servidor como servicio systemd (como root). Uso: sudo scripts/install-service.sh [/opt/ao]
# Copia la carpeta de despliegue (la que genera build.sh) a /opt/ao, crea el usuario `ao`
# y activa ao-server.service. Una copia diaria de la base va por cron (ver docs/OPERACION.md).
set -eu
DEST="${1:-/opt/ao}"
SRC="${SRC:-dist}"
[ -x "$SRC/server" ] || { echo "no hay $SRC/server: ejecuta scripts/build.sh primero" >&2; exit 1; }
id ao >/dev/null 2>&1 || useradd --system --home "$DEST" --shell /usr/sbin/nologin ao
mkdir -p "$DEST"
# rsync conservaria config y datos; con cp no pisamos lo existente de config/ ni data/.
cp "$SRC/server" "$SRC/ao-ops" "$DEST/"
for d in config data; do
    mkdir -p "$DEST/$d"
    for f in "$SRC/$d"/*; do
        [ -e "$DEST/$d/$(basename "$f")" ] || cp "$f" "$DEST/$d/"
    done
done
chown -R ao:ao "$DEST"
sed "s#/opt/ao#$DEST#g" scripts/ao-server.service > /etc/systemd/system/ao-server.service
systemctl daemon-reload
systemctl enable --now ao-server
echo "Servicio activo: systemctl status ao-server   (logs: journalctl -u ao-server -f)"

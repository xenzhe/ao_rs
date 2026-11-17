# Operación del servidor

Estadísticas, copias de seguridad, apagado ordenado y despliegue. Código en
`crates/server/src/ops/`, scripts en `scripts/`, claves en la sección `[ops]` de
`config/server.toml`.

## 1. Compilar

```
# Windows (PowerShell)                    # Linux
.\scripts\build.ps1                       scripts/build.sh
```

Compila `cargo build --release -p server` y arma `dist/` con `server`, `ao-ops` (herramienta de
copias), `config/*.toml`, `data/*.toml` y los scripts. Si `dist/config` o `dist/data` ya existen no
se pisan: una actualización conserva tu configuración y tu base.

El servidor **no incluye** los recursos de AO20 (licencia CC BY-NC-ND): hay que tener la carpeta
`Recursos` en el servidor.

## 2. Configurar

| Qué | Dónde |
|---|---|
| Dirección y puerto (por defecto `127.0.0.1:7666`; para jugadores externos `0.0.0.0:7666`) | `bind` en `config/server.toml`, o variable `AO_BIND` |
| Base de datos SQLite (`data/ao.db`) | `database`, o `AO_DB` |
| Carpeta `Recursos` de AO20 | `recursos`, o `AO_RECURSOS` (vacío: junto al proyecto) |
| Datos de juego (hechizos, criaturas, balance) | `data_dir` (`data/*.toml`) |
| Log | `RUST_LOG=info` (por defecto) |
| Entrada sin cuenta | `dev_login = false` en un servidor real |

Las rutas relativas cuentan desde la carpeta donde se ejecuta `server`: arráncalo siempre desde
`dist/` (o fija `WorkingDirectory` en el servicio).

Puertos: **TCP 7666** (o el de `bind`). Abrirlo en el cortafuegos; no hay más puertos.

Sección `[ops]`:

| Clave | Por defecto | Efecto |
|---|---|---|
| `stats_interval_min` | 5 | minutos entre informes en el log (0 = ninguno) |
| `stats_file` | `data/stats.json` | JSON para una web (`""` = no escribirlo) |
| `stats_file_secs` | 10 | cada cuántos segundos se refresca el JSON y la ventana de medidas |
| `backup_dir` | `data/backups` | carpeta de copias |
| `backup_interval_hours` | 6 | copia periódica (0 = solo al apagar / a mano) |
| `backup_keep_days` | 7 | días que se conservan (ver rotación) |
| `backup_on_shutdown` | true | copia al apagar |

## 3. Estadísticas

Línea en el log cada `stats_interval_min`:

```
estadisticas: jugadores 3 (pico 7), cuentas 10, personajes 12, mapas 2, npcs 55, tick medio 0.25 ms, peor 1.50 ms (total 4.00 ms), 12.5 msg/s, memoria 48 MB, activo 1d 01h 01m
```

`peor` es el tick más lento desde el último informe y `total` desde que arrancó. `msg/s` son mensajes
de clientes por segundo en la última ventana. Memoria = working set en Windows, RSS en Linux.

`data/stats.json` (escritura atómica, se puede leer con la web mientras corre):
`unix, uptime_secs, players, players_peak, accounts, characters, maps_loaded, npcs, tick_avg_ms,
tick_max_ms, tick_max_total_ms, ticks_total, msgs_per_sec, msgs_total, memory_mb`.

Bajo demanda: `ao-ops stats` imprime ese JSON; en Linux `systemctl kill -s USR1 ao-server` fuerza
una línea de informe; desde código, `Ops::stats(&world)` / `Ops::report_now(&world)`.

## 4. Copias de seguridad

Copia en caliente con `VACUUM INTO` sobre una segunda conexión: instantánea consistente aunque el
servidor esté escribiendo (la base va en WAL), archivo `.db` completo y verificado con
`PRAGMA integrity_check`. Se escribe con nombre temporal y se renombra: nunca queda una copia a medias.

- Nombre: `ao-AAAAMMDD-HHMMSS.db` (UTC; `_NN` si caen dos en el mismo segundo).
- Antes de cada copia periódica se guardan todos los personajes conectados.
- **Rotación**: se conserva la copia más reciente de cada uno de los últimos `backup_keep_days` días
  con copias, más todas las del día más reciente. Nunca se borra nada que no sea `ao-*.db`.
- Manual: `scripts\backup.ps1` / `scripts/backup.sh` (`ao-ops backup`), seguro con el servidor encendido.
- Listar: `ao-ops backups`.

### Restaurar

1. **Parar el servidor** (la herramienta se niega si la base está en uso).
2. `scripts\restore.ps1` (sin argumentos lista) y luego `scripts\restore.ps1 -Backup latest`
   (o la ruta de una copia). En Linux: `scripts/restore.sh latest`.
3. La base actual queda al lado como `ao.db.antes-<fecha>` por si te equivocas. Se verifica la copia
   antes de tocar nada; una copia corrupta se rechaza sin cambiar la base.
4. Arrancar el servidor.

Copia fuera de la máquina: sincroniza `backup_dir` con otro disco/servidor (rsync, robocopy, rclone);
una copia en el mismo disco no protege de un fallo del disco.

## 5. Apagado ordenado

Con `server::ops::run` (ver §8), Ctrl+C, SIGTERM (Linux) o cierre de consola/apagado (Windows):
avisa a los jugadores (`Kicked`), guarda a todos, escribe `stats.json`, hace la copia final y sale.
Un `kill -9` / `taskkill /F` / cierre brusco **no** guarda: hoy los personajes solo se guardan al
salir y antes de cada copia periódica (no hay autoguardado), así que una caída pierde como mucho el
progreso desde el último guardado.

## 6. Ejecutar como servicio

### Windows (NSSM)

`nssm.exe` en el PATH (`winget install nssm`), PowerShell como Administrador:

```
cd C:\ao\dist
.\install-service.ps1 -Dir C:\ao\dist -Recursos D:\AO20\Recursos
nssm start AOServer
nssm stop AOServer        # Ctrl+C -> guarda, copia, sale (espera hasta 60 s)
```

Log en `server.log` (rota a 10 MB). Se reinicia solo si cae. No uses el Programador de tareas con
"finalizar": mata el proceso sin guardar.

### Linux (systemd)

```
scripts/build.sh && sudo scripts/install-service.sh /opt/ao
systemctl status ao-server ; journalctl -u ao-server -f
```

La unidad (`scripts/ao-server.service`) usa `KillSignal=SIGTERM` y `TimeoutStopSec=60`. Una copia
diaria extra fuera de la carpeta del servidor, por cron:
`15 4 * * * cd /opt/ao && ./ao-ops backup && rsync -a data/backups/ backup-host:/ao/`.

## 7. Actualizar sin perder datos

1. `scripts\backup.ps1` (o confía en la copia de apagado).
2. Parar el servicio (guarda y copia).
3. `scripts\build.ps1` (no pisa `config/` ni `data/` del destino) y copiar el nuevo `server`/`ao-ops`
   si desplegaste en otra carpeta.
4. Si hay claves nuevas en `config/server.toml` o `data/*.toml`, añádelas (las que falten usan su
   valor por defecto).
5. Arrancar: las **migraciones** de la base se aplican solas al abrir (`PRAGMA user_version`) y nunca
   se editan, solo se añaden; no se puede volver a un binario más viejo sobre una base ya migrada, para
   eso está la copia del paso 1.

## 8. Enganche (lo hace el integrador, `main.rs` y `world/mod.rs` no son de esta parte)

`crates/server/src/main.rs`, dos cambios:

```rust
// al principio de main, tras el tracing_subscriber (subcomandos opcionales)
let args: Vec<String> = std::env::args().skip(1).collect();
if let Some(code) = server::ops::cli(&args) { std::process::exit(code); }
...
// al final, en vez de server::run(...)
server::ops::run(listener, Settings::new(config, data, recursos)).await;
```

`ops::run` es `world::run` con el mismo bucle y tick más: medidas de tick y mensajes, informes,
copia periódica y parada con señal. Si el integrador prefiere conservar `world::run`, las piezas
sueltas son `Ops::new(&cfg)`, `ops.on_frame()` por `Event::Frame`, `ops.on_tick(duracion)` alrededor
de `World::tick`, `ops.housekeeping(&world)` tras cada tick y `ops.shutdown(&world)` al parar.

`ao-ops` (binario propio, `ops/bin.rs`) hace `backup | backups | restore | stats` sin tocar `main.rs`.

Un comando de GM `/stats` (del agente C) puede llamar a `Ops::stats(&world).line()`; `Ops` vive en
el bucle de `ops::run`, así que habría que guardarlo en `World` (campo nuevo) para exponerlo.

## 9. Pruebas

`crates/server/tests/p4_ops.rs`: copia/restauración con datos (y rechazo con la base en uso o con una
copia corrupta), subcomandos, y un servidor real con reloj real que muestra al jugador en `stats.json`
y, al pararlo, avisa al cliente y deja la copia final con la cuenta y el personaje.

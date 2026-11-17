# Argentum Online en Rust (proyecto personal)

AO modernizado: mismo juego y misma esencia (mapas, gráficos, ritmo y reglas de AO20),
con resoluciones actuales, efectos de luz y partículas y un servidor seguro.

Los gráficos y mapas son los de AO20 (`ao-org/Recursos`, licencia CC BY-NC-ND 4.0):
**solo uso personal, sin fines comerciales**. Se leen de una copia local de `Recursos`
(por defecto `../ao20/Recursos`; se cambia en `config/*.toml` o con `AO_RECURSOS`).

## Cómo arrancarlo

```bash
cargo build --release
./target/release/server          # lee config/server.toml y data/*.toml
./target/release/client          # lee config/client.toml
cargo test --release             # todos los tests (incluye ataques al servidor)
```

## Organización

```
crates/
  ao-formats/   Lectura de archivos de AO20: mapas .csm, graficos.ini, cuerpos, cabezas.
                Solo datos: sin red, sin dibujo, sin reglas. Reutilizable por cualquier herramienta.
  ao-rules/     Reglas puras compartidas por cliente y servidor: quién pisa qué casilla,
                línea de visión, experiencia y niveles, definiciones de hechizos/criaturas.
  ao-protocol/  Mensajes cliente/servidor y su codificación (con límites de tamaño).
  server/       Servidor autoritativo.
    net.rs        conexiones, límites de paquetes y flood (no toca el estado del juego)
    world/        el "actor" dueño del mundo, instancias de mapa, entidades
    systems/      un archivo por sistema: sesión, movimiento, magia, combate, IA, regen, chat
    security.rs   piezas anti-trampa (ritmo de paso, anti-replay, strikes, rate limit)
    config.rs     config/server.toml
    data.rs       data/*.toml (con valores por defecto incluidos en el binario)
  client/       Cliente.
    game/         estado, mensajes del servidor y acciones del jugador
    render/       mundo (orden oficial de AO20), luces, partículas, distribución de pantalla
    ui/           widgets, HUD, login
    assets.rs     texturas de Recursos/Graficos
    input.rs, net.rs, config.rs, guard.rs (stats ofuscados en RAM)
    devtools.rs   automatización para pruebas y capturas (variables AO_*)
config/         Configuración técnica (puertos, rutas, seguridad, vídeo).
data/           Datos de juego editables sin recompilar: hechizos, criaturas, equilibrio.
```

### Reglas de la arquitectura

- **El servidor manda.** El cliente solo envía intenciones (caminar, lanzar un hechizo a
  una casilla, hablar); todo se valida en el servidor.
- **Las reglas viven una sola vez** en `ao-rules`. El cliente las usa para predecir y no
  mandar nada que el servidor fuera a castigar; el servidor las aplica.
- **Un solo dueño del estado** (el actor `World`): sin mutex ni condiciones de carrera.
  Cada mapa es una `MapInstance` con sus criaturas e índice de jugadores, preparada para
  correr en su propia tarea más adelante.
- **Datos fuera del código**: lo que un diseñador querría tocar va en `data/*.toml`.
- **Formatos de AO en dudas: mirar el código VB6 oficial** (`../ao20/argentum-online-*`)
  antes de suponer.

### Añadir un sistema nuevo (ejemplo: inventario)

1. Mensajes en `crates/ao-protocol/src/messages.rs` (y subir `PROTOCOL_VERSION`).
2. Reglas puras y sus tests en `crates/ao-rules/src/inventory.rs`.
3. Datos en `data/items.toml` y su carga en `crates/server/src/data.rs`.
4. Sistema del servidor en `crates/server/src/systems/inventory.rs` (+ una línea en el
   `match` de `systems/mod.rs`).
5. Estado en `crates/client/src/game/`, ventana en `crates/client/src/ui/inventory.rs`.
6. Test de integración en `crates/server/tests/` (incluido qué pasa si un cliente hace trampa).

## Pruebas automáticas

- `ao-formats`: los 842 mapas oficiales se leen byte a byte; tablas de gráficos.
- `ao-rules`: reglas de paso de AO20, niveles.
- `server`: piezas de seguridad, datos válidos, y tests de integración que atacan un
  servidor real (speedhack, replay, paquetes gigantes o basura, hechizos ilegales, flood),
  cruzan una salida de mapa y comprueban la población de criaturas.

Para capturas y pruebas del cliente: `AO_SHOT=archivo.png`, `AO_SHOT_AT`, `AO_WALK`,
`AO_FXDEMO`, `AO_HOUR`, `AO_NAME` (ver `crates/client/src/devtools.rs`). En el servidor,
`AO_START=mapa,x,y` cambia el punto de aparición y `RUST_LOG=debug` da más registros.

## Pendiente (siguientes pasos)

- Persistencia de personajes (base de datos; hoy se pierden al salir).
- Un actor (tarea) por mapa para escalar a cientos de jugadores.
- NPCs y objetos originales del mapa, partículas del mapa, armas/cascos/escudos, sonido.

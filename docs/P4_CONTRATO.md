# P4 — Contrato entre agentes (contenido y operación)

P4 = oficios, artesanía, GM y penas, eventos, y la operación del servidor. Cinco agentes a la
vez, cada uno en su rama y **solo en sus archivos**. El esqueleto está en `main` (mensajes vacíos
por sistema, piezas de `Player` y del `World`, migraciones reservadas, ventanas vacías y ganchos
ya conectados). Igual que en P2/P3: lee `README.md`, `docs/P2_CONTRATO.md` (reglas de la casa:
persistencia, mensajes, tests de trampa) y `docs/P3_CONTRATO.md` (base de UI, cómo probar con
capturas, entorno, puertos).

Fuente de verdad: VB6 oficial en `X:\dev\ao20\argentum-online-server-classic\Codigo`
(cp1252: `iconv -f cp1252 -t utf-8`) y datos en `X:\dev\ao20\Recursos\Dat` / `init`. Citar
archivo y función.

## Dueños

| Agente | Sistema | Archivos propios |
|---|---|---|
| Opus A | Trabajo: minería, tala, pesca, fundición (`Trabajo.bas`, `modFishing`, `modSmelting`) | `ao-protocol/src/p4/work.rs`, `server/src/systems/work.rs`, `server/src/world/work.rs`, `server/src/db/work.rs`, `migrations/0007_work.sql`, `client-core/src/p4/work.rs`; `ao-rules/src/work.rs` (nuevo) |
| Opus B | Artesanía: herrería, carpintería, sastrería, alquimia (`modCrafteos`, `clsCrafteo`, recetas de Dat) | `.../p4/craft.rs` (protocolo y cliente), `systems/craft.rs`, `world/craft.rs`, `db/craft.rs`, `migrations/0008_craft.sql`, `client/src/ui/craft.rs`; `ao-rules/src/craft.rs` (nuevo); lectores nuevos en `ao-formats/src/dat/` si hacen falta |
| Opus C | GM: comandos, roles, penas (cárcel, silencio, ban), registros (`Protocol_GmCommands`, `Admin`, `Penas`, `Logging`) | `.../p4/gm.rs`, `systems/gm.rs`, `world/gm.rs`, `db/gm.rs`, `migrations/0009_gm.sql`, `client/src/ui/gm.rs` |
| Opus D | Eventos: retos, torneos, subastas, invasiones, castillos (`ModRetos`, `ModTorneos`, `ModSubasta`, `ModInvasion`, `ModCastle`) | `.../p4/event.rs`, `systems/event.rs`, `world/event.rs`, `db/event.rs`, `migrations/0010_events.sql`, `client/src/ui/events.rs`, `client/src/ui/auction.rs` |
| Sonnet 1 | Operación: estadísticas, copias de seguridad, despliegue | `crates/server/src/ops/` (nuevo módulo; una línea `mod ops;` en `lib.rs` permitida), `docs/OPERACION.md`, `scripts/` (nuevo), `config/server.toml` (solo añadir claves suyas) |

`.../p4/x.rs` = `crates/ao-protocol/src/p4/x.rs` y `crates/client-core/src/p4/x.rs`. Una línea
`pub mod <tuyo>;` en `ao-rules/src/lib.rs` o `ao-formats/src/dat/mod.rs` está permitida.

## Ganchos ya conectados (el dueño los implementa)

| Gancho | Dueño | Ya lo llama |
|---|---|---|
| `on_<x>(id, msg)`, `tick_<x>(now)`, `<x>_on_enter(id)`, `<x>_on_leave(id)` | cada uno | despacho, tick del mundo, entrar/salir |
| `craft_npc(id, npc)` | B | interact: NPC con `Crafts > 0` vivo y cerca |
| `gm_command(id, line) -> bool` | C | chat: toda línea `/...` antes de los comandos de desarrollo |
| `gm_is_silenced(id) -> bool` | C | chat y canales |
| `event_keeps_items_on_death(id) -> bool` | D | `user_die` (no tira el inventario) |
| `event_on_player_killed(killer: Option<u32>, victim)` | D | combate tras cada muerte de jugador |

Ventanas nuevas (`WindowId`): `Crafting` (la abre el sistema B), `Gm` (F10, solo GMs), `Events`
(V), `Auction` (U). El trabajo (A) no tiene ventana: se trabaja con la herramienta equipada sobre
una casilla; diseña el gesto del cliente en `client-core/src/p4/work.rs` y anota cómo enganchar
el clic del mundo (input.rs es del integrador).

Lo que necesites de otro sistema (p. ej. que un GM teletransporte usando `teleport`, que la
artesanía use objetos del inventario, que un torneo use el mapa de arena) úsalo con las funciones
públicas existentes; si falta algo, créalo en tu archivo y anótalo.

## Pruebas y entrega

Tests de bots en `crates/server/tests/p4_<tuyo>.rs` (con trampas), unitarios de reglas, capturas
de tus ventanas en `docs/capturas/p4_<tuyo>_*.png` (2-4). Puertos: A 7711, B 7712, C 7713, D 7714,
Sonnet 7715. `cargo test --workspace` y `cargo clippy --workspace --all-targets` sin avisos nuevos.
Entorno (Bash): `export PATH="$HOME/.cargo/bin:$PATH"`, `export AO_RECURSOS=X:/dev/ao20/Recursos`,
`export CARGO_PROFILE_DEV_DEBUG=0`; sin builds release salvo Sonnet para medir. En Git Bash, las
variables con `/` (p. ej. `AO_SAY=/ir 1 50 12`) necesitan `MSYS_NO_PATHCONV=1`.

Informe final en español: qué portaste (archivo/función VB6), capturas, qué quedó fuera y por
qué, tests, qué necesitas del integrador.

# P3 — Contrato entre agentes (cliente remasterizado)

P3 = el cliente pasa de prototipo a juego: cuentas y creación de personaje rolera, ventanas
para todo lo de P1/P2, pantalla ancha, mundo continuo, efectos, clima y sonido. Seis agentes a
la vez, cada uno en su rama, **solo en sus archivos**. La base ya está en `main`.

Lee antes: `README.md`, `docs/P1_CONTRATO.md` y `docs/P2_CONTRATO.md` (lo que manda el servidor),
y el código de `crates/client` (macroquad) y `crates/client-core` (lógica sin gráficos).

## La base (no la cambies; si necesitas algo, pídelo en el informe)

- `client/src/ui/frame.rs`: `Ui` (escala `u`, foco de teclado, arrastre `Drag`, `mouse_captured` /
  `keyboard_captured`) y el gestor de ventanas (`WindowId`, abrir/cerrar/alternar, mover por el
  título, traer al frente, atajo de teclado). `ui.window(id)` dibuja el marco y devuelve el
  rectángulo del contenido.
- `client/src/ui/theme.rs`: fuentes Cardo (títulos) y Alegreya Sans (texto) de `Recursos/OUTPUT`
  (SIL OFL) y la paleta (madera oscura, pergamino, oro).
- `client/src/ui/widgets.rs`: `text`, `text_with`, `text_fit`, `measure`, `button`, `bar`,
  `slot`, `tooltip`, `text_field` (con foco), `panel`.
- `client/src/ui/mod.rs`: `ui::frame` llama a HUD, consola y ventanas abiertas (de atrás hacia
  delante) **antes** de la entrada del mundo; `input.rs` respeta lo que la UI capturó. Banco,
  comerciante y comercio entre jugadores se abren y cierran solos según lo que manda el servidor.
- `Assets` tiene `objs` (obj.dat) y `draw_obj(obj, rect)` para los iconos de objetos.
- Estado del juego: `game.local` (inventario, oro, vitales, atributos, skills, hechizos, banco,
  suelo) y `game.local.social.{trade,guild,party,faction,quest,channel}`; `game.ents`,
  `game.stats`, `game.spells`, `game.console`, `game.characters`. Para mandar: `game.send(&C2S::..)`.

Interfaz rolera y de calidad: coherente con el tema, legible a cualquier resolución (todo
escala con `u`), tooltips en objetos y botones, sin solapes ni texto cortado, teclas y ratón.

## Dueños

| Agente | Qué | Archivos propios |
|---|---|---|
| Opus C | Pantalla ancha y mundo continuo | `client/src/render/mod.rs`, `render/world.rs`, `render/layout.rs`, `client-core/src/p3/world.rs` |
| Opus D | Efectos, partículas, luces, clima, equipo visible, sonido y música | `render/fx.rs`, `render/lighting.rs`, `render/characters.rs`, `render/weather.rs`, `client/src/audio.rs`, `client/src/game.rs`, `client-core/src/p3/fx.rs` |
| Sonnet 1 | Cuentas, selección y creación de personaje | `ui/login.rs`, `ui/account.rs`, `client-core/src/p3/account.rs` |
| Sonnet 2 | Inventario y equipo (arrastrar y soltar), bóveda, comerciante, comercio | `ui/inventory.rs` (incluido `draw_drag`), `ui/bank.rs`, `ui/shop.rs`, `ui/trade.rs`, `client-core/src/p3/items.rs` |
| Sonnet 3 | Consola con canales, clan, grupo, misiones | `ui/console.rs`, `ui/guild.rs`, `ui/party.rs`, `ui/quests.rs`, `client-core/src/p3/social.rs` |
| Sonnet 4 | HUD (panel lateral, barras, minimapa, barra inferior), hechizos, personaje/skills, mapa del mundo, opciones | `ui/hud.rs`, `ui/spells.rs`, `ui/character.rs`, `ui/worldmap.rs`, `ui/options.rs`, `client-core/src/p3/hud.rs` |

Todo lo demás (servidor, protocolo, `client-core/src/state.rs` y `local.rs`, `ui/frame.rs`,
`ui/theme.rs`, `ui/widgets.rs`, `ui/mod.rs`, `main.rs`, `input.rs`, `assets.rs`, `devtools.rs`)
es del integrador: si lo necesitas, créalo en tu archivo con nombre claro y anótalo en el informe.

## Cómo se prueba (obligatorio)

1. **Lógica** sin ventana: tus acciones en `client-core/src/p3/<tuyo>.rs` con tests de unidad y,
   si hablan con el servidor, con bots de `ao-testkit` (`crates/server/tests/p3_<tuyo>.rs`).
2. **Imagen**: capturas del propio juego (nunca del escritorio). Lanza un servidor en tu puerto
   y el cliente con captura automática:
   - servidor: `AO_BIND=127.0.0.1:<puerto> AO_DB=:memory: target/debug/server.exe` (en segundo plano)
   - cliente: `AO_SERVER=127.0.0.1:<puerto> AO_SHOT=<ruta>.png AO_SHOT_AT=3 AO_OPEN=inventory,bank target/debug/client.exe`
     (otras: `AO_NAME`, `AO_WALK`, `AO_HOUR`, `AO_FXDEMO`; ver `client/src/devtools.rs`).
   - Mira la PNG (herramienta Read) y corrige hasta que se vea bien a 1280x800 y en otra
     resolución (ventana de `config/client.toml` o `AO_CLIENT_CONFIG`).
   - Puertos: Opus C 7701, Opus D 7702, Sonnet 1 7703, Sonnet 2 7704, Sonnet 3 7705, Sonnet 4 7706.
   - Cierra tu servidor al acabar (`taskkill //F //PID <pid>`; no mates por nombre, hay otros).
   - Guarda las capturas finales en `docs/capturas/p3_<tuyo>_*.png` (solo 2-4, pequeñas).
3. `cargo test --workspace` y `cargo clippy --workspace --all-targets` sin avisos nuevos.

Entorno: en Bash `export PATH="$HOME/.cargo/bin:$PATH"`; desde el worktree
`export AO_RECURSOS=X:/dev/ao20/Recursos`; **el disco va justo**: compila con
`export CARGO_PROFILE_DEV_DEBUG=0` y no hagas builds release. El hook bloquea heredocs `<<`,
`sed -i` y `python -`: usa Edit/Write. No edites código con PowerShell `Set-Content` (rompe tildes).

## Al terminar (informe)

Qué hiciste, capturas, qué quedó fuera y por qué, tests, y qué necesitas del integrador.

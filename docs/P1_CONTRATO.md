# P1 — Contrato entre agentes

P1 = núcleo jugable fiel a AO20. Seis agentes trabajan a la vez, cada uno en su rama.
Este documento dice **qué archivo es de quién** y **cómo se llaman los sistemas entre sí**.
Fuente de verdad de reglas: el VB6 oficial en `X:\dev\ao20\argentum-online-server-classic\Codigo`
(archivos en Windows-1252: `iconv -f cp1252 -t utf-8`). Citar archivo y función en los comentarios.

## Ya fijado (no cambiar; si hace falta, pedirlo en el informe)

- Protocolo v5: `crates/ao-protocol/src/messages.rs` (mensajes de combate, magia, inventario,
  banco, interacción, progresión, `Look`, `Status`, `Attack`, `Fx`, `NpcSay`...).
- Estado del cliente: `crates/client-core/src/local.rs` y `state.rs` ya guardan todo lo que
  llega (inventario, oro, vitales, atributos, skills, hechizos, banco, objetos en el suelo,
  equipo visible y estados de cada entidad). Los bots de `ao-testkit` lo leen con `bot.game`.
- Datos oficiales cargados en el servidor: `self.ao.objs / npcs / spells / balance`
  (`crates/server/src/world/ao_data.rs`, tipos de `ao_formats::dat::*`).
- Persistencia (`crates/server/src/db/**`, migraciones): la mantiene el integrador. Se guarda
  todo lo que esté en `Player.stats`, `inv` (slots, equipados, oro), `bank` (slots, oro),
  `spells.slots`, más vida/maná/nivel/exp/posición/cuerpo/cabeza. Vida 0 = muerto.
- `Player` (`world/entities.rs`) tiene una pieza por sistema; **cada pieza es de su dueño** y
  puede añadirle campos libremente (los que no estén en la lista de arriba no se guardan).

## Dueños

| Agente | Sistema | Archivos propios (crear/editar) |
|---|---|---|
| Opus A | Combate | `server/src/systems/melee.rs`, `systems/combat.rs`, `world/combat_state.rs`, `ao-rules/src/combat.rs` (nuevo) |
| Opus B | Hechizos y estados | `server/src/systems/magic.rs`, `world/spellbook.rs`, `ao-rules/src/magic.rs` (nuevo) |
| Opus C | Inventario, equipo, objetos, suelo, drops | `server/src/systems/inventory.rs`, `world/inventory.rs`, `ao-rules/src/items.rs` (nuevo) |
| Opus D | NPCs de los mapas reales, IA, interacción | `server/src/systems/npc_ai.rs`, `systems/interact.rs`, `world/instance.rs`, `Npc` en `world/entities.rs`, `ao-rules/src/pathfinding.rs` (nuevo) |
| Sonnet 1 | Clases, razas, atributos, skills, creación, subida de nivel | `server/src/systems/progression.rs`, `world/stats.rs`, `ao-rules/src/progression.rs`, `ao-rules/src/character.rs` |
| Sonnet 2 | Bóveda | `server/src/systems/bank.rs`, `world/bank.rs` |

Cada agente añade sus tests en `crates/server/tests/p1_<sistema>.rs` con `ao-testkit`
(reloj virtual, bots), **incluido el test de trampa** de su sistema. Una línea en
`crates/ao-rules/src/lib.rs` (`pub mod ...;`) para su módulo nuevo está permitida.
Nada más fuera de su lista: si necesita otra cosa, la anota en el informe final.

## Ganchos entre sistemas (firmas ya creadas, vacías; las implementa el dueño)

| Gancho | Lo implementa | Lo llama |
|---|---|---|
| `drop_npc_loot(map, npc, killer)` | Inventario (C) | Combate (A) al morir una criatura |
| `restore_equipment(id)` | Inventario (C) | Sesión, al entrar al mundo |
| `look_of(id) -> (body, head, weapon, shield, helmet)` | Inventario (C) | `send_look` (sesión) |
| `learn_spell_from_item(id, obj) -> bool` | Magia (B) | Inventario (C) al usar un pergamino |
| `on_level_up(id, levels)` | Progresión (S1) | Combate (A) en `give_exp` |
| `init_new_character(race, class, gender) -> Option<CharacterState>` | Progresión (S1) | Cuentas, al crear un personaje (lo conecta el integrador) |
| `open_bank(id, banker)` | Bóveda (S2) | NPCs (D) al interactuar con un banquero |
| `send_look(id)`, `send_stats(id)` | Sesión (integrador) | Cualquiera tras cambiar equipo/vida/muerte |

Si un sistema necesita llamar a otro y el gancho no existe: crear la llamada a una función
con nombre claro en SU archivo y anotarlo en el informe; el integrador la conecta.

## Reglas comunes

- Fórmulas y valores **de los datos oficiales** (`self.ao.*`), no inventados. Si algo del VB6
  depende de un sistema que no es tuyo, implementa tu parte y deja el resto anotado.
- El servidor manda: valida todo lo que llega del cliente (slots fuera de rango, cantidades
  negativas, objetos que no tiene, distancia a NPCs, muerto intentando actuar...). Trampa
  clara = `self.strike(...)` o `self.kick(...)`; error legítimo = mensaje en consola.
- Tiempo siempre con `self.now()` (reloj del mundo, `GameTime`), nunca `Instant`.
- `cargo test --release` y `cargo clippy --release` limpios en todo el workspace al terminar.
- Commits en tu rama, en español, con la línea `Co-Authored-By` de tu modelo. Sin push ni merge.

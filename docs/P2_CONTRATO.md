# P2 — Contrato entre agentes (social y economía)

P2 = comercio, clanes, grupos, facciones, misiones y canales de chat, fieles a AO20.
Cinco agentes trabajan a la vez, cada uno en su rama. El esqueleto (archivos, mensajes vacíos,
piezas de `Player`, migraciones reservadas, ganchos ya conectados) está en `main`: cada agente
**rellena solo sus archivos**. Fuente de verdad: el VB6 oficial en
`X:\dev\ao20\argentum-online-server-classic\Codigo` (Windows-1252: `iconv -f cp1252 -t utf-8`)
y los datos en `X:\dev\ao20\Recursos\Dat`. Citar archivo y función en los comentarios.

Lee también `docs/P1_CONTRATO.md` (reglas comunes) y `README.md` (estructura).

## Dueños

| Agente | Sistema | Archivos propios |
|---|---|---|
| Opus A | Clanes | `ao-protocol/src/social/guild.rs`, `server/src/systems/guild.rs`, `server/src/world/guild.rs` (`GuildPiece`, `Guilds`), `server/src/db/guilds.rs`, `server/migrations/0003_guilds.sql`, `ao-rules/src/guild.rs`, `client-core/src/social/guild.rs` |
| Opus B | Comercio (NPC y entre jugadores) | `.../social/trade.rs` (protocolo y cliente), `systems/trade.rs`, `world/trade.rs` (`TradePiece`, `Trades`), `db/trade.rs`, `migrations/0006_trade.sql`, `ao-rules/src/trade.rs`; puede **añadir** campos de inventario de comerciante a `ao-formats/src/dat/npcs.rs` |
| Opus C | Grupos y facciones | `.../social/party.rs` y `.../social/faction.rs` (protocolo y cliente), `systems/party.rs`, `systems/faction.rs`, `world/party.rs`, `world/faction.rs`, `db/factions.rs`, `migrations/0004_factions.sql`, `ao-rules/src/party.rs`, `ao-rules/src/faction.rs` |
| Sonnet 1 | Misiones | `.../social/quest.rs` (protocolo y cliente), `systems/quest.rs`, `world/quest.rs`, `db/quests.rs`, `migrations/0005_quests.sql`, `ao-rules/src/quest.rs`, `ao-formats/src/dat/quests.rs` |
| Sonnet 2 | Canales de chat | `.../social/channel.rs` (protocolo y cliente), `on_channel` en `systems/chat.rs` (y lo que necesite en ese archivo) |

`.../social/x.rs` = `crates/ao-protocol/src/social/x.rs` y `crates/client-core/src/social/x.rs`.

Tests: `crates/server/tests/p2_<sistema>.rs` con `ao-testkit` (reloj virtual, bots),
**incluido el test de trampa** (cliente que miente: cantidades negativas o mayores que lo que
tiene, objetos ajenos, aceptar dos veces, cambiar la oferta tras confirmar, actuar muerto o
lejos, flood...). Los ayudantes de bot van en tu archivo de tests (usa `bot.send(&C2S::Guild(..))`
y `bot.game.local.social.<tuyo>`); no edites `ao-testkit`.

## Reglas de la casa

- Persistencia: cada sistema guarda lo suyo con su `impl Db` y su migración (reemplaza el
  `SELECT 1;` reservado de tu archivo; puedes usar tablas con FK a `characters(id)`). Carga en
  `<x>_on_enter(id)` y guarda en `<x>_on_leave(id)` y cuando cambie algo importante. `Player.char_id`
  es `None` en logins de desarrollo: ahí no se guarda nada.
- Estado en memoria: la pieza por jugador (`Player.<x>`) y, si hace falta, el registro global
  (`World.guilds`, `World.parties`, `World.trades`). Puedes añadir campos libremente a tus tipos.
- Mensajes: solo tus enums `XC2S` / `XS2C`. El cliente envía intenciones; el servidor valida todo.
- Texto al jugador en español, como el original. Código y comentarios en inglés.
- `cargo test --workspace` y `cargo clippy --workspace --all-targets` sin avisos nuevos.
  En la shell Bash: `export PATH="$HOME/.cargo/bin:$PATH"` antes de cargo.
- Fuera de tu lista no se toca nada. Si necesitas algo más (un gancho, un campo ajeno, un
  mensaje de otro), créalo con nombre claro en TU archivo y anótalo en el informe final.

## Ganchos (firmas ya creadas y conectadas; el dueño las implementa)

| Gancho | Lo implementa | Ya lo llama |
|---|---|---|
| `open_npc_trade(id, npc)` | Comercio (B) | interact: doble clic en un comerciante (`Comercia=1`) vivo y cerca |
| `share_exp(id, amount, map)` | Grupos (C) | combate al quitar vida a una criatura (hoy: todo a `id`) |
| `party_members_online(id) -> Vec<u32>` | Grupos (C) | canales (S2) |
| `guild_members_online(id) -> Vec<u32>` | Clanes (A) | canales (S2) |
| `faction_npc(id, npc, kind)` | Facciones (C) | interact: reclutador o gobernador |
| `faction_attack_refusal(attacker, victim) -> Option<&str>` | Facciones (C) | melee y magia antes de dañar a un jugador |
| `faction_on_player_killed(killer, victim)` | Facciones (C) | combate tras matar a un jugador |
| `faction_on_npc_killed(killer, npc_number)` | Facciones (C) | combate tras matar a una criatura |
| `quest_npc(id, npc)` | Misiones (S1) | interact: NPC de misiones |
| `quest_on_npc_killed(killer, npc_number, map)` | Misiones (S1) | combate tras matar a una criatura |
| `<x>_on_enter(id)`, `<x>_on_leave(id)` | cada uno | sesión al entrar / antes de salir |

Ya existentes de P1 que podéis usar: `give_exp`, `give_gold`? (mirar `systems/inventory.rs`),
`console`, `send`, `broadcast`, `player`, `player_mut`, `send_stats`, `open_bank`, `close_bank`,
inventario (`inv.slots`, `inv.gold`, `send_inventory_*`), `self.ao.objs / npcs / balance`.

## Al terminar (informe)

Qué portaste (archivo y función VB6 de cada regla), qué quedó fuera y por qué, tests y
resultado, y lo que necesitas del integrador (ganchos, campos ajenos, cliente gráfico).

# Auditoría AO20 – Parte 3: social, eventos, GM y cuentas/personaje

Fecha: 2026-11-18. Solo lectura, por comparación estática (grep/lectura) entre
`X:\dev\ao20\argentum-online-server-classic\Codigo` (VB6) y `X:\dev\ao_rs\crates`.
No se compiló ni se corrieron tests. Se ignoran `tsao*.rs`, salvo `gm_tsao.rs`, que se
cita solo porque ya cubre algunos comandos GM de AO20.

Leyenda: **HECHO** = igual o mejor · **PARCIAL** = falta o difiere algo · **FALTA** = no existe.

## 1. Tabla por sistema

| Módulo VB | Rust | Estado | Notas |
|---|---|---|---|
| `modGuilds.bas` / `clsClan.cls` | `systems/guild.rs`, `ao-rules/src/guild.rs` | HECHO (casi) | Están `CrearNuevoClan`→`guild_found`, `a_NuevoAspirante`→`guild_request_join`, `a_AceptarAspirante`/`a_RechazarAspirante`→`guild_accept`/`guild_reject`, `m_EcharMiembroDeClan`→`guild_kick`, `m_PuedeSalirDeClan`→`can_leave`/`guild_leave`, `ChangeCodexAndDesc`/`ActualizarNoticias`→`guild_update_text`, `SendGuildNews`→`guild_on_enter`, relaciones/guerra (`guild_propose`, `guild_answer_proposal`, `guild_declare_war`), `CheckClanExp`/`NivelDeClan`/`MiembrosPermite`→`guild_add_exp`/`guild_levels`, expulsión por alineación (`m_EstadoPermiteEntrar`). Faltan `GMEscuchaClan`/`GMDejaDeEscucharClan` (un GM escuchando el canal del clan), `ActualizarWebSite` (cosmético) y `SaveUserGuildRejectionReason` (comprobar si se guarda el motivo del rechazo). |
| `ModGrupos.bas` | `systems/party.rs`, `ao-rules/src/party.rs` | HECHO (casi) | `InvitarMiembro`→`party_invite`/`party_respond`, `EcharMiembro`→`party_kick`, `SalirDeGrupo(Forzado)`→`party_abandon`/`party_remove`, `FinalizarGrupo`→`party_disband`, `CalcularDarExpGrupal`→`share_exp`. Falta `CompartirUbicacion` (marcar en el mapa dónde están los compañeros). |
| `ModFacciones.bas` | `systems/faction.rs`, `ao-rules/src/faction` | HECHO | Están `EnlistarArmadaReal`/`EnlistarCaos`, `Recompensa*`, `DarRecompensas`, `Expulsar*`, `PerderItemsFaccionarios`, la puntuación de facción por muerte (`HandleFactionScoreForKill`→`faction_on_player_killed`, con `score`) y `VolverCriminal`. Falta comprobar `ForgiveUserFactionStats` (perdón) y `NotifyConnectionToFaction` (aviso de conexión a la facción); `TituloReal`/`TituloCaos` deberían venir de `ao-rules`. |
| `ModQuest.bas` | `systems/quest.rs` | PARCIAL | Están `EnviarQuest`, `CanUserAcceptQuest`, `FinishQuestCheck`, `FinishQuest`, `CleanQuestSlot`/`ArrangeUserQuests`, `AddDoneQuest`, los contadores de NPC y de visita, y la persistencia (tiene tests). Faltan las quests que empiezan al usar un objeto (`OtQuest`/`QuestOpenByObj`); el propio archivo lo reconoce. |
| `ModGlobalQuests.bas` | — (los datos `global_quest_index` se leen en `ao-formats`) | FALTA | `quest.rs` deja fuera a propósito las quests con `global_quest_index>0`. Falta todo: `ContributeToGlobalQuestCounter`, `StartGlobalQuest`/`FinalizeGlobalQuest`, `MaybeChangeGlobalQuestsState` (por fechas), jefes (`IsAnyGlobalQuestBossAlive`/`MarkGlobalQuestBossAsDead`) y las tablas de contribución. |
| `ModEventos.bas` (eventos por hora: multiplicador de oro, exp o drop) | `config.rs` solo tiene `exp_mult`/`gold_mult` fijos | FALTA | Faltan `CargarEventos` (de `Eventos.dat`/horario), `CheckEvento`/`IniciarEvento`/`FinalizarEvento`/`ForzarEvento` y el anuncio. Los multiplicadores existen pero son estáticos. |
| `ModRetos.bas` | `systems/event.rs` (duelos) | HECHO | Están `CrearReto`→`duel_propose`, `AceptarReto`→`duel_accept`, `CancelarSolicitudReto`, `BuscarSala`→`find_room` (con cola), `IniciarReto`/`iniciarRonda`→`start_duel`/`begin_round`, `MuereEnReto`/`ProcesarRondaGanada`→`round_won`, `FinalizarReto`, `AbandonarReto`, `SalaLiberada`, `PuedeReto`, 1v1 y 2v2, desconexión, tiempo agotado y apuestas. Tiene tests. Falta comprobar `TirarItemsEnPos`/`IniciarDepositoItems` (retos "por ítems": el que pierde suelta lo que lleva) y `NombreRepetidoEnReto`. |
| `ModTorneos.bas` | `event.rs` (`tournament_open/join/leave/start/close`) | HECHO | Se corresponden 1:1 con `IniciarTorneo`/`ParticiparTorneo`/`BorrarIndexInTorneo`/`ComenzarTorneoOk`/`ResetearTorneo`. |
| `ModSubasta.bas` | `event.rs` (`auction_publish/bid`, `tick_auctions`, `close_lot`) | HECHO (mejor) | Permite varios lotes, sobrevive a un reinicio y entrega por correo. La comisión del 5 % y los topes son los oficiales. |
| `ModCastle.bas` | — | FALTA | Faltan el castillo del emperador (`CreateNewEmperorCastle`, `CreateCastleInMap`/`DestroyCastleInMap`), las listas blancas de entrada (`CheckCastleEntryWhiteList`, `ModifyCastleEntryWhiteList`), el enfriamiento de reubicación y su persistencia en la BD. `event.rs` lo descarta a propósito. |
| `ModInvasion.bas` | — | FALTA | Faltan `IniciarInvasion`/`FinalizarInvasion`, `InvasionSpawnNPC`, `MuereNpcInvasion` y `SumarScoreInvasion`. Viene desactivado en los datos (`Cantidad=0`), así que el impacto es bajo. |
| `ModCaptura.bas`, `clsCaptura*.cls` (captura de la bandera) | — | FALTA | Tampoco existe el comando GM `IniciarCaptura`. |
| `ModLobby.bas`, `IBaseScenario`, `CustomScenarios.bas`, `ScenarioDeathMatch.cls`, `ScenarioHuntNpc.cls`, `ScoreBoard.cls` | — | FALTA | No hay ningún sistema de lobby. Faltan las inscripciones con filtros de nivel/clase/precio (`SetupLobby`, `CanPlayerJoin`, `AddPlayerOrGroup`), el reparto en equipos (`SortTeams`), la invocación y el retorno de los jugadores (`SummonAll`, `ReturnAllPlayers`), la reconexión, y los escenarios Deathmatch (zona de lucha que se cierra, ítems prohibidos o que aparecen, reaparición) y Caza de NPC, con su marcador y premios. `event.rs` reutiliza el texto del anuncio, pero solo para torneos. |
| `ModCollectibleCards.bas` | — (solo `collectible_card_index` en `ao-formats`) | FALTA | Faltan las cartas por cuenta (`AddCollectibleCardToUser`, el array de bits) y sus bonificaciones contra NPC: exp, daño, reducción de daño y drop (`GetCard*BonusForNpc`). |
| `ModShopAO20.bas` | — | FALTA | Tienda de créditos/premium (`init_transaction`, `is_purchaseable_item`, `RegisterTransaction`). Depende del modelo de negocio. |
| `ModEmail.bas` | (n/a) | n/a | Solo tiene utilidades (`GetUserGuildIndex`, `ObtenerCriminal`). El correo de jugadores (`mail.rs`) es de TSAO, no de AO20. |
| `Penas.bas` | `systems/gm.rs` (`penalty`, `gm_show_penalties`, `persist_sentence`) | PARCIAL | Están `BanPJ`→`gm_ban_char`, `BanearCuenta`/`DesbanearCuenta`→`gm_ban_account` y el historial. `GlobalChecks` no está identificado. |
| `BanTemporal.bas` + `Admin.BanTemporal` | — | FALTA | `gm_ban_char` solo banea para siempre (`penalty(...,"ban",0,...)`). Faltan el ban por días con su vencimiento (`ChangeBan`) y el comando `BanTemporal`. |
| `Admin.bas` | `gm.rs`, `session.rs` | PARCIAL | Están `Encarcelar`→`gm_jail` (con `RemoveCriticalObjectsWhileEnteringJail`→`remove_jail_items`), `PurgarPenas`→`tick_sentences`, `UserDarPrivilegioLevel`→`gm_set_role`, `UnBan`/`BANCheck`→`gm_login_refusal`, `WorldSave`→`autosave_all` y la invisibilidad del admin. Faltan comprobar `DetenerTrabajoPorCarcel` (cortar el trabajo al encarcelar), `ReSpawnOrigPosNpcs` y `CanUserSeeAdminInvisible` (entre rangos). |
| `Protocol_GmCommands.bas` | `gm.rs` (31 comandos), `gm_tsao.rs` (16 comandos), `chat.rs` (`/online`) | PARCIAL | De unos 130 manejadores, unos 45 tienen equivalente. Ver §2. |
| `CharacterPersistence.bas`, `Database_Queries`, `DatabaseGlobals` | `systems/account.rs`, `session.rs`, `server/src/db/*` (SQLite) | HECHO | Se guardan cuenta, personajes, inventario, banco, hechizos, quests, facción y clan. El esquema es propio, no el de MySQL. Faltan `SaveDCUserCache`/`RestoreDCUserCache` (al reconectar en pocos segundos, recuperar el estado sin pasar por la BD). |
| `Modulo_UsUaRiOs.bas` – conexión y creación | `account.rs` (`create_account`, `create_character`, `delete_character`, `play_character`), `progression.rs` (`init_new_character*`) | HECHO | Equivale a `ConnectUser_Check/Prepare/Complete`. La creación usa los valores oficiales (hay tests: humano mago, orco guerrero, gnomo mago). |
| `Modulo_UsUaRiOs.bas` – nivel y skills | `progression.rs` (`on_level_up`, `level_rewards`, `remove_newbie_items`, `on_assign_skill`) | HECHO | `CheckUserLevel` sube varios niveles a la vez. `SubirSkill` está en `work.rs`/`combat.rs`. |
| `Modulo_UsUaRiOs.bas` – muerte y resurrección | `combat.rs` (`user_die`, `resurrect`), `inventory.rs` (`drop_inventory_on_death`), `pets.rs` (`pets_on_owner_death`), `event.rs` (sin pérdida de ítems en eventos) | HECHO | Equivale a `UserDie`, `RevivirUsuario`/`ResurrectUser`, `HandleUserPetsOnDeath`, `ContarMuerte`/`AlreadyKilledBy`/`RegisterRecentKiller` (en `faction.rs`). |
| `Modulo_UsUaRiOs.bas` – estados | `combat.rs`, `magic.rs`, `movement.rs` | PARCIAL | Faltan `SetModoConsulta` (modo consulta del GM con un usuario), `GetUserSpouse` (matrimonio), `IsPatreon` y `CancelExit`/`Cerrar_Usuario` con cuenta atrás (comprobar en `session.rs`, `logout`). `LimpiarEstadosAlterados`, `StunPlayer` e `Inmovilize` corresponden al área de combate (parte 2). |
| `AntiCheat.bas` | `session.rs` (`strike`, límites de ritmo) y `tests/anticheat.rs` | n/a / distinto | El VB envuelve una DLL externa; aquí hay una validación propia en el servidor. No hay nada que portar 1:1. |
| `ModAutomatedActions.bas` | `systems/work.rs` (`work_start`, `tick_work`) | HECHO | Pertenece a la parte de trabajo. |
| `clsAntiMassClon.cls` | — | FALTA (baja) | Límite de conexiones por IP. |

## 2. Comandos GM

**Tenemos** (VB → Rust):
- En `gm.rs`: GMMessage→`/GMSG`, ServerMessage→`/RMSG`, Where→`/DONDE`, Punishments→`/PENAS`, Silence→`/SILENCIAR`, WarnUser→`/ADVERTENCIA`, Kick→`/ECHAR`, WarpChar→`/TELEP`, GoToChar→`/IRA`, SummonChar→`/SUM`, RequestCharInfo→`/INFO`, ReviveChar→`/REVIVIR`, Execute→`/EJECUTAR`, Jail→`/CARCEL`, BanChar→`/BAN`, UnbanChar→`/UNBAN`, BanCuenta/UnBanCuenta→`/BANCUENTA`/`/UNBANCUENTA`, Invisible→`/INVISIBLE`, CreateNPC/CreateNPCWithRespawn→`/ACC`/`/RACC`, KillNPC→`/MATA`, EditChar→`/MOD`, CreateItem→`/CI`, GiveItem→`/DAR`, `/QUITAR`, `/ROL` (UserDarPrivilegioLevel), `/APAGAR`, `/BANIP`/`/UNBANIP` (extras), `/LOG`.
- En `gm_tsao.rs`: GoNearby→`/IRCERCA`, CreaturesInMap→`/NENE`, ItemsInTheFloor→`/SOBJ`, DestroyItems→`/DEST`, SetTrigger→`/TRIGGER`, Tile_BlockedToggle→`/BLOQ`, KillNPCNoRespawn y KillAllNearbyNPCs (parcial, `/KILL`), Forgive→`/LIBERAR`.
- En `chat.rs`: Online→`/online`. Torneos y subastas tienen su propio panel de eventos.

**Faltan**, por prioridad:
- **Alta** (moderación diaria): `BanTemporal`, `SOSShowList`/`SOSRemove`/`CleanSOS` y `Denounce` (sistema de ayuda/SOS y denuncias), `RequestCharStats`/`RequestCharGold`/`RequestCharInventory`/`RequestCharBank`/`RequestCharSkills` (inspeccionar a un jugador), `RemovePunishment`, `MakeDumb`/`MakeDumbNoMore`, `NickToIP`/`IPToNick`, `OnlineGM`/`OnlineMap`, `RequestUserList`, `Working`/`Hiding`, `KickAllChars`, `SaveChars`/`DoBackUp`, `CerrarCliente`, `AlterName`, `GMPanel` (comprobar `gm_send_panel`).
- **Media**: `WarpMeToTarget`, `NPCFollow`, `SpawnListRequest`/`SpawnCreature`, `ResetNPCInventory`, `CleanWorld`, `DestroyAllItemsInArea`, `TeleportCreate`/`TeleportDestroy`, `ChangeMapInfo*` (PK, Backup, Restricted, NoMagic, NoInvi, NoResu, Land, Zone) y `ChangeMapSetting`, `SetMOTD`/`ChangeMOTD` y `RequestMOTD`, `GlobalMessage`/`GlobalOnOff`, `CuentaRegresiva`, `ShowName`, `SetCharDescription`, `TalkAsNPC`, `AskTrigger`, `CheckSlot`, `SetSpeed`, `PossUser`, `MarcaDeGM`, `CancelarExit`, `CreateEvent`/`StartEvent`/`CancelarEvento`/`FinEvento`/`EventoInfo`/`CrearEvento` (dependen del lobby y de ModEventos), `CrearTorneo`/`ComenzarTorneo`/`CancelarTorneo` (comprobar si el panel de eventos ya los cubre), `EventoFaccionario`, `LobbyCommand`, `IniciarCaptura`, `BusquedaTesoro`.
- **Baja**: `RainToggle`/`NieveToggle`/`NieblaToggle` (`weather.rs` no tiene comando), `ForceMIDIAll`/`ForceWAVEAll`/`ForceWAVEToMap`, `Genio`, `QuestionGM`, `DebugRequest`, `FeatureToggle`, `ShowServerForm`, `Ignored`, `UpTime`, `Help`, `RequestStats`, `RoleMasterRequest`, `Gamble`, `MapPriceEntrance`.

## 3. Faltantes priorizados

| # | Qué falta | Impacto | Esfuerzo |
|---|---|---|---|
| 1 | Ban temporal: días, vencimiento y comando | Alta | S |
| 2 | Comandos GM de inspección y moderación (stats/oro/inventario/banco/skills, NickToIP, MakeDumb, RemovePunishment, OnlineMap/GM, KickAll, SaveChars) | Alta | M |
| 3 | SOS y denuncias (cola de ayuda para los GM) | Alta | M |
| 4 | Lobby y escenarios (Deathmatch, Caza de NPC, ScoreBoard) más los comandos CreateEvent/StartEvent/LobbyCommand | Alta (son los eventos automáticos de AO20) | L |
| 5 | ModEventos: multiplicadores de exp/oro/drop por hora y ForzarEvento | Media | S |
| 6 | Quests globales (contador común, jefes, fechas, BD) | Media | M |
| 7 | Quests que empiezan al usar un objeto (`QuestOpenByObj`) | Media | S |
| 8 | Castillo del emperador con lista blanca | Media | L |
| 9 | Cartas coleccionables y sus bonificaciones contra NPC | Media | M |
| 10 | Comandos GM de mapa y mundo (ChangeMapInfo*, Teleport*, CleanWorld, MOTD, CuentaRegresiva, GlobalOnOff) | Media | M |
| 11 | Captura de la bandera (ModCaptura y comando IniciarCaptura) | Baja/Media | L |
| 12 | CompartirUbicacion del grupo y GMEscuchaClan | Baja | S |
| 13 | Caché de reconexión rápida (`SaveDCUserCache`) | Baja | M |
| 14 | Invasiones (desactivadas en los datos oficiales) | Baja | M |
| 15 | Matrimonio (`GetUserSpouse`), Patreon, tienda AO20 (`ModShopAO20`) y AntiMassClon | Baja | S–M |
| 16 | Comandos GM de clima y sonido (Rain, Nieve, Niebla, ForceMIDI/WAVE) | Baja | S |

## 4. Cobertura estimada: alrededor del 55 %

Se ponderó cada sistema por su peso en el juego real:
- Núcleo social y de personaje, unos 50 puntos de peso: clanes, grupos, facciones, quests normales, cuentas/creación/persistencia, nivel, muerte y resurrección. Está entre el 90 y el 95 % y aporta unos 46 puntos.
- Eventos, unos 25 puntos de peso: retos, torneos y subastas están hechos (unos 10 puntos); lobby/escenarios, ModEventos, quests globales, castillo, captura, invasión y cartas faltan del todo. El bloque queda en torno al 40 % (unos 10 puntos).
- GM y penas, unos 20 puntos de peso: hay unos 45 de 130 comandos, pero entre ellos están los más usados. Falta el ban temporal y el SOS. Queda en torno al 45 % (unos 9 puntos).
- Tienda, Patreon, antimasa y matrimonio, unos 5 puntos de peso: 0 %.

Total: unos 65 de 100 en bruto. Como faltan varios sistemas grandes que AO20 usa a diario (lobby y quests globales), la cifra se rebaja a un **55 % prudente**.

Hay puntos marcados «comprobar» que no se pudieron confirmar solo con grep: retos por ítems, `ForgiveUserFactionStats`, `CancelExit` y `DetenerTrabajoPorCarcel`.

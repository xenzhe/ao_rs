//! Lobbies and custom scenarios: port of `ModLobby.bas`, `IBaseScenario.cls`,
//! `CustomScenarios.bas`, `ScoreBoard.cls`, `ScenarioDeathMatch.cls` and
//! `ScenarioHuntNpc.cls`. Pure rules (teams, scoreboard, prizes) are in
//! `ao_rules::events`.
//!
//! **Flow** (as in AO20): a GM (or the automatic calendar) creates a public event with
//! `/CREAREVENTO LOBBY|CACERIA|DEATHMATCH participantes nivel_min nivel_max [precio]
//! [tamaño_equipo]`; the inscriptions open at once and are announced. Players join with
//! `/PARTICIPAR [id contraseña]` (the leader of a party when it is by premade teams) or by
//! clicking an EventMaster NPC (it lists the open rooms, `/SALAS` does the same). After 5
//! minutes (or when full) it starts if there are enough players, else it is cancelled and
//! the fees are returned. The GM tunes it with `/CONFIGLOBBY` (SPAWN, MAXLVL, MINLVL,
//! SUMPLAYER, SUMALL, RETURNPLAYER, RETALL, OPEN, START, END, LIST, CLASS, KICK,
//! FORCERESET, SETTEAMSIZE, ADDPLAYER, SETPRICE) and stops it with `/CANCELAREVENTO`.
//!
//! **Decisions**:
//! - No map instances: the VB cloned the scenario map (49) into a free number
//!   (`InstanceManager.CloneMap`), which our client cannot load (it reads the map file by
//!   number). The scenario runs on its map; one scenario per map at a time.
//! - The hunt reads `Dat/Scenarios/NpcHunt.ini`, which the official data does not have:
//!   `snakes.ini` (same format, made for it) is used when it is missing.
//! - Players keep their items when dying in a scenario (the VB maps had `DropItems=0`) and
//!   killing there has no faction consequences (`SafeFightMap`).
//!
//! **VB defects fixed**: `OpenLobby` refused every lobby that had a spawn point
//! (`RequiresSpawn Or Map > 0` instead of `And Map <= 0`); `/PARTICIPAR` without a global
//! lobby read `LobbyList(-1)`; `HandleStartEvent` (GM path) checked no rank at all; a second
//! public event overwrote the running one (`GlobalLobbyIndex`) leaving it orphaned;
//! `/CONFIGLOBBY KICK` was sent by the client but never handled; the deathmatch summoned
//! everyone to the same tile (`RandomNumber(50 - Min, 50 - Min)`); `RespawnItems` used the
//! stack count of item 0 for every item; dead players were sent home as ghosts when the
//! match ended (they are revived now); a generic lobby without spawn tried to warp to map
//! -1 (players stay where they are until the GM sets one); scores keyed by the user slot
//! were lost on reconnection (keyed by the lobby seat now).
use crate::clock::GameTime;
use crate::world::inventory::InvItem;
use crate::world::World;
use ao_formats::dat::npcs::NpcType;
use ao_protocol::p4::gm::GmRank;
use ao_protocol::{Tone, S2C};
use ao_rules::events::{self as rules, EventType, ScoreBoard};
use std::collections::HashMap;

/// `WaitingForPlayersTime`: 5 minutes of inscriptions.
pub const WAITING_MS: u64 = 300_000;
/// `BroadOpenEvent`: reminder every 30 seconds.
const REMINDER_MS: u64 = 30_000;
/// `LobbyCount`.
const MAX_LOBBIES: usize = 200;
/// `AssistDamageValidTime` (Balance.dat [EXTRA]).
const ASSIST_MS: u64 = 8_000;
/// `RequiredWeaponId`: the club of the hunt.
pub const HUNT_WEAPON: u16 = 1847;
/// Hearts FX (`e_ParticleEffects.Corazones` is a particle; FX 35 is the resurrection).
const RESPAWN_FX: u16 = 35;
const RESPAWN_WAV: u16 = 104;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum LobbyState {
    Uninitialized,
    Initialized,
    AcceptingPlayers,
    InProgress,
    Completed,
    Closed,
}

/// `PlayerInLobby`.
#[derive(Debug, Clone)]
pub struct Seat {
    /// Stable key of the seat (scoreboard key).
    pub key: i64,
    pub name: String,
    pub id: Option<u32>,
    pub team: u32,
    pub from: Option<(u16, i32, i32)>,
    pub summoned: bool,
    pub return_on_reconnect: bool,
}

/// `t_NewScenearioSettings`.
#[derive(Debug, Clone, Default)]
pub struct LobbySettings {
    pub kind: Option<EventType>,
    pub min_level: u32,
    pub max_level: u32,
    pub min_players: u32,
    pub max_players: u32,
    pub team_size: u32,
    pub premade: bool,
    pub fee: u32,
    pub description: String,
    pub password: String,
}

/// `t_Lobby`.
#[derive(Debug)]
pub struct Lobby {
    pub id: u32,
    pub min_level: u32,
    pub max_level: u32,
    pub max_players: u32,
    pub min_players: u32,
    pub class_filter: Option<u8>,
    pub state: LobbyState,
    pub spawn: Option<(u16, i32, i32)>,
    pub team_size: u32,
    pub premade: bool,
    pub next_team: u32,
    pub fee: u32,
    pub pot: u64,
    pub canceled: bool,
    pub description: String,
    pub password: String,
    pub public: bool,
    pub opened_at: GameTime,
    pub next_reminder: GameTime,
    pub seats: Vec<Seat>,
    pub next_key: i64,
    pub scenario: Option<Scenario>,
}

/// `IBaseScenario` implementations.
#[derive(Debug)]
pub enum Scenario {
    DeathMatch(Arena),
    Hunt(Arena),
}

/// Shared state of the deathmatch and the hunt (both are an arena on one map).
#[derive(Debug, Default)]
pub struct Arena {
    pub map: u16,
    pub target_score: i32,
    pub max_ms: u64,
    pub respawn_ms: u64,
    pub board: ScoreBoard,
    /// `CountdownTimer.Occurrences` / `StartTimer`: 10 one-second steps.
    pub countdown: u32,
    pub next_count: GameTime,
    pub started: bool,
    pub started_at: GameTime,
    pub next_time_notice: GameTime,
    pub next_score: GameTime,
    /// Seat key -> when to respawn.
    pub respawn: HashMap<i64, GameTime>,
    /// Map flags saved by `ConfigMapLobby` and restored by `ClearMap`.
    pub saved_safe: bool,
    pub closed: bool,
    // Deathmatch
    pub forbidden: Vec<u16>,
    /// Seat key -> items taken away while playing (`SpawnedUserList`).
    pub removed: HashMap<i64, Vec<InvItem>>,
    pub items: Vec<SpawnItem>,
    pub min_arena: i32,
    pub area_tenths: i32,
    pub ring: [u16; 6],
    /// Fight area (x1, y1, x2, y2).
    pub area: (i32, i32, i32, i32),
    /// Tiles blocked by the ring and their previous block bits.
    pub ring_tiles: Vec<(i32, i32, u8)>,
    /// Victim seat -> (last attacker seat, when), for assists.
    pub last_hit: HashMap<i64, (i64, GameTime)>,
    // Hunt
    pub targets: Vec<HuntTarget>,
    /// Creatures alive in the hunt.
    pub spawned: Vec<u32>,
}

#[derive(Debug, Clone)]
pub struct SpawnItem {
    pub obj: u16,
    pub amount: u32,
    pub stack_tenths: i32,
    pub initial_tenths: i32,
    pub every_ms: u64,
    pub next: GameTime,
}

/// `EventNpcDefinition`.
#[derive(Debug, Clone)]
pub struct HuntTarget {
    pub npc: u16,
    pub count: u32,
    pub score: i32,
}

#[derive(Debug, Default)]
pub struct LobbyWorld {
    pub lobbies: Vec<Lobby>,
    pub next_id: u32,
    /// `GlobalLobbyIndex`.
    pub global: Option<u32>,
    pub next_tick: GameTime,
}

impl Arena {
    fn name(kind: &Scenario) -> &'static str {
        match kind {
            Scenario::DeathMatch(_) => "DeathMatch",
            Scenario::Hunt(_) => "Caceria",
        }
    }
}

impl Scenario {
    pub fn arena(&self) -> &Arena {
        match self {
            Scenario::DeathMatch(a) | Scenario::Hunt(a) => a,
        }
    }
    pub fn arena_mut(&mut self) -> &mut Arena {
        match self {
            Scenario::DeathMatch(a) | Scenario::Hunt(a) => a,
        }
    }
    pub fn name(&self) -> &'static str {
        Arena::name(self)
    }
}

/// Class names of `/CONFIGLOBBY CLASS` (`ConfigLobbyClass`).
fn class_id(s: &str) -> Option<u8> {
    Some(match s.to_uppercase().as_str() {
        "MAGE" | "MAGO" => 1,
        "CLERIC" | "CLERIGO" => 2,
        "WARRIOR" | "GUERRERO" => 3,
        "ASSASIN" | "ASESINO" => 4,
        "BARD" | "BARDO" => 5,
        "DRUID" | "DRUIDA" => 6,
        "PALADIN" => 7,
        "HUNTER" | "CAZADOR" => 8,
        "WORKER" | "TRABAJADOR" => 9,
        "PIRATE" | "PIRATA" => 10,
        "THIEF" | "LADRON" => 11,
        "BANDIT" | "BANDIDO" => 12,
        _ => return None,
    })
}

impl World {
    // ================================================================== helpers (phase 6)

    /// Everybody's console.
    pub(crate) fn f6_say_all(&self, text: impl Into<String>) {
        self.broadcast_all(&S2C::Console { text: text.into(), color: Tone::Info }, None);
    }

    pub(crate) fn f6_say_map(&self, map: u16, text: impl Into<String>) {
        self.broadcast(map, &S2C::Console { text: text.into(), color: Tone::Info }, None);
    }

    /// Creates an npcs.dat creature on a free tile near (x, y). `no_respawn`: removed for
    /// good when it dies (`SpawnNpc(.., Respawn:=False)`).
    pub(crate) fn f6_spawn_npc(&mut self, map: u16, number: u16, x: i32, y: i32, no_respawn: bool) -> Option<u32> {
        if !self.ensure_map(map) {
            return None;
        }
        let ao = self.ao.clone();
        let d = ao.npcs.get(number as u32).filter(|d| !d.name.is_empty())?;
        let (x, y) = self.free_tile_near(map, x.clamp(1, 100), y.clamp(1, 100), 3);
        let now = self.now();
        let mut npc = self.new_npc(number, d, x, y, now);
        // Gone for good when it dies (tick_respawns), not back at a random tile.
        npc.pet.no_respawn = no_respawn;
        let nid = self.next_npc;
        self.next_npc += 1;
        let msg = npc.spawn_msg(nid);
        self.maps.get_mut(&map).unwrap().add_npc(nid, npc);
        self.broadcast(map, &msg, None);
        if no_respawn {
            self.gms.no_respawn.insert((map, nid));
        }
        Some(nid)
    }

    /// `QuitarNPC`: off the map for good.
    pub(crate) fn f6_remove_npc(&mut self, map: u16, nid: u32) {
        if let Some(inst) = self.maps.get_mut(&map) {
            if inst.remove_npc(nid).is_some() {
                self.broadcast(map, &S2C::Despawn { id: nid }, None);
            }
        }
        self.gms.no_respawn.remove(&(map, nid));
    }

    /// `AddGold`.
    pub(crate) fn f6_give_gold(&mut self, id: u32, amount: u64) {
        if self.player(id).is_none() || amount == 0 {
            return;
        }
        let p = self.player_mut(id);
        p.inv.gold = (p.inv.gold as u64 + amount).min(u32::MAX as u64) as u32;
        self.send_inventory_vitals(id);
    }

    /// `RemoveGold`: false if there is not enough.
    pub(crate) fn f6_take_gold(&mut self, id: u32, amount: u32) -> bool {
        let Some(p) = self.player(id) else { return false };
        if p.inv.gold < amount {
            return false;
        }
        self.player_mut(id).inv.gold -= amount;
        self.send_inventory_vitals(id);
        true
    }

    /// `WarpToLegalPos`.
    pub(crate) fn f6_warp(&mut self, id: u32, map: u16, x: i32, y: i32) {
        if self.player(id).is_some() && self.ensure_map(map) {
            self.teleport(id, map, x.clamp(1, 100), y.clamp(1, 100));
        }
    }

    fn f6_jailed(&self, id: u32) -> bool {
        self.player(id).is_some_and(|p| p.gm.jail_ms > 0)
    }

    fn scenario_ini(&self, file: &str) -> Option<HashMap<String, HashMap<String, String>>> {
        let path = self.recursos.join("Dat").join("Scenarios").join(file);
        let bytes = std::fs::read(path).ok()?;
        let text: String = bytes.iter().map(|b| *b as char).collect();
        Some(rules::ini(&text))
    }

    // ================================================================== lookups

    fn lobby_idx(&self, lobby: u32) -> Option<usize> {
        self.lobbies.lobbies.iter().position(|l| l.id == lobby)
    }

    /// The lobby and seat of a player (by name: seats survive reconnections).
    pub(crate) fn lobby_seat_of(&self, id: u32) -> Option<(usize, usize)> {
        let name = self.player(id)?.name.clone();
        self.lobbies.lobbies.iter().enumerate().find_map(|(li, l)| {
            (l.state >= LobbyState::AcceptingPlayers && l.state < LobbyState::Closed).then_some(())?;
            l.seats.iter().position(|s| s.name.eq_ignore_ascii_case(&name)).map(|si| (li, si))
        })
    }

    /// The lobby whose scenario owns `map`.
    fn scenario_on(&self, map: u16) -> Option<usize> {
        self.lobbies.lobbies.iter().position(|l| l.scenario.as_ref().is_some_and(|s| s.arena().map == map && !s.arena().closed))
    }

    /// HOOK (event_keeps_items_on_death): players in a running scenario keep everything.
    pub(crate) fn lobby_keeps_items(&self, id: u32) -> bool {
        let Some(map) = self.player(id).map(|p| p.map) else { return false };
        self.scenario_on(map).is_some()
    }

    /// HOOK (faction): fights inside a scenario or the capture have no faction
    /// consequences (`SafeFightMap`, `jugando_captura`).
    pub(crate) fn event_safe_fight(&self, id: u32) -> bool {
        self.lobby_keeps_items(id) || self.capture_has(id)
    }

    /// `CurrentTeam` of both is the same team (> 0).
    pub(crate) fn event_same_team(&self, a: u32, b: u32) -> bool {
        let team = |id: u32| -> u32 {
            if let Some(t) = self.capture_team(id) {
                return t as u32;
            }
            self.lobby_seat_of(id).map_or(0, |(l, s)| self.lobbies.lobbies[l].seats[s].team)
        };
        let t = team(a);
        t > 0 && t == team(b)
    }

    // ================================================================== creation

    /// `CreatePublicEvent` (+ `ValidateLobbySettings`, `SetupLobby`, `PrepareNewEvent`,
    /// `OpenLobby`). Returns an error text to show the GM.
    pub(crate) fn lobby_create_public(&mut self, s: LobbySettings) -> Result<u32, String> {
        rules::validate_levels(s.min_level, s.max_level).map_err(str::to_string)?;
        if let Some(g) = self.lobbies.global.and_then(|g| self.lobby_idx(g)) {
            if self.lobbies.lobbies[g].state < LobbyState::Completed {
                return Err("Ya hay un evento en curso. Cancélalo con /CANCELAREVENTO primero.".into());
            }
        }
        if self.lobbies.lobbies.len() >= MAX_LOBBIES {
            return Err("No hay salas libres.".into());
        }
        let scenario = match s.kind.unwrap_or(EventType::Generic) {
            EventType::Generic => None,
            EventType::DeathMatch => Some(Scenario::DeathMatch(self.load_deathmatch()?)),
            EventType::NpcHunt => Some(Scenario::Hunt(self.load_hunt()?)),
            EventType::NavalBattle => return Err("La batalla naval todavía no está disponible.".into()),
            EventType::CaptureTheFlag => return Err("La captura de bandera se crea con /CREAREVENTO CAPTURA.".into()),
        };
        if let Some(sc) = &scenario {
            if self.scenario_on(sc.arena().map).is_some() {
                return Err("El mapa del escenario ya está en uso.".into());
            }
        }
        let max_players = s.max_players.max(1);
        let team_size = if s.team_size <= 1 { 1 } else { s.team_size };
        if max_players % team_size != 0 {
            return Err("La cantidad maxima de jugadores no es divisible por la cantidad de grupos.".into());
        }
        self.lobbies.next_id += 1;
        let id = self.lobbies.next_id;
        let now = self.now();
        let mut lobby = Lobby {
            id,
            min_level: s.min_level,
            max_level: s.max_level,
            max_players,
            min_players: s.min_players.max(1),
            class_filter: None,
            state: LobbyState::Initialized,
            spawn: None,
            team_size,
            premade: s.premade,
            next_team: 1,
            fee: s.fee,
            pot: 0,
            canceled: false,
            description: s.description,
            password: s.password,
            public: true,
            opened_at: now,
            next_reminder: now.after_ms(REMINDER_MS),
            seats: Vec::new(),
            next_key: 1,
            scenario,
        };
        // ConfigMapLobby: the scenario map is a safe zone while waiting.
        if let Some(sc) = &mut lobby.scenario {
            let map = sc.arena().map;
            if !self.ensure_map(map) {
                return Err("No se pudo cargar el mapa del escenario.".into());
            }
            let a = sc.arena_mut();
            a.saved_safe = self.maps[&map].map.safe;
            self.maps.get_mut(&map).unwrap().map.safe = true;
        }
        self.lobbies.lobbies.push(lobby);
        self.lobbies.global = Some(id);
        self.lobby_open(id, true);
        Ok(id)
    }

    fn load_deathmatch(&self) -> Result<Arena, String> {
        let ini = self.scenario_ini("DeathMatch.ini").ok_or("Falta Dat/Scenarios/DeathMatch.ini.")?;
        let get = |s: &str, k: &str| ini.get(s).and_then(|m| m.get(k)).and_then(|v| v.parse::<i64>().ok()).unwrap_or(0);
        let mut a = Arena {
            map: get("init", "mapnumber") as u16,
            target_score: get("init", "targetscore") as i32,
            max_ms: get("init", "maxtime") as u64 * 1000,
            respawn_ms: get("init", "playerrespawntime") as u64 * 1000,
            min_arena: get("init", "minarenasize") as i32,
            area_tenths: get("init", "playerareaincreasemodifier") as i32,
            ring: [
                get("init", "ringlimiterhitemid") as u16,
                get("init", "ringlimitervitemid") as u16,
                get("init", "ringlimitertlitemid") as u16,
                get("init", "ringlimitertritemid") as u16,
                get("init", "ringlimiterblitemid") as u16,
                get("init", "ringlimiterbritemid") as u16,
            ],
            ..Default::default()
        };
        for i in 1..=get("forbidden", "count") {
            a.forbidden.push(get("forbidden", &format!("itemid{i}")) as u16);
        }
        for i in 1..=get("init", "spawnitems") {
            let s = format!("spawnitem{i}");
            a.items.push(SpawnItem {
                obj: get(&s, "itemid") as u16,
                amount: get(&s, "itemcount") as u32,
                stack_tenths: get(&s, "stackcount") as i32,
                initial_tenths: get(&s, "initialstacks") as i32,
                every_ms: get(&s, "spawninterval").max(1) as u64 * 1000,
                next: GameTime::ZERO,
            });
        }
        (a.map > 0).then_some(a).ok_or_else(|| "DeathMatch.ini no tiene mapa.".into())
    }

    fn load_hunt(&self) -> Result<Arena, String> {
        let ini = self.scenario_ini("NpcHunt.ini").or_else(|| self.scenario_ini("snakes.ini")).ok_or("Falta Dat/Scenarios/NpcHunt.ini (o snakes.ini).")?;
        let get = |s: &str, k: &str| ini.get(s).and_then(|m| m.get(k)).and_then(|v| v.parse::<i64>().ok()).unwrap_or(0);
        let mut a = Arena {
            map: get("init", "mapnumber") as u16,
            target_score: get("init", "targetscore") as i32,
            max_ms: get("init", "maxtime") as u64 * 1000,
            respawn_ms: get("init", "playerrespawntime") as u64 * 1000,
            ..Default::default()
        };
        for i in 1..=get("init", "targetnpc") {
            let s = format!("target{i}");
            let npc = get(&s, "npcid") as u16;
            if npc > 0 && !a.targets.iter().any(|t| t.npc == npc) {
                a.targets.push(HuntTarget { npc, count: get(&s, "count") as u32, score: get(&s, "score") as i32 });
            }
        }
        (a.map > 0).then_some(a).ok_or_else(|| "El archivo de la cacería no tiene mapa.".into())
    }

    /// `OpenLobby`.
    fn lobby_open(&mut self, lobby: u32, public: bool) {
        let Some(li) = self.lobby_idx(lobby) else { return };
        let now = self.now();
        let l = &mut self.lobbies.lobbies[li];
        l.public = public;
        l.state = LobbyState::AcceptingPlayers;
        l.opened_at = now;
        l.next_reminder = now.after_ms(REMINDER_MS);
        if !public {
            return;
        }
        let name = if !l.description.is_empty() { l.description.clone() } else { l.scenario.as_ref().map_or("Evento", |s| s.name()).to_string() };
        let (max, min, maxl, fee) = (l.max_players, l.min_level, l.max_level, l.fee);
        let sc = l.scenario.as_ref().map(|s| matches!(s, Scenario::DeathMatch(_)));
        self.f6_say_all(format!(
            "Se creó un evento de tipo \"{name}\". Hay solamente {max} cupos disponibles para este evento. Se permite el ingreso a los personajes de nivel {min} hasta nivel {maxl} inclusive. El costo de inscripción del evento es de {fee} monedas de oro. Para ingresar al mismo, escribe /PARTICIPAR (si es un evento privado, escribe /PARTICIPAR {lobby} CONTRASEÑA). Si el evento es en grupos, solo el líder deberá enviar el comando /PARTICIPAR."
        ));
        match sc {
            Some(true) => self.f6_say_all("Recorda que no podes tener pociones rojas o azules para ingresar, estas aparecen automaticamente durante la partida."),
            Some(false) => self.f6_say_all("Usa un garrote para matar serpientes, al final del tiempo quien haya sumado mas puntos gana la partida, debes tener un garrote en el inventario para poder participar."),
            None => {}
        }
        if fee > 0 {
            self.f6_say_all(format!("El precio de la inscripcion es de {fee} monedas de oro por participante."));
        }
    }

    // ================================================================== joining

    /// `CanPlayerJoin`: None = may join, else the official refusal.
    fn lobby_refusal(&self, li: usize, id: u32) -> Option<String> {
        let l = &self.lobbies.lobbies[li];
        let p = self.player(id)?;
        if p.level < l.min_level || p.level > l.max_level {
            return Some("No puedes participar debido a tu nivel.".into());
        }
        if l.seats.len() as u32 >= l.max_players {
            return Some("La sala ya esta llena.".into());
        }
        if l.class_filter.is_some_and(|c| c != p.class) {
            return Some("Tu clase no puede participar.".into());
        }
        if self.is_dead(id) {
            return Some("No puedes estar muerto para entrar a las arenas.".into());
        }
        if p.event.room.is_some() || p.event.in_tournament || self.capture_has(id) {
            return Some("Ya estas en un evento, debes terminar para poder participar en otro.".into());
        }
        if p.inv.gold < l.fee {
            return Some("No tienes suficiente oro para pagar el ingreso.".into());
        }
        if self.f6_jailed(id) {
            return Some("No puedes participar estando encarcelado.".into());
        }
        if matches!(l.scenario, Some(Scenario::Hunt(_))) && !p.inv.slots.iter().flatten().any(|i| i.obj == HUNT_WEAPON) {
            return Some("Debes tener un Garrote en tu inventario para participar.".into());
        }
        if self.lobby_seat_of(id).is_some() {
            return Some("Ya estas registrado.".into());
        }
        None
    }

    /// `AddPlayer`.
    fn lobby_add(&mut self, li: usize, id: u32, team: u32) -> Result<(), String> {
        if let Some(why) = self.lobby_refusal(li, id) {
            return Err(why);
        }
        let fee = self.lobbies.lobbies[li].fee;
        if fee > 0 {
            if !self.f6_take_gold(id, fee) {
                return Err("No tienes suficiente oro para pagar el ingreso.".into());
            }
            self.lobbies.lobbies[li].pot += fee as u64;
        }
        let name = self.name_of(id);
        let l = &mut self.lobbies.lobbies[li];
        let key = l.next_key;
        l.next_key += 1;
        l.seats.push(Seat { key, name, id: Some(id), team, from: None, summoned: false, return_on_reconnect: false });
        let si = l.seats.len() - 1;
        match &l.scenario {
            Some(Scenario::DeathMatch(_)) => self.console(id, "Te has anotado para jugar un Deathmatch. En este evento, por cada enemigo que mates sumarás 3 puntos, por cada asistencia de muerte sumarás 1 punto y cada vez que mueras, restarás 1 punto. Al finalizar el tiempo, quien haya sumado mas puntos, gana el evento.", Tone::Info),
            Some(Scenario::Hunt(_)) => self.console(id, "Solo las serpientes que mates golpeando con el garrote suman puntos, gana quien sume mas puntos al terminar el tiempo.", Tone::Info),
            None => {}
        }
        // SummonAfterInscription.
        self.lobby_summon(li, si);
        Ok(())
    }

    /// `AddPlayerOrGroup` (+ `HandleParticipar` for lobbies).
    pub(crate) fn lobby_join(&mut self, id: u32, lobby: Option<u32>, password: &str) -> bool {
        let lobby = lobby.or(self.lobbies.global);
        let Some(li) = lobby.and_then(|l| self.lobby_idx(l)) else { return false };
        let l = &self.lobbies.lobbies[li];
        if l.state != LobbyState::AcceptingPlayers {
            return false;
        }
        if !l.public {
            self.console(id, "No podes ingresar a una sala privada.", Tone::Warn);
            return true;
        }
        if password != l.password {
            self.console(id, "Contraseña incorrecta.", Tone::Warn);
            return true;
        }
        if l.team_size > 1 && l.premade {
            let team_size = l.team_size as usize;
            let Some(pid) = self.party_of(id) else {
                self.console(id, "Necesitas estar en grupo para participar.", Tone::Warn);
                return true;
            };
            let party = self.parties.by_id.get(&pid).cloned();
            let Some(party) = party else { return true };
            if party.leader != id {
                self.console(id, "Solo el lider del grupo puede anotarse.", Tone::Warn);
                return true;
            }
            let members = self.party_members_online(id);
            if members.len() != team_size {
                self.console(id, "El grupo no tiene la cantidad de jugadores requerida para participar.", Tone::Warn);
                return true;
            }
            for m in &members {
                if let Some(why) = self.lobby_refusal(li, *m) {
                    let n = self.name_of(*m);
                    self.console(id, format!("{n}: no puede participar, motivo: {why}"), Tone::Warn);
                    return true;
                }
            }
            let team = self.lobbies.lobbies[li].next_team;
            self.lobbies.lobbies[li].next_team += 1;
            for m in members {
                match self.lobby_add(li, m, team) {
                    Ok(()) => self.console(m, "Has Ingresado al evento.", Tone::Good),
                    Err(why) => self.console(m, why, Tone::Warn),
                }
            }
        } else {
            match self.lobby_add(li, id, 0) {
                Ok(()) => self.console(id, "Has Ingresado al evento.", Tone::Good),
                Err(why) => self.console(id, why, Tone::Warn),
            }
        }
        true
    }

    /// `SummonPlayer` (+ the scenario's `WillSummonPlayer`).
    fn lobby_summon(&mut self, li: usize, si: usize) {
        let Some(id) = self.lobbies.lobbies[li].seats[si].id.filter(|i| self.player(*i).is_some()) else { return };
        let pos = self.player(id).map(|p| (p.map, p.x, p.y)).unwrap();
        let key = self.lobbies.lobbies[li].seats[si].key;
        let mut dest = self.lobbies.lobbies[li].spawn;
        if let Some(sc) = &self.lobbies.lobbies[li].scenario {
            let a = sc.arena();
            let (x, y) = match sc {
                Scenario::DeathMatch(a) => (self.rng.range(50 - a.min_arena, 50 + a.min_arena), self.rng.range(50 - a.min_arena, 50 + a.min_arena)),
                Scenario::Hunt(_) => (self.rng.range(12, 85), self.rng.range(12, 85)),
            };
            dest = Some((a.map, x, y));
        }
        let Some((m, x, y)) = dest else { return };
        {
            let seat = &mut self.lobbies.lobbies[li].seats[si];
            if !seat.summoned && seat.from.is_none() {
                seat.from = Some(pos);
            }
            seat.summoned = true;
        }
        if let Some(sc) = &mut self.lobbies.lobbies[li].scenario {
            sc.arena_mut().board.add_player(key);
        }
        if matches!(self.lobbies.lobbies[li].scenario, Some(Scenario::DeathMatch(_))) {
            self.dm_take_forbidden(li, si, id);
        }
        self.f6_warp(id, m, x, y);
    }

    /// `ReturnPlayer`: back where they were summoned from (revived if they died there).
    fn lobby_return(&mut self, li: usize, si: usize) {
        let seat = self.lobbies.lobbies[li].seats[si].clone();
        let Some(id) = seat.id.filter(|i| self.player(*i).is_some()) else { return };
        // MatchCompleted already gave everything back.
        if matches!(self.lobbies.lobbies[li].scenario, Some(Scenario::DeathMatch(_))) && self.lobbies.lobbies[li].state < LobbyState::Completed {
            self.dm_give_back(li, seat.key, id);
        }
        if let Some(sc) = &mut self.lobbies.lobbies[li].scenario {
            sc.arena_mut().respawn.remove(&seat.key);
        }
        self.lobbies.lobbies[li].seats[si].summoned = false;
        if !seat.summoned {
            return;
        }
        if self.is_dead(id) {
            self.resurrect(id);
        }
        if let Some((m, x, y)) = seat.from {
            self.f6_warp(id, m, x, y);
        }
    }

    fn lobby_return_all(&mut self, li: usize) {
        for si in 0..self.lobbies.lobbies[li].seats.len() {
            self.lobby_return(li, si);
        }
    }

    fn lobby_summon_all(&mut self, li: usize) {
        for si in 0..self.lobbies.lobbies[li].seats.len() {
            self.lobby_summon(li, si);
        }
    }

    /// `KickPlayer` (+ `ClearUserSocket`).
    fn lobby_kick(&mut self, li: usize, si: usize) {
        self.lobby_return(li, si);
        self.lobbies.lobbies[li].seats.remove(si);
    }

    /// `CancelLobby`: fees back, everyone home, `Closed` (the scenario closes the map).
    pub(crate) fn lobby_cancel(&mut self, li: usize) {
        let l = &mut self.lobbies.lobbies[li];
        l.canceled = true;
        let fee = l.fee as u64;
        if fee > 0 {
            let ids: Vec<u32> = l.seats.iter().filter_map(|s| s.id).collect();
            for id in ids {
                if self.lobbies.lobbies[li].pot >= fee && self.player(id).is_some() {
                    self.lobbies.lobbies[li].pot -= fee;
                    self.f6_give_gold(id, fee);
                }
            }
        }
        self.lobby_return_all(li);
        self.lobby_set_state(li, LobbyState::Closed);
        self.lobbies.lobbies[li].seats.clear();
    }

    /// `UpdateLobbyState` (+ the scenario's).
    fn lobby_set_state(&mut self, li: usize, state: LobbyState) {
        let now = self.now();
        self.lobbies.lobbies[li].state = state;
        if let Some(sc) = &mut self.lobbies.lobbies[li].scenario {
            match state {
                LobbyState::InProgress => {
                    let a = sc.arena_mut();
                    a.countdown = 0;
                    a.next_count = now.after_ms(1000);
                }
                LobbyState::Closed => self.arena_match_completed(li),
                _ => {}
            }
        }
    }

    /// `StartLobby` (+ `SortTeams`).
    fn lobby_start(&mut self, li: usize, gm: Option<u32>) {
        let l = &self.lobbies.lobbies[li];
        if l.state >= LobbyState::InProgress {
            if let Some(g) = gm {
                self.console(g, "El evento ya fue iniciado.", Tone::Warn);
            }
            return;
        }
        if l.team_size > 1 && !l.premade {
            self.lobby_sort_teams(li);
        }
        self.lobby_set_state(li, LobbyState::InProgress);
        if let Some(g) = gm {
            self.console(g, "Evento iniciado", Tone::Good);
        }
    }

    /// `SortTeams`: the last ones that cannot fill a team go home; the rest are dealt
    /// into teams by level, back and forth.
    fn lobby_sort_teams(&mut self, li: usize) {
        let size = self.lobbies.lobbies[li].team_size as usize;
        let keep = rules::players_for_teams(self.lobbies.lobbies[li].seats.len(), size);
        while self.lobbies.lobbies[li].seats.len() > keep {
            let si = self.lobbies.lobbies[li].seats.len() - 1;
            if let Some(id) = self.lobbies.lobbies[li].seats[si].id {
                self.console(id, "No hay suficientes jugadores para armar otro equipo y fuiste removido por ser el ultimo en entrar.", Tone::Warn);
                // The fee goes back too (the VB kept it).
                let fee = self.lobbies.lobbies[li].fee as u64;
                if fee > 0 && self.lobbies.lobbies[li].pot >= fee {
                    self.lobbies.lobbies[li].pot -= fee;
                    self.f6_give_gold(id, fee);
                }
            }
            self.lobby_kick(li, si);
        }
        let mut order: Vec<(usize, u32)> = self.lobbies.lobbies[li].seats.iter().enumerate().map(|(i, s)| (i, s.id.and_then(|id| self.player(id)).map_or(0, |p| p.level))).collect();
        order.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
        let teams = rules::snake_teams(order.len(), keep / size.max(1));
        for ((si, _), t) in order.into_iter().zip(teams) {
            self.lobbies.lobbies[li].seats[si].team = t;
        }
    }

    // ================================================================== tick

    /// Every world tick; acts once per second.
    pub(crate) fn tick_lobbies(&mut self, now: GameTime) {
        if now < self.lobbies.next_tick {
            return;
        }
        self.lobbies.next_tick = now.after_ms(1000);
        let mut li = 0;
        while li < self.lobbies.lobbies.len() {
            match self.lobbies.lobbies[li].state {
                LobbyState::AcceptingPlayers => self.lobby_waiting(li, now),
                LobbyState::InProgress | LobbyState::Completed => self.arena_tick(li, now),
                _ => {}
            }
            let l = &self.lobbies.lobbies[li];
            let done = match &l.scenario {
                Some(s) => s.arena().closed,
                None => l.state == LobbyState::Closed,
            };
            if done {
                let id = l.id;
                self.lobbies.lobbies.remove(li);
                if self.lobbies.global == Some(id) {
                    self.lobbies.global = None;
                }
            } else {
                li += 1;
            }
        }
    }

    /// `UpdateWaitingForPlayers`.
    fn lobby_waiting(&mut self, li: usize, now: GameTime) {
        let l = &self.lobbies.lobbies[li];
        if l.public && now >= l.next_reminder {
            let left = WAITING_MS.saturating_sub(now.since(l.opened_at)) / 1000;
            let txt1 = format!("Esperando jugadores.. La partida iniciará en {} minutos y {} segundos.", left / 60, left % 60);
            let txt2 = format!("De momento, hay {} de {} jugadores inscriptos. Se requiere un minimo de {} para que el evento inicie.", l.seats.len(), l.max_players, l.min_players);
            let ids: Vec<u32> = l.seats.iter().filter_map(|s| s.id).collect();
            self.lobbies.lobbies[li].next_reminder = now.after_ms(REMINDER_MS);
            for id in ids {
                self.console(id, txt1.clone(), Tone::Info);
                self.console(id, txt2.clone(), Tone::Info);
            }
        }
        let l = &self.lobbies.lobbies[li];
        if now.since(l.opened_at) > WAITING_MS {
            if l.seats.len() as u32 >= l.min_players {
                self.lobby_start(li, None);
            } else {
                let ids: Vec<u32> = l.seats.iter().filter_map(|s| s.id).collect();
                for id in ids {
                    self.console(id, "Evento cancelado por falta de jugadores.", Tone::Warn);
                }
                self.lobby_cancel(li);
            }
        } else if l.seats.len() as u32 >= l.max_players {
            self.f6_say_all("Evento> ¡Los cupos están completos! El evento comenzará en instantes.");
            self.lobby_start(li, None);
        }
    }

    // ================================================================== arena (DM / hunt)

    fn seat_ids(&self, li: usize) -> Vec<(i64, u32)> {
        self.lobbies.lobbies[li].seats.iter().filter_map(|s| s.id.filter(|i| self.player(*i).is_some()).map(|i| (s.key, i))).collect()
    }

    fn arena_tick(&mut self, li: usize, now: GameTime) {
        let Some(sc) = &self.lobbies.lobbies[li].scenario else { return };
        let dm = matches!(sc, Scenario::DeathMatch(_));
        let a = sc.arena();
        let map = a.map;
        let state = self.lobbies.lobbies[li].state;
        if state == LobbyState::InProgress {
            if !a.started {
                if now >= a.next_count {
                    let a = self.lobbies.lobbies[li].scenario.as_mut().unwrap().arena_mut();
                    a.countdown += 1;
                    a.next_count = now.after_ms(1000);
                    let c = a.countdown;
                    if c >= 10 {
                        if dm {
                            self.dm_start(li, now);
                        } else {
                            self.hunt_start(li, now);
                        }
                    } else if dm {
                        self.f6_say_map(map, format!("La matanza iniciará en {} segundos.", 10 - c));
                    } else {
                        self.f6_say_map(map, format!("La cacería iniciará en {}", 10 - c));
                    }
                }
                return;
            }
            self.arena_respawns(li, now);
            if dm {
                self.dm_respawn_items(li, now);
            }
            let a = self.lobbies.lobbies[li].scenario.as_ref().unwrap().arena();
            let elapsed = now.since(a.started_at);
            if elapsed > a.max_ms {
                self.f6_say_map(map, "¡Se acabó el tiempo!");
                self.arena_match_completed(li);
                return;
            }
            if now >= a.next_time_notice {
                let left = (a.max_ms - elapsed) / 1000;
                let a = self.lobbies.lobbies[li].scenario.as_mut().unwrap().arena_mut();
                a.next_time_notice = now.after_ms(if left <= 11 { 1000 } else { 60_000 });
                // BroadcastPendingTime: from half the time on, minutes, then the last 10 s.
                if left * 2000 <= a.max_ms {
                    if left >= 60 {
                        self.f6_say_map(map, format!("Quedan {} minutos.", left / 60));
                    } else if left <= 10 {
                        self.f6_say_map(map, left.to_string());
                    }
                }
            }
            let a = self.lobbies.lobbies[li].scenario.as_ref().unwrap().arena();
            if now >= a.next_score {
                let every = if dm { 20_000 } else { 30_000 };
                self.lobbies.lobbies[li].scenario.as_mut().unwrap().arena_mut().next_score = now.after_ms(every);
                self.arena_publish(li, false);
            }
        } else if state == LobbyState::Completed {
            let a = self.lobbies.lobbies[li].scenario.as_mut().unwrap().arena_mut();
            if a.countdown < 10 && now >= a.next_count {
                a.countdown += 1;
                a.next_count = now.after_ms(1000);
                let c = a.countdown;
                if c >= 10 {
                    self.arena_close(li);
                } else {
                    self.f6_say_map(map, format!("El mapa se cerrará en {} segundos.", 10 - c));
                }
            }
        }
    }

    /// `StartGame` (deathmatch): ring, everybody into the area, first items, fight on.
    fn dm_start(&mut self, li: usize, now: GameTime) {
        let players = self.lobbies.lobbies[li].seats.len();
        let a = self.lobbies.lobbies[li].scenario.as_mut().unwrap().arena_mut();
        a.started = true;
        a.started_at = now;
        a.next_time_notice = now.after_ms(60_000);
        a.next_score = now.after_ms(20_000);
        let half = rules::arena_half_side(a.min_arena, players, a.area_tenths).clamp(1, 46);
        a.area = (50 - half, 50 - half, 50 + half, 50 + half);
        for it in &mut a.items {
            it.next = now.after_ms(it.every_ms);
        }
        let (map, area, ring, items) = (a.map, a.area, a.ring, a.items.clone());
        self.f6_say_map(map, "¡Que inicie la masacre!");
        self.dm_ring(li, map, area, ring);
        for (_, id) in self.seat_ids(li) {
            if self.player(id).is_some_and(|p| p.map == map) {
                let (x, y) = (self.rng.range(area.0, area.2), self.rng.range(area.1, area.3));
                self.f6_warp(id, map, x, y);
            }
        }
        for it in items {
            for _ in 0..=rules::stacks_for(it.initial_tenths, players) {
                self.dm_spawn_item(map, area, it.obj, it.amount);
            }
        }
        // ConfigMapEvent: fighting allowed.
        self.maps.get_mut(&map).unwrap().map.safe = false;
    }

    /// `LimitFightArea`: posts around the area, their tiles blocked.
    fn dm_ring(&mut self, li: usize, map: u16, (x1, y1, x2, y2): (i32, i32, i32, i32), ring: [u16; 6]) {
        let mut posts: Vec<(i32, i32, u16)> = Vec::new();
        for x in x1..=x2 {
            posts.push((x, y1 - 1, ring[0]));
            posts.push((x, y2 + 1, ring[0]));
        }
        for y in y1..=y2 {
            posts.push((x1 - 1, y, ring[1]));
            posts.push((x2 + 1, y, ring[1]));
        }
        posts.extend([(x1 - 1, y1 - 1, ring[2]), (x2 + 1, y1 - 1, ring[3]), (x1 - 1, y2 + 1, ring[4]), (x2 + 1, y2 + 1, ring[5])]);
        let mut saved = Vec::new();
        for (x, y, obj) in posts {
            if !(1..=100).contains(&x) || !(1..=100).contains(&y) {
                continue;
            }
            if obj > 0 && self.maps[&map].ground.at(x, y).is_none() {
                self.make_obj(map, x, y, InvItem { obj, amount: 1 });
            }
            let i = ao_formats::csm::tile(x, y);
            let m = &mut self.maps.get_mut(&map).unwrap().map;
            saved.push((x, y, m.blocked[i]));
            m.blocked[i] |= ao_formats::csm::BLOCK_ALL;
        }
        self.lobbies.lobbies[li].scenario.as_mut().unwrap().arena_mut().ring_tiles = saved;
    }

    fn dm_spawn_item(&mut self, map: u16, (x1, y1, x2, y2): (i32, i32, i32, i32), obj: u16, amount: u32) {
        if obj == 0 || amount == 0 {
            return;
        }
        let (x, y) = (self.rng.range(x1, x2), self.rng.range(y1, y2));
        self.drop_to_floor(map, x, y, InvItem { obj, amount }, false);
    }

    /// `RespawnItems` (each item with its own stack count; the VB used item 0's).
    fn dm_respawn_items(&mut self, li: usize, now: GameTime) {
        let players = self.lobbies.lobbies[li].seats.len();
        let a = self.lobbies.lobbies[li].scenario.as_mut().unwrap().arena_mut();
        let (map, area) = (a.map, a.area);
        let mut due = Vec::new();
        for it in &mut a.items {
            if now >= it.next {
                it.next = now.after_ms(it.every_ms);
                due.push((it.obj, it.amount, rules::stacks_for(it.stack_tenths, players)));
            }
        }
        for (obj, amount, stacks) in due {
            for _ in 0..stacks {
                self.dm_spawn_item(map, area, obj, amount);
            }
        }
    }

    /// `WillSummonPlayer` (deathmatch): forbidden objects (red and blue potions...) are
    /// kept aside while playing.
    fn dm_take_forbidden(&mut self, li: usize, si: usize, id: u32) {
        let key = self.lobbies.lobbies[li].seats[si].key;
        let Some(Scenario::DeathMatch(a)) = &self.lobbies.lobbies[li].scenario else { return };
        if a.removed.contains_key(&key) {
            return;
        }
        let forbidden = a.forbidden.clone();
        let slots: Vec<(u8, InvItem)> = self.player(id).map(|p| (1..=p.inv.slots.len() as u8).filter_map(|s| p.inv.get(s).map(|i| (s, i))).filter(|(_, i)| forbidden.contains(&i.obj)).collect()).unwrap_or_default();
        if slots.is_empty() {
            return;
        }
        let mut taken = Vec::new();
        for (slot, item) in slots {
            self.remove_from_slot(id, slot, item.amount);
            taken.push(item);
        }
        if let Some(Scenario::DeathMatch(a)) = &mut self.lobbies.lobbies[li].scenario {
            a.removed.insert(key, taken);
        }
    }

    /// `RemoveForbiddenItems`: what was picked up in the arena goes, what was kept aside
    /// comes back.
    fn dm_give_back(&mut self, li: usize, key: i64, id: u32) {
        let Some(Scenario::DeathMatch(a)) = &mut self.lobbies.lobbies[li].scenario else { return };
        let forbidden = a.forbidden.clone();
        let back = a.removed.remove(&key).unwrap_or_default();
        let slots: Vec<(u8, u32)> = self.player(id).map(|p| (1..=p.inv.slots.len() as u8).filter_map(|s| p.inv.get(s).map(|i| (s, i))).filter(|(_, i)| forbidden.contains(&i.obj)).map(|(s, i)| (s, i.amount)).collect()).unwrap_or_default();
        for (slot, n) in slots {
            self.remove_from_slot(id, slot, n);
        }
        for item in back {
            if !self.add_item(id, item) {
                let (m, x, y) = self.player(id).map(|p| (p.map, p.x, p.y)).unwrap();
                self.drop_to_floor(m, x, y, item, false);
            }
        }
    }

    /// `StartGame` (hunt): everybody to a random tile, the creatures appear.
    fn hunt_start(&mut self, li: usize, now: GameTime) {
        let a = self.lobbies.lobbies[li].scenario.as_mut().unwrap().arena_mut();
        a.started = true;
        a.started_at = now;
        a.next_time_notice = now.after_ms(60_000);
        a.next_score = now.after_ms(30_000);
        let (map, targets) = (a.map, a.targets.clone());
        for (_, id) in self.seat_ids(li) {
            if self.player(id).is_some_and(|p| p.map == map) {
                let (x, y) = (self.rng.range(12, 85), self.rng.range(12, 85));
                self.f6_warp(id, map, x, y);
            }
        }
        for t in targets {
            for _ in 0..t.count {
                self.hunt_spawn(li, t.npc);
            }
        }
        self.f6_say_map(map, "¡Que inicie la cacería!");
    }

    fn hunt_spawn(&mut self, li: usize, npc: u16) {
        let map = self.lobbies.lobbies[li].scenario.as_ref().unwrap().arena().map;
        let (x, y) = (self.rng.range(12, 85), self.rng.range(12, 85));
        match self.f6_spawn_npc(map, npc, x, y, true) {
            Some(nid) => self.lobbies.lobbies[li].scenario.as_mut().unwrap().arena_mut().spawned.push(nid),
            None => warn_admins(self, format!("No se pudo spawnear el npc {npc}, revisar.")),
        }
    }

    /// HOOK (combat::kill_npc): a creature died. `melee` = killed by a weapon blow.
    pub(crate) fn lobby_on_npc_killed(&mut self, map: u16, nid: u32, number: u16, killer: u32, melee: bool) {
        let Some(li) = self.scenario_on(map) else { return };
        let Some(Scenario::Hunt(a)) = &mut self.lobbies.lobbies[li].scenario else { return };
        let Some(pos) = a.spawned.iter().position(|n| *n == nid) else { return };
        a.spawned.remove(pos);
        let score = a.targets.iter().find(|t| t.npc == number).map(|t| t.score);
        let running = self.lobbies.lobbies[li].state == LobbyState::InProgress;
        if running {
            if let Some(score) = score {
                let weapon = self.player(killer).and_then(|p| p.inv.equipped.weapon.and_then(|s| p.inv.get(s))).map(|i| i.obj);
                if melee && weapon == Some(HUNT_WEAPON) {
                    if let Some(key) = self.lobby_seat_of(killer).filter(|(l, _)| *l == li).map(|(_, s)| self.lobbies.lobbies[li].seats[s].key) {
                        let a = self.lobbies.lobbies[li].scenario.as_mut().unwrap().arena_mut();
                        let total = a.board.update(key, score);
                        if total >= a.target_score {
                            self.f6_say_map(map, "¡Tenemos un ganador!");
                            self.arena_match_completed(li);
                            return;
                        }
                    }
                } else if self.player(killer).is_some() {
                    self.console(killer, "Debes golpear con Garrote para sumar puntos.", Tone::Warn);
                }
            }
            self.hunt_spawn(li, number);
        }
    }

    /// HOOK (combat::hurt_player): a player hit another (assists of the deathmatch).
    pub(crate) fn lobby_on_player_hit(&mut self, attacker: u32, victim: u32) {
        let (Some((la, sa)), Some((lv, sv))) = (self.lobby_seat_of(attacker), self.lobby_seat_of(victim)) else { return };
        if la != lv {
            return;
        }
        let now = self.now();
        let (ka, kv) = (self.lobbies.lobbies[la].seats[sa].key, self.lobbies.lobbies[lv].seats[sv].key);
        if let Some(Scenario::DeathMatch(a)) = &mut self.lobbies.lobbies[la].scenario {
            let prev = a.last_hit.get(&kv).copied();
            // The assist is the one who hit before the killer.
            if prev.is_none_or(|(k, _)| k != ka) {
                a.last_hit.insert(kv, (ka, now));
            }
        }
    }

    /// HOOK (combat): a player in a scenario died (`UserDie` + `PlayerKillPlayer`).
    pub(crate) fn lobby_on_player_killed(&mut self, killer: Option<u32>, victim: u32) {
        let Some((li, sv)) = self.lobby_seat_of(victim) else { return };
        if self.lobbies.lobbies[li].state != LobbyState::InProgress {
            return;
        }
        let Some(sc) = &self.lobbies.lobbies[li].scenario else { return };
        if self.player(victim).is_none_or(|p| p.map != sc.arena().map) {
            return;
        }
        let now = self.now();
        let kv = self.lobbies.lobbies[li].seats[sv].key;
        let killer_key = killer.and_then(|k| self.lobby_seat_of(k)).filter(|(l, _)| *l == li).map(|(_, s)| self.lobbies.lobbies[li].seats[s].key);
        let map = sc.arena().map;
        let sc = self.lobbies.lobbies[li].scenario.as_mut().unwrap();
        let dm = matches!(sc, Scenario::DeathMatch(_));
        let a = sc.arena_mut();
        let respawn = now.after_ms(a.respawn_ms);
        a.respawn.insert(kv, respawn);
        if !dm {
            return;
        }
        a.board.update(kv, -1);
        let mut winner = false;
        if let Some(kk) = killer_key.filter(|k| *k != kv) {
            winner |= a.board.update(kk, 3) >= a.target_score;
            if let Some((assist, at)) = a.last_hit.remove(&kv) {
                if assist != kk && now.since(at) < ASSIST_MS {
                    winner |= a.board.update(assist, 1) >= a.target_score;
                }
            }
        }
        if winner {
            self.f6_say_map(map, "¡Tenemos un ganador!");
            self.arena_match_completed(li);
        }
    }

    /// `RespawnPlayers`.
    fn arena_respawns(&mut self, li: usize, now: GameTime) {
        let a = self.lobbies.lobbies[li].scenario.as_ref().unwrap().arena();
        let due: Vec<i64> = a.respawn.iter().filter(|(_, t)| now >= **t).map(|(k, _)| *k).collect();
        let (map, dm, area) = (a.map, matches!(self.lobbies.lobbies[li].scenario, Some(Scenario::DeathMatch(_))), a.area);
        for key in due {
            self.lobbies.lobbies[li].scenario.as_mut().unwrap().arena_mut().respawn.remove(&key);
            let Some(id) = self.lobbies.lobbies[li].seats.iter().find(|s| s.key == key).and_then(|s| s.id) else { continue };
            if self.player(id).is_none_or(|p| p.map != map) {
                continue;
            }
            let (x, y) = if dm { (self.rng.range(area.0, area.2), self.rng.range(area.1, area.3)) } else { (self.rng.range(12, 85), self.rng.range(12, 85)) };
            self.f6_warp(id, map, x, y);
            self.resurrect(id);
            self.broadcast(map, &S2C::Fx { id, fx: RESPAWN_FX, loops: 1 }, None);
            let (px, py) = self.player(id).map(|p| (p.x, p.y)).unwrap();
            self.broadcast(map, &S2C::Sound { wav: RESPAWN_WAV, x: px, y: py }, None);
        }
    }

    /// `PublishScoreboard`.
    fn arena_publish(&mut self, li: usize, last: bool) {
        let l = &self.lobbies.lobbies[li];
        let Some(sc) = &l.scenario else { return };
        let a = sc.arena();
        let mut lines = vec!["Posiciones:".to_string()];
        if l.team_size > 1 {
            let teams: HashMap<i64, u32> = l.seats.iter().map(|s| (s.key, s.team)).collect();
            let count = (l.seats.len() as u32 / l.team_size).max(1);
            let rank = a.board.team_ranking(&teams, count);
            if last {
                if let Some((t, score)) = rank.first() {
                    lines.insert(0, format!("Ganador equipo {t}: {score}"));
                    let names: Vec<String> = l.seats.iter().filter(|s| s.team == *t).map(|s| format!("{} ({})", s.name, a.board.score(s.key))).collect();
                    lines.insert(1, names.join(" "));
                }
            }
            for (t, s) in rank {
                lines.push(format!("Equipo {t}: {s}"));
            }
        } else {
            for (i, (key, score)) in a.board.ranking(5).into_iter().enumerate() {
                let name = l.seats.iter().find(|s| s.key == key).map_or_else(String::new, |s| s.name.clone());
                lines.push(format!("{}) {name}: {score} puntos", i + 1));
            }
        }
        let map = a.map;
        for line in lines {
            self.f6_say_map(map, line);
        }
    }

    /// `MatchCompleted` / `MatchComplete`.
    fn arena_match_completed(&mut self, li: usize) {
        let now = self.now();
        let completed = self.lobbies.lobbies[li].state == LobbyState::Completed;
        let Some(sc) = &mut self.lobbies.lobbies[li].scenario else { return };
        if sc.arena().closed || completed {
            return;
        }
        let a = sc.arena_mut();
        a.countdown = 0;
        a.next_count = now.after_ms(1000);
        let (map, spawned) = (a.map, std::mem::take(&mut a.spawned));
        a.respawn.clear();
        self.maps.get_mut(&map).unwrap().map.safe = true;
        let seats: Vec<(i64, u32)> = self.seat_ids(li);
        // A cancelled lobby already gave everything back when sending everyone home.
        if matches!(self.lobbies.lobbies[li].scenario, Some(Scenario::DeathMatch(_))) && !self.lobbies.lobbies[li].canceled {
            for (key, id) in &seats {
                self.dm_give_back(li, *key, *id);
            }
        }
        self.lobbies.lobbies[li].state = LobbyState::Completed;
        if !self.lobbies.lobbies[li].canceled {
            self.arena_publish(li, true);
            self.arena_rewards(li);
        }
        for nid in spawned {
            self.f6_remove_npc(map, nid);
        }
        // CustomScenarios.ClearMap: what lies on the floor goes.
        let tiles: Vec<((i32, i32), u32)> = self.maps[&map].ground.tiles.iter().map(|(k, v)| (*k, v.amount)).collect();
        let ring: Vec<(i32, i32)> = self.lobbies.lobbies[li].scenario.as_ref().unwrap().arena().ring_tiles.iter().map(|(x, y, _)| (*x, *y)).collect();
        for ((x, y), n) in tiles {
            if !ring.contains(&(x, y)) {
                self.erase_obj(map, x, y, n);
            }
        }
    }

    /// `GiveRewards`.
    fn arena_rewards(&mut self, li: usize) {
        let l = &self.lobbies.lobbies[li];
        let Some(sc) = &l.scenario else { return };
        let name = sc.name();
        let a = sc.arena();
        let team_size = l.team_size.max(1) as usize;
        let groups = l.seats.len() / team_size;
        let prizes = rules::prize_split(l.pot, groups);
        let mut lines = vec![format!("{name} finalizado, ganadores:")];
        let mut pay: Vec<(u32, u64)> = Vec::new();
        if team_size > 1 {
            let teams: HashMap<i64, u32> = l.seats.iter().map(|s| (s.key, s.team)).collect();
            let rank = a.board.team_ranking(&teams, groups.max(1) as u32);
            for (i, (t, score)) in rank.into_iter().take(3).enumerate() {
                let each = prizes[i] / team_size as u64;
                let names: Vec<String> = l.seats.iter().filter(|s| s.team == t).map(|s| s.name.clone()).collect();
                for s in l.seats.iter().filter(|s| s.team == t) {
                    if let Some(id) = s.id {
                        pay.push((id, each));
                    }
                }
                let reward = if l.pot > 0 { format!(", ganaron {each} Modenas de oro cada uno.") } else { String::new() };
                lines.push(format!("{}) {}: {score} puntos{reward}", i + 1, names.join(" ")));
            }
        } else {
            for (i, (key, score)) in a.board.ranking(3).into_iter().enumerate() {
                let seat = l.seats.iter().find(|s| s.key == key);
                let n = seat.map_or_else(String::new, |s| s.name.clone());
                if let Some(id) = seat.and_then(|s| s.id) {
                    pay.push((id, prizes[i]));
                }
                let reward = if l.pot > 0 { format!(", gano {} Modenas de oro.", prizes[i]) } else { String::new() };
                lines.push(format!("{}) {n}: {score} puntos{reward}", i + 1));
            }
        }
        for line in lines {
            self.f6_say_all(line);
        }
        for (id, gold) in pay {
            if gold > 0 && self.player(id).is_some() && self.lobbies.lobbies[li].pot >= gold {
                self.lobbies.lobbies[li].pot -= gold;
                self.f6_give_gold(id, gold);
            }
        }
    }

    /// `CloseScenario` + `ClearMap`: home, ring down, map flags back.
    fn arena_close(&mut self, li: usize) {
        if self.lobbies.lobbies[li].scenario.as_ref().is_none_or(|s| s.arena().closed) {
            return;
        }
        self.lobby_return_all(li);
        let a = self.lobbies.lobbies[li].scenario.as_mut().unwrap().arena_mut();
        a.closed = true;
        let (map, safe, ring) = (a.map, a.saved_safe, std::mem::take(&mut a.ring_tiles));
        for (x, y, bits) in ring {
            if let Some(g) = self.maps[&map].ground.at(x, y) {
                self.erase_obj(map, x, y, g.amount);
            }
            self.maps.get_mut(&map).unwrap().map.blocked[ao_formats::csm::tile(x, y)] = bits;
        }
        self.maps.get_mut(&map).unwrap().map.safe = safe;
        self.lobbies.lobbies[li].state = LobbyState::Uninitialized;
    }

    // ================================================================== connections

    /// HOOK (event_on_leave): `RegisterDisconnectedUser`: a summoned player goes home now
    /// (so they are saved outside) and comes back on reconnecting.
    pub(crate) fn lobby_on_leave(&mut self, id: u32) {
        let Some((li, si)) = self.lobby_seat_of(id) else { return };
        let summoned = self.lobbies.lobbies[li].seats[si].summoned;
        self.lobby_return(li, si);
        let s = &mut self.lobbies.lobbies[li].seats[si];
        s.id = None;
        s.return_on_reconnect = summoned;
    }

    /// HOOK (event_on_enter): `RegisterReconnectedUser`.
    pub(crate) fn lobby_on_enter(&mut self, id: u32) {
        let Some((li, si)) = self.lobby_seat_of(id) else { return };
        let s = &mut self.lobbies.lobbies[li].seats[si];
        s.id = Some(id);
        if std::mem::take(&mut s.return_on_reconnect) && self.lobbies.lobbies[li].state < LobbyState::Completed {
            self.lobby_summon(li, si);
        }
    }

    // ================================================================== commands

    /// `/SALAS` and the EventMaster NPC: the open public rooms (`WriteUpdateLobbyList`).
    pub(crate) fn lobby_list(&self, id: u32) {
        let open: Vec<&Lobby> = self.lobbies.lobbies.iter().filter(|l| l.state == LobbyState::AcceptingPlayers && l.public).collect();
        if open.is_empty() && self.capture_open().is_none() {
            return self.console(id, "No hay eventos abiertos.", Tone::Info);
        }
        for l in open {
            let kind = l.scenario.as_ref().map_or("Evento", |s| s.name());
            let lock = if l.password.is_empty() { "" } else { " (con contraseña)" };
            let teams = if l.team_size > 1 { format!(", equipos de {}{}", l.team_size, if l.premade { " armados" } else { " al azar" }) } else { String::new() };
            self.console(
                id,
                format!("Sala {}: {} {} - niveles {}-{}, {}/{} inscriptos (mínimo {}), inscripción {} oro{teams}{lock}. /PARTICIPAR {}", l.id, kind, l.description, l.min_level, l.max_level, l.seats.len(), l.max_players, l.min_players, l.fee, l.id),
                Tone::Info,
            );
        }
        if let Some(text) = self.capture_open() {
            self.console(id, text, Tone::Info);
        }
    }

    /// HOOK (interact): clicking an EventMaster.
    pub(crate) fn lobby_event_master(&mut self, id: u32, kind: NpcType) -> bool {
        if kind != NpcType::EventMaster {
            return false;
        }
        if self.is_dead(id) {
            self.console(id, "Estas muerto!!", Tone::Warn);
        } else {
            self.lobby_list(id);
        }
        true
    }

    /// `/CREAREVENTO LOBBY|CACERIA|DEATHMATCH|NAVALCONQUEST participantes min max [precio]
    /// [equipo]` (`CreateEventCmd` + `HandleStartEvent`).
    pub(crate) fn lobby_gm_create(&mut self, id: u32, me: &str, kind: EventType, a: &[String]) {
        let n = |i: usize| a.get(i).and_then(|s| s.parse::<u32>().ok());
        let (Some(max), Some(min), Some(maxl)) = (n(1), n(2), n(3)) else {
            return self.console(id, format!("Valor incorrecto. Utilice /CREAREVENTO {} PARTICIPANTES NIVEL_MINIMO NIVEL_MAXIMO [PRECIO] [EQUIPO].", a.first().map_or("", |s| s.as_str())), Tone::Info);
        };
        let s = LobbySettings { kind: Some(kind), min_level: min, max_level: maxl, min_players: 1, max_players: max, team_size: n(5).unwrap_or(1), premade: false, fee: n(4).unwrap_or(0), ..Default::default() };
        match self.lobby_create_public(s) {
            Ok(l) => {
                self.log_gm(me, "crear evento", "", &format!("{kind:?} sala {l} {max} {min}-{maxl}"));
                self.console(id, format!("Sala {l} creada."), Tone::Good);
            }
            Err(e) => self.console(id, e, Tone::Warn),
        }
    }

    /// `/CANCELAREVENTO` (`HandleCancelarEvento`).
    pub(crate) fn lobby_gm_cancel(&mut self, id: u32, me: &str) {
        if self.capture.is_some() {
            self.capture_finish(None);
            self.log_gm(me, "cancelar evento", "", "captura");
            return;
        }
        match self.lobbies.global.and_then(|g| self.lobby_idx(g)) {
            Some(li) if self.lobbies.lobbies[li].state < LobbyState::Completed => {
                self.lobby_cancel(li);
                self.f6_say_all("Eventos» El evento ha sido cancelado.");
                self.log_gm(me, "cancelar evento", "", "");
            }
            _ => self.console(id, "No se encontró ningún evento activo.", Tone::Info),
        }
    }

    /// `/CONFIGLOBBY sub [args]` (`HandleLobbyCommand` -> `HandleRemoteLobbyCommand`).
    pub(crate) fn lobby_gm_config(&mut self, id: u32, me: &str, a: &[String]) {
        let Some(li) = self.lobbies.global.and_then(|g| self.lobby_idx(g)) else { return self.console(id, "Servidor >> No se pudo procesar el comando.", Tone::Warn) };
        let sub = a.first().map(|s| s.to_uppercase()).unwrap_or_default();
        let arg = a.get(1).cloned().unwrap_or_default();
        let num = arg.parse::<u32>().ok();
        self.log_gm(me, "configlobby", "", &a.join(" "));
        let state = self.lobbies.lobbies[li].state;
        match sub.as_str() {
            "SPAWN" => {
                let pos = self.player(id).map(|p| (p.map, p.x, p.y));
                self.lobbies.lobbies[li].spawn = pos;
                self.console(id, "Posición de invocación guardada.", Tone::Good);
            }
            "MAXLVL" if num.is_some() => self.lobbies.lobbies[li].max_level = num.unwrap(),
            "MINLVL" if num.is_some() => self.lobbies.lobbies[li].min_level = num.unwrap(),
            "CLASS" => match class_id(&arg) {
                Some(c) => self.lobbies.lobbies[li].class_filter = Some(c),
                None => self.console(id, "Clase inválida.", Tone::Warn),
            },
            "SUMPLAYER" | "RETURNPLAYER" | "KICK" if num.is_some_and(|n| (n as usize) < self.lobbies.lobbies[li].seats.len()) => {
                let si = num.unwrap() as usize;
                match sub.as_str() {
                    "SUMPLAYER" => self.lobby_summon(li, si),
                    "RETURNPLAYER" => self.lobby_return(li, si),
                    _ => self.lobby_kick(li, si),
                }
            }
            "SUMALL" => self.lobby_summon_all(li),
            "RETALL" => self.lobby_return_all(li),
            "OPEN" => {
                let public = !arg.eq_ignore_ascii_case("PRIVATE");
                let l = self.lobbies.lobbies[li].id;
                self.lobby_open(l, public);
            }
            "START" => self.lobby_start(li, Some(id)),
            "END" => self.lobby_cancel(li),
            "LIST" => {
                let seats = self.lobbies.lobbies[li].seats.clone();
                for (i, s) in seats.iter().enumerate() {
                    let who = if s.id.is_some() { s.name.clone() } else { "Disconnected player.".into() };
                    self.console(id, format!("{i}) {who}"), Tone::Info);
                }
            }
            "FORCERESET" => {
                let l = &mut self.lobbies.lobbies[li];
                if l.scenario.is_none() {
                    l.state = LobbyState::Closed;
                    l.seats.clear();
                } else {
                    self.lobby_cancel(li);
                }
                self.console(id, "Reset done.", Tone::Info);
            }
            "SETTEAMSIZE" if num.is_some() => {
                let size = num.unwrap().max(1);
                let l = &mut self.lobbies.lobbies[li];
                if l.max_players % size != 0 {
                    self.console(id, "La cantidad maxima de jugadores no es divisible por la cantidad de grupos.", Tone::Warn);
                } else if !(state == LobbyState::Initialized || (state == LobbyState::AcceptingPlayers && l.seats.is_empty())) {
                    self.console(id, "No puede cambiar la cantidad de equipos una vez abierta la inscripcion.", Tone::Warn);
                } else {
                    l.team_size = size;
                    l.premade = a.get(2).is_none_or(|p| p != "0");
                    self.console(id, "Equipos configurados con exito", Tone::Good);
                }
            }
            "ADDPLAYER" if a.len() >= 2 => {
                let name = a[1..].join(" ");
                let Some(t) = self.id_by_name(&name) else { return self.console(id, format!("User {name} not found."), Tone::Warn) };
                match self.lobby_add(li, t, 0) {
                    Ok(()) => {
                        self.console(id, "Player has been registered", Tone::Good);
                        self.console(t, "Has Ingresado al evento.", Tone::Good);
                    }
                    Err(why) => self.console(id, format!("Failed to add player with message: {why}"), Tone::Warn),
                }
            }
            "SETPRICE" if num.is_some() => {
                if state != LobbyState::Initialized && !(state == LobbyState::AcceptingPlayers && self.lobbies.lobbies[li].seats.is_empty()) {
                    return self.console(id, "Failed to update insription price", Tone::Warn);
                }
                self.lobbies.lobbies[li].fee = num.unwrap();
                self.console(id, "Inscription Price updated", Tone::Good);
            }
            _ => self.console(id, "Uso: /CONFIGLOBBY SPAWN|MAXLVL n|MINLVL n|CLASS clase|SUMPLAYER n|SUMALL|RETURNPLAYER n|RETALL|OPEN [PRIVATE]|START|END|LIST|KICK n|FORCERESET|SETTEAMSIZE n [0=al azar]|ADDPLAYER nick|SETPRICE oro", Tone::Info),
        }
    }
}

fn warn_admins(w: &World, text: String) {
    for id in w.player_ids() {
        if w.gm_rank(id) >= GmRank::Dios {
            w.console(id, text.clone(), Tone::Warn);
        }
    }
}

//! Phase 6 pure rules: lobbies and scenarios (`ModLobby.bas`, `ScoreBoard.cls`,
//! `ScenarioDeathMatch.cls`, `ScenarioHuntNpc.cls`), hourly events (`ModEventos.bas`),
//! global quests (`ModGlobalQuests.bas`), the emperor castle (`ModCastle.bas`), capture the
//! flag (`clsCaptura.cls`) and invasions (`ModInvasion.bas`). No I/O: the server systems
//! (`systems/lobby.rs`, `world_events.rs`, `castle.rs`, `capture.rs`) apply them.
use std::collections::HashMap;

// ------------------------------------------------------------------ calendar

/// Days since 1970-01-01 of a civil date (proleptic Gregorian).
pub fn days_from_civil(y: i64, m: u32, d: u32) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let m = m as i64;
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + d as i64 - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// Local civil time of a unix second with a fixed offset in hours.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Civil {
    pub year: i64,
    pub month: u32,
    pub day: u32,
    pub hour: u32,
    pub minute: u32,
    /// VB `Weekday`: Sunday = 1 ... Saturday = 7.
    pub weekday: u32,
}

pub fn civil(unix: i64, offset_hours: i32) -> Civil {
    let t = unix + offset_hours as i64 * 3600;
    let days = t.div_euclid(86_400);
    let secs = t.rem_euclid(86_400);
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let month = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let year = yoe + era * 400 + if month <= 2 { 1 } else { 0 };
    // 1970-01-01 was a Thursday (VB 5).
    let weekday = ((days + 4).rem_euclid(7) + 1) as u32;
    Civil { year, month, day, hour: (secs / 3600) as u32, minute: (secs / 60 % 60) as u32, weekday }
}

/// `CDate("2026-10-5 23:00:00")` in local time -> unix seconds. Accepts one-digit month,
/// day and hour and an optional time.
pub fn parse_datetime(s: &str, offset_hours: i32) -> Option<i64> {
    let s = s.trim();
    let (date, time) = s.split_once(' ').unwrap_or((s, "0:00:00"));
    let mut d = date.split('-').map(|p| p.trim().parse::<i64>().ok());
    let (y, m, dd) = (d.next()??, d.next()??, d.next()??);
    if !(1..=12).contains(&m) || !(1..=31).contains(&dd) {
        return None;
    }
    let mut t = time.trim().split(':').map(|p| p.trim().parse::<i64>().ok());
    let h = t.next().flatten().unwrap_or(0);
    let mi = t.next().flatten().unwrap_or(0);
    let se = t.next().flatten().unwrap_or(0);
    Some(days_from_civil(y, m as u32, dd as u32) * 86_400 + h * 3600 + mi * 60 + se - offset_hours as i64 * 3600)
}

// ------------------------------------------------------------------ lobby

/// `e_EventType`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EventType {
    Generic = 0,
    CaptureTheFlag = 1,
    NpcHunt = 2,
    DeathMatch = 3,
    NavalBattle = 4,
}

impl EventType {
    pub fn from_id(v: i64) -> Option<EventType> {
        Some(match v {
            0 => EventType::Generic,
            1 => EventType::CaptureTheFlag,
            2 => EventType::NpcHunt,
            3 => EventType::DeathMatch,
            4 => EventType::NavalBattle,
            _ => return None,
        })
    }

    /// The `/CREAREVENTO` keyword of the client (`CreateEventCmd`).
    pub fn from_keyword(s: &str) -> Option<EventType> {
        Some(match s.to_uppercase().as_str() {
            "LOBBY" => EventType::Generic,
            "CAPTURA" => EventType::CaptureTheFlag,
            "CACERIA" => EventType::NpcHunt,
            "DEATHMATCH" => EventType::DeathMatch,
            "NAVALCONQUEST" => EventType::NavalBattle,
            _ => return None,
        })
    }
}

/// `MaxLevel` accepted by `ValidateLobbySettings`.
pub const LOBBY_MAX_LEVEL: u32 = 47;

/// `ValidateLobbySettings`: the official locale message on a bad range.
pub fn validate_levels(min: u32, max: u32) -> Result<(), &'static str> {
    if min < 1 || max > LOBBY_MAX_LEVEL {
        return Err("Hay pocos jugadores en el servidor, intenta con una cantidad menor de participantes.");
    }
    if min > max {
        return Err("El nivel mínimo debe ser menor al máximo.");
    }
    Ok(())
}

/// `SortTeams` second half: assigns teams 1..=count to players already sorted by level
/// (highest first) in the "1 2 3 3 2 1 1 2 3" order, so teams stay balanced.
pub fn snake_teams(players: usize, team_count: usize) -> Vec<u32> {
    let mut out = Vec::with_capacity(players);
    if team_count == 0 {
        return vec![0; players];
    }
    let (mut team, mut dir) = (1i64, 1i64);
    for _ in 0..players {
        out.push(team as u32);
        team += dir;
        if team > team_count as i64 {
            dir = -1;
            team = team_count as i64;
        } else if team < 1 {
            dir = 1;
            team = 1;
        }
    }
    out
}

/// `SortTeams` first half: how many of the registered players can make full teams. The
/// last ones to register beyond that are kicked (`MsgNotEnoughPlayerForTeam`).
pub fn players_for_teams(registered: usize, team_size: usize) -> usize {
    if team_size == 0 {
        return registered;
    }
    registered / team_size * team_size
}

/// `GiveRewards`: the share of the collected fees for places 1-3 (55/35 with two teams or
/// players, else 40/30/20; the house keeps the rest).
pub fn prize_split(pot: u64, groups: usize) -> [u64; 3] {
    if groups == 2 {
        [pot * 55 / 100, pot * 35 / 100, 0]
    } else {
        [pot * 40 / 100, pot * 30 / 100, pot * 20 / 100]
    }
}

/// `ScoreBoard.cls`: points per player (key = character id here, the VB used the user
/// slot, which a reconnection changes; `DisconnectUser`/`RestoreDisconnectUser` patched
/// that, a stable key does not need it).
#[derive(Debug, Default, Clone)]
pub struct ScoreBoard {
    scores: HashMap<i64, i32>,
    order: Vec<i64>,
}

impl ScoreBoard {
    pub fn add_player(&mut self, key: i64) {
        if let std::collections::hash_map::Entry::Vacant(e) = self.scores.entry(key) {
            e.insert(0);
            self.order.push(key);
        }
    }

    /// `UpdatePlayerScore`: never below 0. Unknown players are ignored (0).
    pub fn update(&mut self, key: i64, delta: i32) -> i32 {
        let Some(s) = self.scores.get_mut(&key) else { return 0 };
        *s = (*s + delta).max(0);
        *s
    }

    pub fn score(&self, key: i64) -> i32 {
        self.scores.get(&key).copied().unwrap_or(0)
    }

    /// `GetRanking`: the best `size`, highest first; ties keep the joining order.
    pub fn ranking(&self, size: usize) -> Vec<(i64, i32)> {
        let mut v: Vec<(usize, i64, i32)> = self.order.iter().enumerate().map(|(i, k)| (i, *k, self.scores[k])).collect();
        v.sort_by(|a, b| b.2.cmp(&a.2).then(a.0.cmp(&b.0)));
        v.into_iter().take(size).map(|(_, k, s)| (k, s)).collect()
    }

    /// `GetTeamRanking`: (team, summed score), best first, for teams 1..=count.
    pub fn team_ranking(&self, team_of: &HashMap<i64, u32>, count: u32) -> Vec<(u32, i32)> {
        let mut v: Vec<(u32, i32)> = (1..=count).map(|t| (t, 0)).collect();
        for (k, s) in &self.scores {
            if let Some(t) = team_of.get(k).filter(|t| (1..=count).contains(*t)) {
                v[*t as usize - 1].1 += s;
            }
        }
        v.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
        v
    }
}

/// `UpdateFightAreaForPlayerCount`: half side of the deathmatch ring around (50, 50).
pub fn arena_half_side(min_arena: i32, players: usize, modifier_tenths: i32) -> i32 {
    min_arena + (players as f64 * modifier_tenths as f64 / 10.0) as i32
}

/// Stacks a deathmatch item spawns (`StackCount`/`InitialStacks` are tenths per player).
pub fn stacks_for(tenths_per_player: i32, players: usize) -> usize {
    (tenths_per_player as f64 / 10.0 * players as f64).round().max(0.0) as usize
}

// ------------------------------------------------------------------ hourly events

/// An hourly or forced event (`t_EventoPropiedades`): which multipliers it touches.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct HourEvent {
    /// 1 gold, 2 exp, 3 gathering, 4 drop, 5 gold+exp, 6 +gathering, 7 +drop.
    pub kind: u8,
    pub minutes: u8,
    pub mult: u8,
}

impl HourEvent {
    /// `ReadField` of `tipo-duración-multiplicación`.
    pub fn parse(s: &str) -> HourEvent {
        let mut f = s.split('-').map(|p| p.trim().parse::<u8>().unwrap_or(0));
        HourEvent { kind: f.next().unwrap_or(0), minutes: f.next().unwrap_or(0), mult: f.next().unwrap_or(0) }
    }

    pub fn gold(&self) -> bool {
        matches!(self.kind, 1 | 5 | 6 | 7)
    }
    pub fn exp(&self) -> bool {
        matches!(self.kind, 2 | 5 | 6 | 7)
    }
    pub fn gather(&self) -> bool {
        matches!(self.kind, 3 | 6 | 7)
    }
    pub fn drop(&self) -> bool {
        matches!(self.kind, 4 | 7)
    }
    pub fn valid(&self) -> bool {
        (1..=7).contains(&self.kind) && self.mult > 0
    }

    /// The text after "multiplicad..." of `CheckEvento` / `DescribirEvento`.
    pub fn what(&self) -> &'static str {
        match self.kind {
            1 => "Oro multiplicado por",
            2 => "Experiencia multiplicada por",
            3 => "Recoleccion multiplicada por",
            4 => "Dropeo multiplicado por",
            5 => "Oro y experiencia multiplicados por",
            6 => "Oro, experiencia y recoleccion multiplicados por",
            _ => "Oro, experiencia, recoleccion y dropeo multiplicados por",
        }
    }
}

/// `HandleCrearEvento` + `ForzarEvento` checks: types 1-3, up to 59 minutes, gold and exp
/// up to x2, gathering up to x5 (a multiplier above 5 became 2 before the checks).
pub fn validate_forced(kind: u8, minutes: u8, mult: u8) -> Result<HourEvent, &'static str> {
    let mult = if mult > 5 { 2 } else { mult };
    if !(1..=3).contains(&kind) {
        return Err("Tipo de evento invalido.");
    }
    if minutes > 59 || minutes == 0 {
        return Err("Duracion invalida. maxima 59 minutos.");
    }
    if mult == 0 || (kind <= 2 && mult > 2) {
        return Err("Multiplicacion invalida. maxima x2.");
    }
    Ok(HourEvent { kind, minutes, mult })
}

/// `HandleEventoInfo`: the next scheduled hour after `hour` (wrapping), if any.
pub fn next_event_hour(table: &[HourEvent; 24], hour: u32) -> Option<u32> {
    (1..=24).map(|d| (hour + d) % 24).find(|h| table[*h as usize].kind != 0)
}

// ------------------------------------------------------------------ global quests

/// `t_GlobalQuestData` (the static part, from `GlobalQuests.dat`).
#[derive(Debug, Clone, PartialEq)]
pub struct GlobalQuestDef {
    pub index: u32,
    pub name: String,
    pub threshold: i64,
    /// `GatheringInitialInstallments`: every this many points a boss wave appears.
    pub installment: i64,
    pub boss_map: u16,
    /// Normalised box (the data has inverted corners in quest 3).
    pub boss_box: (i32, i32, i32, i32),
    pub bosses: Vec<u16>,
    pub finish_on_threshold: bool,
    pub start: i64,
    pub end: i64,
    pub obj: u16,
}

/// Reads `GlobalQuests.dat` (an INI: `[INIT] NumGlobalQuest`, `[GlobalQuestN]`).
pub fn parse_global_quests(text: &str, offset_hours: i32) -> Vec<GlobalQuestDef> {
    let ini = ini(text);
    let get = |s: &str, k: &str| ini.get(&s.to_lowercase()).and_then(|m| m.get(&k.to_lowercase())).cloned().unwrap_or_default();
    let n: u32 = get("INIT", "NumGlobalQuest").trim().parse().unwrap_or(0);
    let num = |s: &str, k: &str| get(s, k).trim().parse::<i64>().unwrap_or(0);
    (1..=n)
        .filter_map(|i| {
            let s = format!("GlobalQuest{i}");
            let (x1, y1, x2, y2) = (num(&s, "BossSpawnPositionTopLeftX"), num(&s, "BossSpawnPositionTopLeftY"), num(&s, "BossSpawnPositionBottomRightX"), num(&s, "BossSpawnPositionBottomRightY"));
            Some(GlobalQuestDef {
                index: i,
                name: get(&s, "Name").trim().to_string(),
                threshold: num(&s, "GatheringThreshold"),
                installment: num(&s, "GatheringInitialInstallments"),
                boss_map: num(&s, "BossSpawnMap") as u16,
                boss_box: (x1.min(x2) as i32, y1.min(y2) as i32, x1.max(x2) as i32, y1.max(y2) as i32),
                bosses: get(&s, "BossIndex").split('-').filter_map(|b| b.trim().parse().ok()).filter(|b| *b > 0).collect(),
                finish_on_threshold: num(&s, "FinishOnThresholdReach") != 0,
                start: parse_datetime(&get(&s, "StartDate"), offset_hours)?,
                end: parse_datetime(&get(&s, "EndDate"), offset_hours)?,
                obj: num(&s, "ObjectIndex") as u16,
            })
        })
        .collect()
}

/// What `MaybeChangeGlobalQuestsState` does with a quest now.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QuestChange {
    Start,
    Finish,
    Keep,
}

pub fn global_quest_change(active: bool, now: i64, def: &GlobalQuestDef) -> QuestChange {
    let ended = now > def.end;
    let started = now > def.start;
    if active && ended && !def.finish_on_threshold {
        QuestChange::Finish
    } else if !active && started && !ended {
        QuestChange::Start
    } else {
        QuestChange::Keep
    }
}

// ------------------------------------------------------------------ castle

/// Footprint of a castle around its foundation tile (x, y): `CastleX/YNegative/PositiveOffset`.
pub const CASTLE_NEG: (i32, i32) = (8, 8);
pub const CASTLE_POS: (i32, i32) = (6, 2);
pub const CASTLE_TRIGGER_FIRST: u16 = 21;
pub const CASTLE_TRIGGER_LAST: u16 = 40;
pub const CASTLE_FOUNDATION_TRIGGER: u16 = 41;
pub const CASTLE_MOCKUP_OBJ: u16 = 6382;
pub const CASTLE_SIGN_OBJ: u16 = 6419;
pub const CASTLE_RELOCATION_DAYS: i64 = 7;

pub fn castle_footprint_in_bounds(x: i32, y: i32) -> bool {
    let ok = |v: i32| (1..=100).contains(&v);
    ok(x - CASTLE_NEG.0) && ok(y - CASTLE_NEG.1) && ok(x + CASTLE_POS.0) && ok(y + CASTLE_POS.1)
}

/// The tiles `CreateCastleInMap` blocks (the building's walls), relative to (x, y).
pub fn castle_blocked_tiles(x: i32, y: i32) -> Vec<(i32, i32)> {
    let mut v = Vec::new();
    for dx in [-6, -5, -4, -3, 0, 1, 2, 3] {
        v.push((x + dx, y));
    }
    for dy in 1..=7 {
        for dx in -6..=3 {
            // Row y-1 keeps the two doorway tiles (x-1, x-2) open: they are the exits.
            if dy == 1 && (dx == -1 || dx == -2) {
                continue;
            }
            v.push((x + dx, y - dy));
        }
    }
    v
}

/// The four tiles stamped with the castle's entry trigger (in front of the door).
pub fn castle_entry_tiles(x: i32, y: i32) -> [(i32, i32); 4] {
    [(x - 1, y), (x - 2, y), (x - 1, y + 1), (x - 2, y + 1)]
}

/// The two doorway tiles that take you inside.
pub fn castle_door_tiles(x: i32, y: i32) -> [(i32, i32); 2] {
    [(x - 1, y - 1), (x - 2, y - 1)]
}

const PREFIXES: [&str; 20] = ["Dragon's", "Eagle's", "Shadow", "Iron", "Stone", "Crystal", "Dark", "Golden", "Silver", "Frost", "Storm", "Blood", "Ancient", "Royal", "Mystic", "Thunder", "Moon", "Star", "Raven's", "Wolf's"];
const SUFFIXES: [&str; 20] = ["Keep", "Fortress", "Citadel", "Bastion", "Tower", "Hold", "Stronghold", "Castle", "Sanctuary", "Haven", "Peak", "Spire", "Gate", "Watch", "Rest", "Reach", "Guard", "Crest", "Crown", "Throne"];

/// `GenerateRandomCastleName` from two rolls in 0..20.
pub fn castle_name(a: usize, b: usize) -> String {
    format!("{} {}", PREFIXES[a % 20], SUFFIXES[b % 20])
}

// ------------------------------------------------------------------ capture the flag

/// `ModCaptura.bas` constants.
pub mod capture {
    pub const SIGNUP_SECS: i32 = 90;
    pub const ROUND_START_SECS: i32 = 60;
    pub const FLAG_HOLD_SECS: i32 = 7;
    pub const DEATH_SECS: i32 = 5;
    pub const SECS_PER_DEATH: i32 = 2;
    pub const WAIT_MAP: u16 = 278;
    pub const WAIT_BOX: (i32, i32, i32, i32) = (45, 68, 56, 73);
    pub const TEAM_MAP: [u16; 2] = [275, 277];
    pub const TEAM_BASE: [(i32, i32); 2] = [(43, 51), (65, 55)];
    pub const FLAG_POS: [(i32, i32); 2] = [(37, 51), (71, 55)];
    pub const FLAG_OBJ: [u16; 2] = [3674, 3675];
    pub const COLOR: [&str; 2] = ["azul", "rojo"];

    /// `finalizarRonda`: wins needed in a best of `rounds`.
    pub fn rounds_to_win(rounds: u32) -> u32 {
        rounds / 2 + 1
    }

    /// `iniciarCaptura` team order over players sorted by level: 1, 2, 2, 1, 1, 2, 2...
    pub fn teams(n: usize) -> Vec<u8> {
        let (mut team, mut counter) = (1u8, 1u8);
        let mut v = Vec::with_capacity(n);
        for _ in 0..n {
            v.push(team);
            counter += 1;
            if counter == 2 {
                team = if team == 1 { 2 } else { 1 };
                counter = 0;
            }
        }
        v
    }

    /// Seconds a dead player waits in the waiting room (`muereUsuario`).
    pub fn respawn_secs(deaths: u32) -> i32 {
        deaths as i32 * SECS_PER_DEATH + DEATH_SECS
    }

    /// "In my base" = within 8 x 5 tiles of my flag stand.
    pub fn in_base(team: usize, map: u16, x: i32, y: i32) -> bool {
        map == TEAM_MAP[team] && (x - FLAG_POS[team].0).abs() <= 8 && (y - FLAG_POS[team].1).abs() <= 5
    }
}

// ------------------------------------------------------------------ invasions

/// `SumarScoreInvasion` fixed: adds to a player's score and keeps the 10 best, sorted.
/// The VB loop that shifted entries down had no `Step -1` (it never ran) and a player
/// already in the table was inserted a second time.
pub fn top10_add(top: &mut Vec<(String, i64)>, name: &str, score: i64) {
    if let Some(e) = top.iter_mut().find(|e| e.0 == name) {
        e.1 += score;
    } else {
        top.push((name.to_string(), score));
    }
    top.sort_by_key(|e| std::cmp::Reverse(e.1));
    top.truncate(10);
}

/// Spawn box of an invasion (`t_SpawnBox`), its direction and wall coordinate.
#[derive(Debug, Clone, PartialEq)]
pub struct SpawnBox {
    pub map: u16,
    pub area: (i32, i32, i32, i32),
    /// 'N', 'S', 'E', 'W'.
    pub heading: char,
    pub wall: i32,
    /// `LegalBox`: the box cut at the wall.
    pub legal: (i32, i32, i32, i32),
}

/// `SpawnBoxX=Mapa-X1-Y1-X2-Y2-Dir-CoordMuralla`.
pub fn parse_spawn_box(s: &str) -> Option<SpawnBox> {
    let f: Vec<&str> = s.split('#').next()?.trim().splitn(7, '-').collect();
    if f.len() != 7 {
        return None;
    }
    let n = |i: usize| f[i].trim().parse::<i32>().ok();
    let heading = match f[5].trim().to_lowercase().as_str() {
        "norte" => 'N',
        "sur" => 'S',
        "este" => 'E',
        "oeste" => 'W',
        _ => return None,
    };
    let area = (n(1)?, n(2)?, n(3)?, n(4)?);
    let wall = n(6)?;
    let mut legal = area;
    match heading {
        'N' => legal.1 = wall,
        'S' => legal.3 = wall,
        'E' => legal.2 = wall,
        _ => legal.0 = wall,
    }
    Some(SpawnBox { map: n(0)? as u16, area, heading, wall, legal })
}

// ------------------------------------------------------------------ ini

/// Minimal INI reader (sections and keys lowercased, `'` and `#` comments, last wins).
pub fn ini(text: &str) -> HashMap<String, HashMap<String, String>> {
    let mut out: HashMap<String, HashMap<String, String>> = HashMap::new();
    let mut sec = String::new();
    for line in text.lines() {
        let l = line.trim();
        if l.is_empty() || l.starts_with('\'') || l.starts_with(';') {
            continue;
        }
        if l.starts_with('[') {
            sec = l.trim_matches(|c| c == '[' || c == ']').to_lowercase();
            continue;
        }
        if let Some((k, v)) = l.split_once('=') {
            let v = v.split('\t').next().unwrap_or("");
            let v = v.split(" #").next().unwrap_or("").split('\'').next().unwrap_or("");
            out.entry(sec.clone()).or_default().insert(k.trim().to_lowercase(), v.trim().to_string());
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn calendar_round_trips() {
        let t = parse_datetime("2026-10-5 23:00:00", 0).unwrap();
        let c = civil(t, 0);
        assert_eq!((c.year, c.month, c.day, c.hour), (2026, 10, 5, 23));
        // 2026-10-05 is a Monday (VB 2).
        assert_eq!(c.weekday, 2);
        assert_eq!(civil(0, 0).weekday, 5);
        let local = parse_datetime("2026-01-01 00:00:00", -3).unwrap();
        assert_eq!(civil(local, -3).hour, 0);
        assert_eq!(civil(local, 0).hour, 3);
    }

    #[test]
    fn teams_go_back_and_forth() {
        assert_eq!(snake_teams(7, 3), vec![1, 2, 3, 3, 2, 1, 1]);
        assert_eq!(players_for_teams(7, 2), 6);
        assert_eq!(capture::teams(6), vec![1, 2, 2, 1, 1, 2]);
        assert_eq!(capture::rounds_to_win(3), 2);
    }

    #[test]
    fn scoreboard_never_goes_negative_and_ranks() {
        let mut b = ScoreBoard::default();
        for k in [10, 20, 30] {
            b.add_player(k);
        }
        assert_eq!(b.update(10, -1), 0);
        b.update(20, 3);
        b.update(30, 3);
        b.update(30, 1);
        assert_eq!(b.ranking(2), vec![(30, 4), (20, 3)]);
        let teams: HashMap<i64, u32> = [(10, 1), (20, 1), (30, 2)].into();
        assert_eq!(b.team_ranking(&teams, 2), vec![(2, 4), (1, 3)]);
        assert_eq!(prize_split(1000, 2), [550, 350, 0]);
        assert_eq!(prize_split(1000, 5), [400, 300, 200]);
    }

    #[test]
    fn forced_events_follow_the_vb_limits() {
        assert!(validate_forced(4, 10, 2).is_err());
        assert!(validate_forced(1, 60, 2).is_err());
        assert!(validate_forced(2, 30, 3).is_err());
        assert_eq!(validate_forced(3, 30, 5).unwrap().mult, 5);
        assert_eq!(validate_forced(1, 30, 9).unwrap().mult, 2);
        let mut t = [HourEvent::default(); 24];
        t[3] = HourEvent::parse("2-30-2");
        assert_eq!(next_event_hour(&t, 20), Some(3));
        assert!(t[3].exp() && !t[3].gold());
    }

    #[test]
    fn global_quests_read_the_official_file() {
        let text = std::fs::read(dat("GlobalQuests.dat"));
        let Ok(bytes) = text else { return };
        let text: String = bytes.iter().map(|b| *b as char).collect();
        let q = parse_global_quests(&text, 0);
        assert_eq!(q.len(), 4);
        assert_eq!(q[3].bosses, vec![860, 861, 858, 569, 859]);
        assert_eq!(q[2].boss_box, (45, 71, 55, 88));
        assert!(q[3].end > q[3].start);
        assert_eq!(global_quest_change(false, q[0].start + 10, &q[0]), QuestChange::Start);
        assert_eq!(global_quest_change(true, q[0].end + 10, &q[0]), QuestChange::Finish);
        assert_eq!(global_quest_change(true, q[1].end + 10, &q[1]), QuestChange::Keep);
    }

    pub(super) fn dat(name: &str) -> std::path::PathBuf {
        let root = std::env::var("AO_RECURSOS").unwrap_or_else(|_| "X:/dev/ao20_full".into());
        std::path::Path::new(&root).join("Dat").join(name)
    }

    #[test]
    fn castle_geometry() {
        assert!(castle_footprint_in_bounds(9, 9));
        assert!(!castle_footprint_in_bounds(8, 50));
        let b = castle_blocked_tiles(50, 50);
        assert!(!b.contains(&(49, 49)) && !b.contains(&(48, 49)), "the door stays open");
        assert!(b.contains(&(50, 50)) && b.contains(&(44, 43)));
        assert_eq!(castle_name(0, 19), "Dragon's Throne");
    }

    #[test]
    fn invasion_parsing_and_top10() {
        let b = parse_spawn_box("58-53-21-81-24-Norte-20").unwrap();
        assert_eq!((b.map, b.heading, b.legal.1), (58, 'N', 20));
        let mut t = Vec::new();
        top10_add(&mut t, "a", 5);
        top10_add(&mut t, "b", 7);
        top10_add(&mut t, "a", 5);
        assert_eq!(t, vec![("a".to_string(), 10), ("b".to_string(), 7)]);
    }
}

//! World events: port of `ModEventos.bas` (hourly gold / exp / gathering / drop
//! multipliers, `ForzarEvento`, the automatic lobby of `frmMain.Automatic_Event_Timer`),
//! `ModGlobalQuests.bas` (global quests: a shared counter, bosses every installment,
//! start and end dates, kept in the database) and `ModInvasion.bas` (invasions).
//!
//! Commands: `/EVENTO` (anyone: the running multiplier and the next one), `/CREAREVENTO
//! tipo@duración@multiplicación` (Admin), `/FINALIZAREVENTO` (SemiDios), `/CE INVASION
//! BANDER|INVASION CARCEL` (Dios).
//!
//! VB defects fixed:
//! - The client parser has `/CREAREVENTO` twice: the lobby one came first, so the forced
//!   multiplier (`HandleCrearEvento`) was unreachable. Here `@` picks the multiplier.
//! - `frmMain.Evento` (the timer counting `TiempoRestanteEvento` down) is gone from the
//!   form: an event never ended by itself. Here it lasts its minutes.
//! - `FinalizarEvento` restored the multipliers read at boot, undoing a GM's `/EXP` in
//!   between; here it undoes only what the event applied.
//! - `HandleEventoInfo` announced the next event with the description of the current one.
//! - `SumarScoreInvasion` duplicated players and never shifted the table (see
//!   `ao_rules::events::top10_add`).
//! - Invasions: `Cantidad=0` in `Eventos.dat` (official), so they stay off unless the data
//!   enables them, exactly as in AO20; `/CE` follows the same switch.
use crate::clock::GameTime;
use crate::world::World;
use ao_formats::dat::quests::Quest;
use ao_protocol::p4::gm::GmRank;
use ao_protocol::{Tone, S2C};
use ao_rules::events::{self as rules, EventType, GlobalQuestDef, HourEvent, QuestChange, SpawnBox};
use std::collections::HashMap;

/// `IntervalChangeGlobalQuestsState`.
const GLOBAL_QUEST_CHECK_MS: u64 = 60_000;
/// `e_SoundEffects.BAOLegionHorn`.
const LEGION_HORN: u16 = 186;
/// Thunders of `CheckEvento` / `FinalizarEvento`.
const WAV_EVENT_START: u16 = 553;
const WAV_EVENT_END: u16 = 551;

/// A running multiplier event.
#[derive(Debug, Clone)]
pub struct ActiveHourEvent {
    pub ev: HourEvent,
    pub minutes_left: u32,
    pub next_minute: GameTime,
    /// What was multiplied, to undo exactly that: (exp, gold, drop, gather) factors.
    pub applied: (u32, u32, u32, u32),
    pub text: String,
}

/// One global quest at runtime (`t_GlobalQuestData`).
#[derive(Debug, Clone)]
pub struct GlobalQuest {
    pub def: GlobalQuestDef,
    pub active: bool,
    pub counter: i64,
    /// `GatheringGlobalInstallments`: next count that brings the bosses.
    pub next_wave: i64,
    /// Live bosses (map, npc id).
    pub bosses: Vec<(u16, u32)>,
}

/// `t_Invasion`.
#[derive(Debug, Clone, Default)]
pub struct Invasion {
    pub name: String,
    pub active: bool,
    pub wall: i64,
    pub max_wall: i64,
    pub top: Vec<(String, i64)>,
    /// Live invaders: npc id -> (map, spawn box).
    pub alive: HashMap<u32, (u16, usize)>,
    pub npcs: Vec<u16>,
    pub max_npcs: usize,
    pub warn: String,
    pub warn_minutes: i64,
    pub warn_every: i64,
    pub warn_timer: i64,
    pub desc: String,
    pub desc_every: i64,
    pub desc_timer: i64,
    pub boxes: Vec<SpawnBox>,
    pub spawn_every: i64,
    pub spawn_timer: i64,
    /// Minutes counter (`TimerInvasion`, starts at `Offset`).
    pub minutes: i64,
    pub interval: i64,
    pub duration: i64,
    pub info_timer: i64,
    pub won: String,
    pub lost: String,
}

#[derive(Debug, Default)]
pub struct WorldEvents {
    pub hourly: Option<[HourEvent; 24]>,
    pub active: Option<ActiveHourEvent>,
    /// Last local hour seen (an hourly event starts when it changes).
    pub last_hour: Option<u32>,
    /// `AlreadyDidAutoEventToday` / `LastAutoEventAttempt`: day number of the last one.
    pub auto_day: Option<i64>,
    pub quests: Option<Vec<GlobalQuest>>,
    pub next_quest_check: GameTime,
    pub invasions: Option<Vec<Invasion>>,
    /// `Cantidad` of `Eventos.dat` > 0.
    pub invasions_on: bool,
    pub next_second: GameTime,
    pub seconds: u64,
    /// `EnEventoFaccionario` (`/EVENTOFACCIONARIO`).
    pub faction_event: bool,
}

fn latin1(path: std::path::PathBuf) -> Option<String> {
    std::fs::read(path).ok().map(|b| b.iter().map(|c| *c as char).collect())
}

impl World {
    fn sound_all(&self, wav: u16) {
        for id in self.player_ids() {
            if let Some(p) = self.player(id) {
                self.send(id, &S2C::Sound { wav, x: p.x, y: p.y });
            }
        }
    }

    // ================================================================== tick

    pub(crate) fn tick_world_events(&mut self, now: GameTime) {
        if now < self.wevents.next_second {
            return;
        }
        self.wevents.next_second = now.after_ms(1000);
        self.wevents.seconds += 1;
        let local = rules::civil(self.unix_now(), self.cfg.events.utc_offset_hours);
        if self.wevents.last_hour != Some(local.hour) {
            let first = self.wevents.last_hour.is_none();
            self.wevents.last_hour = Some(local.hour);
            if !first {
                self.hourly_check(local.hour);
            }
        }
        self.tick_hour_event(now);
        self.auto_event_check(local);
        if now >= self.wevents.next_quest_check {
            self.wevents.next_quest_check = now.after_ms(GLOBAL_QUEST_CHECK_MS);
            self.gquest_check_dates();
        }
        self.tick_invasions();
    }

    // ================================================================== hourly events

    fn hourly_table(&mut self) -> [HourEvent; 24] {
        if self.wevents.hourly.is_none() {
            let mut t = [HourEvent::default(); 24];
            for (h, v) in &self.cfg.events.hourly {
                if let Ok(h) = h.trim().parse::<usize>() {
                    if h < 24 {
                        t[h] = HourEvent::parse(v);
                    }
                }
            }
            self.wevents.hourly = Some(t);
        }
        self.wevents.hourly.unwrap()
    }

    /// `CheckEvento`.
    fn hourly_check(&mut self, hour: u32) {
        if self.wevents.active.is_some() {
            return;
        }
        let ev = self.hourly_table()[hour as usize];
        if ev.valid() && ev.minutes > 0 {
            let text = format!("Eventos> Nuevo evento iniciado: {} {} - Duración del evento: {} minutos.", ev.what(), ev.mult, ev.minutes);
            self.hour_event_begin(ev, text);
        }
    }

    fn hour_event_begin(&mut self, ev: HourEvent, text: String) {
        let w = &mut self.cfg.world;
        let m = ev.mult as u32;
        let f = |on: bool| if on { m } else { 1 };
        let applied = (f(ev.exp()), f(ev.gold()), f(ev.drop()), f(ev.gather()));
        w.exp_mult = w.exp_mult.max(1) * applied.0;
        w.gold_mult = w.gold_mult.max(1) * applied.1;
        w.drop_mult = w.drop_mult.max(1) * applied.2;
        w.gather_mult = w.gather_mult.max(1) * applied.3;
        let now = self.now();
        self.wevents.active = Some(ActiveHourEvent { ev, minutes_left: ev.minutes as u32, next_minute: now.after_ms(60_000), applied, text: format!("Evento en curso> {} {}", ev.what(), ev.mult) });
        self.sound_all(WAV_EVENT_START);
        self.broadcast_all(&S2C::Console { text, color: Tone::Good }, None);
    }

    /// `FinalizarEvento`.
    pub(crate) fn hour_event_end(&mut self) {
        let Some(a) = self.wevents.active.take() else { return };
        let w = &mut self.cfg.world;
        w.exp_mult = (w.exp_mult / a.applied.0).max(1);
        w.gold_mult = (w.gold_mult / a.applied.1).max(1);
        w.drop_mult = (w.drop_mult / a.applied.2).max(1);
        w.gather_mult = (w.gather_mult / a.applied.3).max(1);
        self.f6_say_all("Eventos > Evento finalizado.");
        self.sound_all(WAV_EVENT_END);
    }

    fn tick_hour_event(&mut self, now: GameTime) {
        let Some(a) = &mut self.wevents.active else { return };
        if now < a.next_minute {
            return;
        }
        a.next_minute = now.after_ms(60_000);
        a.minutes_left = a.minutes_left.saturating_sub(1);
        if a.minutes_left == 0 {
            self.hour_event_end();
        }
    }

    /// `HandleEventoInfo` (`/EVENTO`).
    pub(crate) fn hour_event_info(&mut self, id: u32) {
        match &self.wevents.active {
            Some(a) => self.console(id, format!("{}. Tiempo restante: {} minuto(s).", a.text, a.minutes_left), Tone::Info),
            None => self.console(id, "Eventos> Actualmente no hay ningún evento en curso.", Tone::Info),
        }
        let table = self.hourly_table();
        let hour = self.wevents.last_hour.unwrap_or(0);
        match rules::next_event_hour(&table, hour) {
            Some(h) => {
                let e = table[h as usize];
                self.console(id, format!("Eventos> El proximo evento ({} {} - Duración: {} minutos) iniciara a las {h}:00 horas.", e.what(), e.mult, e.minutes), Tone::Info)
            }
            None => self.console(id, "Eventos> No hay eventos próximos.", Tone::Info),
        }
        if self.wevents.faction_event {
            self.console(id, "Evento faccionario en curso.", Tone::Info);
        }
    }

    /// `HandleCrearEvento` + `ForzarEvento` (`/CREAREVENTO tipo@duración@multiplicación`).
    pub(crate) fn hour_event_force(&mut self, id: u32, me: &str, a: &[String]) {
        if self.gm_rank(id) < GmRank::Admin {
            return self.console(id, "Servidor >> Solo Administradores pueder crear estos eventos.", Tone::Warn);
        }
        if self.wevents.active.is_some() {
            return self.console(id, "Ya hay un evento en curso. Finalicelo con /FINALIZAREVENTO primero.", Tone::Warn);
        }
        let n = |i: usize| a.get(i).and_then(|s| s.trim().parse::<u8>().ok());
        let (Some(k), Some(d), Some(m)) = (n(0), n(1), n(2)) else { return self.console(id, "Utilice /CREAREVENTO TIPO@DURACION@MULTIPLICACION.", Tone::Info) };
        match rules::validate_forced(k, d, m) {
            Ok(ev) => {
                let text = format!("Eventos> {me} inicio un nuevo evento: {} {} - Duración del evento: {} minutos.", ev.what(), ev.mult, ev.minutes);
                self.hour_event_begin(ev, text);
                self.log_gm(me, "forzar evento", "", &format!("{k}@{d}@{m}"));
            }
            Err(e) => self.console(id, e, Tone::Warn),
        }
    }

    /// `Automatic_Event_Timer`: at `auto_hour`, the event of the weekday, once a day.
    fn auto_event_check(&mut self, local: rules::Civil) {
        let ev = &self.cfg.events;
        if ev.auto_hour < 0 || local.hour as i32 != ev.auto_hour {
            return;
        }
        let day = rules::days_from_civil(local.year, local.month, local.day);
        if self.wevents.auto_day == Some(day) {
            return;
        }
        self.wevents.auto_day = Some(day);
        let kind = ev.auto_days[(local.weekday as usize - 1).min(6)];
        let Some(kind) = EventType::from_id(kind as i64).filter(|k| *k != EventType::Generic) else { return };
        let (min, max, minp, maxp, team, rounds, fee) = (ev.auto_min_level, ev.auto_max_level, ev.auto_min_players, ev.auto_max_players, ev.auto_team_size, ev.auto_rounds, ev.auto_fee);
        let r = if kind == EventType::CaptureTheFlag {
            self.capture_start(maxp, rounds, min, max, fee as i64).map_err(str::to_string)
        } else {
            let s = crate::systems::lobby::LobbySettings { kind: Some(kind), min_level: min, max_level: max, min_players: minp, max_players: maxp, team_size: team, fee, ..Default::default() };
            self.lobby_create_public(s).map(|_| ())
        };
        if let Err(e) = r {
            tracing::warn!(%e, "evento automático no creado");
        }
    }

    // ================================================================== global quests

    /// `LoadGlobalQuests` (lazily: dat file, then the database state).
    fn gquests(&mut self) -> &mut Vec<GlobalQuest> {
        if self.wevents.quests.is_none() {
            let off = self.cfg.events.utc_offset_hours;
            let defs = latin1(self.recursos.join("Dat").join("GlobalQuests.dat")).map(|t| rules::parse_global_quests(&t, off)).unwrap_or_default();
            let mut list = Vec::new();
            for def in defs {
                let (mut active, mut counter) = (false, 0);
                match self.db.global_quest(def.index) {
                    Ok(Some((a, c))) => {
                        active = a;
                        counter = c;
                    }
                    Ok(None) => {
                        let _ = self.db.global_quest_insert(def.index, &def.name, def.obj, def.threshold, def.start, def.end);
                    }
                    Err(e) => tracing::warn!(?e, "no se pudo leer la quest global"),
                }
                let next_wave = counter + def.installment;
                list.push(GlobalQuest { def, active, counter, next_wave, bosses: Vec::new() });
            }
            self.wevents.quests = Some(list);
        }
        self.wevents.quests.as_mut().unwrap()
    }

    fn gquest(&mut self, index: i32) -> Option<&mut GlobalQuest> {
        self.gquests().iter_mut().find(|q| q.def.index as i32 == index)
    }

    /// `MaybeChangeGlobalQuestsState`.
    fn gquest_check_dates(&mut self) {
        let now = self.unix_now();
        let n = self.gquests().len();
        for i in 0..n {
            let q = &self.wevents.quests.as_ref().unwrap()[i];
            match rules::global_quest_change(q.active, now, &q.def) {
                QuestChange::Start => {
                    let q = &mut self.wevents.quests.as_mut().unwrap()[i];
                    q.active = true;
                    let (idx, name, th, obj) = (q.def.index, q.def.name.clone(), q.def.threshold, q.def.obj);
                    let _ = self.db.global_quest_set_active(idx, true);
                    let oname = self.ao.objs.get(obj as u32).map_or(String::new(), |o| o.name.clone());
                    self.f6_say_all(format!("El evento global {name} ha iniciado! el objetivo es entregar {th} {oname}."));
                    self.sound_all(LEGION_HORN);
                }
                QuestChange::Finish => self.gquest_finish(i),
                QuestChange::Keep => {}
            }
        }
    }

    /// `FinalizeGlobalQuest`.
    fn gquest_finish(&mut self, i: usize) {
        let q = &mut self.wevents.quests.as_mut().unwrap()[i];
        q.active = false;
        let (idx, name, c, obj) = (q.def.index, q.def.name.clone(), q.counter, q.def.obj);
        let _ = self.db.global_quest_set_active(idx, false);
        let oname = self.ao.objs.get(obj as u32).map_or(String::new(), |o| o.name.clone());
        self.f6_say_all(format!("El evento global {name} ha finalizado! Se han entregado un total de {c} {oname}."));
        self.sound_all(LEGION_HORN);
    }

    /// `CanUserAcceptQuest` (global part): the quest may be taken now.
    pub(crate) fn gquest_accept_open(&self, q: &Quest) -> bool {
        if q.global_quest_index <= 0 {
            return true;
        }
        let Some(g) = self.wevents.quests.as_ref().and_then(|l| l.iter().find(|g| g.def.index as i32 == q.global_quest_index)) else { return false };
        if q.global_quest_threshold > 0 {
            g.counter >= q.global_quest_threshold as i64
        } else {
            g.active && g.bosses.is_empty()
        }
    }

    /// `FinishGlobalQuestCheck`; `tell` sends the official refusal.
    pub(crate) fn gquest_finish_check(&self, id: u32, q: &Quest, tell: bool) -> bool {
        let need = q.global_quest_threshold as i64;
        let Some(g) = self.wevents.quests.as_ref().and_then(|l| l.iter().find(|g| g.def.index as i32 == q.global_quest_index)) else { return false };
        let why = if need > 0 {
            (g.counter < need).then(|| format!("Esta recompensa esta bloqueada, aun no se ha alcanzado el objetivo global necesario, de momento solo se han entregado {} / {} y se necesitan mas de {need}", g.counter, g.def.threshold))
        } else if !g.bosses.is_empty() {
            Some("No puedes entregar objetos estacionales si el jefe del evento esta vivo!".into())
        } else if !g.active {
            Some("El evento global ya ha finalizado, no puedes entregar mas objetos.".into())
        } else {
            None
        };
        if let Some(w) = &why {
            if tell {
                self.console(id, w.clone(), Tone::Warn);
            }
        }
        why.is_none()
    }

    /// `FinishGlobalQuest` + `ContributeToGlobalQuestCounter` + `InsertContributionIntoDatabase`.
    pub(crate) fn gquest_contribute(&mut self, id: u32, q: &Quest, amount: i64) {
        if q.global_quest_index <= 0 || q.global_quest_threshold != 0 || amount <= 0 {
            return;
        }
        let name = self.name_of(id);
        let at = self.unix_now();
        let Some(g) = self.gquest(q.global_quest_index) else { return };
        g.counter += amount;
        let mut wave = None;
        if g.counter >= g.next_wave && g.def.installment > 0 {
            g.next_wave += g.def.installment;
            if g.bosses.is_empty() {
                wave = Some((g.def.boss_map, g.def.boss_box, g.def.bosses.clone()));
            }
        }
        let (idx, obj, c, th, finish) = (g.def.index, g.def.obj, g.counter, g.def.threshold, g.def.finish_on_threshold);
        let _ = self.db.global_quest_contribute(idx, &name, at, amount);
        let oname = self.ao.objs.get(obj as u32).map_or(String::new(), |o| o.name.clone());
        self.f6_say_all(format!("{name} ha entregado {amount} {oname}, conteo total: {c} / {th}."));
        if let Some((map, (x1, y1, x2, y2), bosses)) = wave {
            let (x, y) = (self.rng.range(x1, x2), self.rng.range(y1, y2));
            let mut alive = Vec::new();
            for b in bosses {
                if let Some(nid) = self.f6_spawn_npc(map, b, x, y, true) {
                    alive.push((map, nid));
                }
            }
            self.gquest(idx as i32).unwrap().bosses = alive;
        }
        if finish && c >= th {
            let g = self.gquest(idx as i32).unwrap();
            g.active = false;
            let _ = self.db.global_quest_set_active(idx, false);
        }
    }

    /// HOOK (combat::kill_npc): `MarkGlobalQuestBossAsDead` and `MuereNpcInvasion`.
    pub(crate) fn world_events_on_npc_killed(&mut self, map: u16, nid: u32, killer: u32) {
        if let Some(list) = &mut self.wevents.quests {
            for g in list.iter_mut() {
                g.bosses.retain(|b| *b != (map, nid));
            }
        }
        let mut scored = None;
        if let Some(list) = &mut self.wevents.invasions {
            for (i, inv) in list.iter_mut().enumerate() {
                if inv.alive.remove(&nid).is_some() {
                    scored = Some(i);
                }
            }
        }
        if let (Some(i), Some(p)) = (scored, self.player(killer)) {
            let name = p.name.clone();
            let inv = &mut self.wevents.invasions.as_mut().unwrap()[i];
            rules::top10_add(&mut inv.top, &name, 1);
        }
    }

    // ================================================================== invasions

    /// `CargarInfoEventos`.
    fn invasions(&mut self) -> &mut Vec<Invasion> {
        if self.wevents.invasions.is_none() {
            let ini = latin1(self.recursos.join("Dat").join("Eventos.dat")).map(|t| rules::ini(&t)).unwrap_or_default();
            let get = |s: &str, k: &str| ini.get(&s.to_lowercase()).and_then(|m| m.get(&k.to_lowercase())).cloned().unwrap_or_default();
            let num = |s: &str, k: &str| get(s, k).trim().parse::<i64>().unwrap_or(0);
            self.wevents.invasions_on = num("Invasiones", "Cantidad") > 0;
            let mut list = Vec::new();
            for i in 1..=9 {
                let name = get("Invasiones", &format!("Invasion{i}"));
                if name.is_empty() || !ini.contains_key(&name.to_lowercase()) {
                    continue;
                }
                let s = name.as_str();
                list.push(Invasion {
                    name: name.clone(),
                    max_wall: num(s, "MaxVidaMuralla"),
                    max_npcs: num(s, "MaxNPCs").max(0) as usize,
                    warn: get(s, "Aviso"),
                    warn_minutes: num(s, "AvisarTiempo"),
                    warn_every: num(s, "RepetirAviso"),
                    desc: get(s, "Desc"),
                    desc_every: num(s, "RepetirDesc"),
                    spawn_every: num(s, "IntervaloSpawn"),
                    duration: num(s, "Duracion"),
                    interval: num(s, "Intervalo"),
                    minutes: num(s, "Offset"),
                    won: get(s, "MensajeGanaron"),
                    lost: get(s, "MensajePerdieron"),
                    npcs: get(s, "NPCs").split('-').filter_map(|n| n.trim().parse().ok()).collect(),
                    boxes: (1..=num(s, "SpawnBoxes")).filter_map(|j| rules::parse_spawn_box(&get(s, &format!("SpawnBox{j}")))).collect(),
                    ..Default::default()
                });
            }
            self.wevents.invasions = Some(list);
        }
        self.wevents.invasions.as_mut().unwrap()
    }

    /// `IniciarInvasion`.
    pub(crate) fn invasion_start(&mut self, i: usize) -> bool {
        self.invasions();
        if !self.wevents.invasions_on || i >= self.invasions().len() {
            return false;
        }
        let inv = &mut self.invasions()[i];
        inv.active = true;
        inv.wall = inv.max_wall;
        inv.minutes = 0;
        let desc = inv.desc.clone();
        self.f6_say_all(desc);
        self.sound_all(150);
        true
    }

    /// `FinalizarInvasion`.
    pub(crate) fn invasion_end(&mut self, i: usize) {
        let inv = &mut self.invasions()[i];
        let won = inv.wall > 0;
        inv.active = false;
        inv.warn_timer = 0;
        inv.minutes = 0;
        inv.info_timer = 0;
        inv.desc_timer = 0;
        inv.spawn_timer = 0;
        let alive: Vec<(u32, u16)> = inv.alive.drain().map(|(n, (m, _))| (n, m)).collect();
        let top = std::mem::take(&mut inv.top);
        let text = if won { inv.won.clone() } else { inv.lost.clone() };
        self.f6_say_all(text);
        self.sound_all(if won { 44 } else { 45 });
        for (nid, map) in alive {
            self.f6_remove_npc(map, nid);
        }
        if won {
            let gold = 50_000u64 * self.cfg.world.gold_mult.max(1) as u64;
            for (name, score) in top {
                if score <= 0 {
                    continue;
                }
                if let Some(id) = self.id_by_name(&name) {
                    self.console(id, format!("¡La ciudad te entrega {gold} monedas de oro por tu ayuda durante la invasión!"), Tone::Good);
                    self.f6_give_gold(id, gold);
                }
            }
        }
    }

    /// `Invasion_Timer` (every minute) and the spawn / info part of `General.bas`
    /// (every second).
    fn tick_invasions(&mut self) {
        if self.wevents.invasions.is_none() {
            self.invasions();
        }
        let minute = self.wevents.seconds.is_multiple_of(60);
        let on = self.wevents.invasions_on;
        let n = self.invasions().len();
        for i in 0..n {
            if minute {
                let inv = &mut self.invasions()[i];
                inv.minutes += 1;
                if inv.active {
                    if inv.minutes >= inv.duration {
                        self.invasion_end(i);
                        continue;
                    }
                    inv.desc_timer += 1;
                    if inv.desc_timer >= inv.desc_every {
                        inv.desc_timer = 0;
                        let d = inv.desc.clone();
                        self.f6_say_all(d);
                    }
                } else if inv.interval > 0 && on {
                    if inv.minutes >= inv.interval {
                        self.invasion_start(i);
                    } else if inv.minutes >= inv.interval - inv.warn_minutes {
                        inv.warn_timer -= 1;
                        if inv.warn_timer <= 0 {
                            inv.warn_timer = inv.warn_every;
                            let w = inv.warn.clone();
                            self.f6_say_all(w);
                        }
                    }
                }
            }
            let inv = &mut self.invasions()[i];
            if inv.active {
                inv.spawn_timer += 1;
                if inv.spawn_timer >= inv.spawn_every {
                    inv.spawn_timer = 0;
                    self.invasion_spawn(i);
                }
            }
        }
    }

    /// `InvasionSpawnNPC`.
    fn invasion_spawn(&mut self, i: usize) {
        let inv = &self.invasions()[i];
        if inv.alive.len() >= inv.max_npcs || inv.npcs.is_empty() || inv.boxes.is_empty() {
            return;
        }
        let (npcs, boxes) = (inv.npcs.clone(), inv.boxes.clone());
        let npc = npcs[self.rng.range(0, npcs.len() as i32 - 1) as usize];
        let b = self.rng.range(0, boxes.len() as i32 - 1) as usize;
        let sb = &boxes[b];
        let (x, y) = (self.rng.range(sb.area.0, sb.area.2), self.rng.range(sb.area.1, sb.area.3));
        if let Some(nid) = self.f6_spawn_npc(sb.map, npc, x, y, true) {
            self.invasions()[i].alive.insert(nid, (sb.map, b));
        }
    }

    /// HOOK (npc_ai): `MovimientoInvasion`. True = this creature is an invader (handled).
    pub(crate) fn invasion_npc_ai(&mut self, map: u16, nid: u32, now: GameTime) -> bool {
        let Some(list) = &self.wevents.invasions else { return false };
        let Some((i, sb)) = list.iter().enumerate().find_map(|(i, inv)| inv.alive.get(&nid).map(|(_, b)| (i, inv.boxes[*b].clone()))) else { return false };
        let Some(n) = self.maps.get(&map).and_then(|m| m.npcs.get(&nid)) else { return true };
        let (nx, ny) = (n.x, n.y);
        let vertical = matches!(sb.heading, 'N' | 'S');
        let dist = if vertical { (ny - sb.wall).abs() } else { (nx - sb.wall).abs() };
        if dist > 1 {
            let dir = if vertical {
                if sb.wall < ny { ao_rules::Dir::Up } else { ao_rules::Dir::Down }
            } else if sb.wall < nx {
                ao_rules::Dir::Left
            } else {
                ao_rules::Dir::Right
            };
            let (dx, dy) = dir.delta();
            let (tx, ty) = (nx + dx, ny + dy);
            let (x1, y1, x2, y2) = sb.legal;
            let inside = (x1.min(x2)..=x1.max(x2)).contains(&tx) && (y1.min(y2)..=y1.max(y2)).contains(&ty);
            let dir = if inside { dir } else { match dir { ao_rules::Dir::Up => ao_rules::Dir::Down, ao_rules::Dir::Down => ao_rules::Dir::Up, ao_rules::Dir::Left => ao_rules::Dir::Right, ao_rules::Dir::Right => ao_rules::Dir::Left } };
            self.npc_move(map, nid, dir);
            return true;
        }
        let n = &self.maps[&map].npcs[&nid];
        if now < n.next_attack {
            return true;
        }
        let Some(d) = self.ao.npcs.get(n.def as u32) else { return true };
        let dmg = self.rng.range(d.min_hit, d.max_hit.max(d.min_hit)) as i64;
        let interval = if d.attack_interval > 0 { d.attack_interval as u64 } else { 2000 };
        self.maps.get_mut(&map).unwrap().npcs.get_mut(&nid).unwrap().next_attack = now.after_ms(interval);
        self.broadcast(map, &S2C::Sound { wav: 10, x: nx, y: ny }, None);
        let inv = &mut self.wevents.invasions.as_mut().unwrap()[i];
        inv.wall -= dmg;
        if inv.wall <= 0 {
            self.invasion_end(i);
        }
        true
    }

    /// `/CE nombre` (`HandleCreateEvent`).
    pub(crate) fn invasion_gm(&mut self, id: u32, me: &str, rest: &str) {
        let i = match rest.trim().to_uppercase().as_str() {
            "INVASION BANDER" => 0,
            "INVASION CARCEL" => 1,
            _ => return self.console(id, format!("No existe el evento \"{}\".", rest.trim()), Tone::Warn),
        };
        if self.invasion_start(i) {
            self.log_gm(me, "forzar evento", "", rest.trim());
        } else {
            self.console(id, format!("No existe el evento \"{}\".", rest.trim()), Tone::Warn);
        }
    }
}

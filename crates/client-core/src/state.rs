//! Entities, stats and the handling of every server message.
use crate::events::{pt, ClientEvent, Pt};
use crate::guard::Guarded;
use crate::local::Local;
use crate::net::Net;
use ao_formats::AoMap;
use ao_protocol::{CharacterSummary, EntityKind, Tone, C2S, PROTOCOL_VERSION, S2C};
use ao_rules::spells::SpellDef;
use ao_rules::Dir;
use std::collections::{HashMap, VecDeque};
use std::path::PathBuf;

pub const TILE: f32 = 32.0;

#[derive(Clone, Debug)]
pub struct Ent {
    pub name: String,
    pub kind: EntityKind,
    pub body: u16,
    pub head: u16,
    pub x: i32,
    pub y: i32,
    pub px: i32,
    pub py: i32,
    /// Walk progress 0..1 from (px, py) to (x, y).
    pub t: f32,
    pub dir: Dir,
    pub hp: i32,
    pub max_hp: i32,
    pub walk_ms: f32,
    /// Walking speed multiplier (`S2C::Speed`, 1 = normal).
    pub speed: f32,
    /// Visible equipment (official weapon/shield/helmet animation ids, 0 = none).
    pub weapon: u16,
    pub shield: u16,
    pub helmet: u16,
    /// `ao_protocol::status` bits.
    pub status: u32,
    /// Last thing it said (NPC speech) and when, for overhead text.
    pub said: Option<(String, f64)>,
}

impl Ent {
    #[allow(clippy::too_many_arguments)]
    pub fn new(name: String, kind: EntityKind, body: u16, head: u16, x: i32, y: i32, dir: Dir, hp: i32, max_hp: i32) -> Ent {
        Ent { name, kind, body, head, x, y, px: x, py: y, t: 1.0, dir, hp, max_hp, walk_ms: 0.0, speed: 1.0, weapon: 0, shield: 0, helmet: 0, status: 0, said: None }
    }

    /// Interpolated top-left of the character's tile, in world pixels.
    pub fn tile_tl(&self) -> Pt {
        let t = self.t.min(1.0);
        let x = self.px as f32 + (self.x - self.px) as f32 * t;
        let y = self.py as f32 + (self.y - self.py) as f32 * t;
        pt((x - 1.0) * TILE, (y - 1.0) * TILE)
    }

    /// Feet position in world pixels.
    pub fn world_pos(&self) -> Pt {
        self.tile_tl() + pt(16.0, 28.0)
    }

    pub fn moving(&self) -> bool {
        self.t < 1.0
    }

    pub fn snap(&mut self, x: i32, y: i32) {
        self.px = x;
        self.py = y;
        self.x = x;
        self.y = y;
        self.t = 1.0;
    }

    pub fn step_to(&mut self, x: i32, y: i32) {
        self.px = self.x;
        self.py = self.y;
        self.x = x;
        self.y = y;
        self.t = 0.0;
    }
}

pub fn tile_center(x: i32, y: i32) -> Pt {
    pt((x - 1) as f32 * TILE + 16.0, (y - 1) as f32 * TILE + 14.0)
}

/// Displayed stats, kept obfuscated in memory (see `guard`).
pub struct Stats {
    pub hp: Guarded,
    pub max_hp: Guarded,
    pub mana: Guarded,
    pub max_mana: Guarded,
    pub level: u32,
    pub exp: u32,
    pub next_exp: u32,
}

#[derive(PartialEq, Debug, Clone)]
pub enum Screen {
    /// Account login / new account (and the development login).
    Login,
    /// Account logged in: choose, delete or create a character (P3, Sonnet 1).
    Characters,
    /// Character creation (P3, Sonnet 1).
    Create,
    Game,
    Kicked(String),
}

pub struct Game {
    pub screen: Screen,
    pub name_input: String,
    pub server_addr: String,
    /// AO20 `Recursos` folder (maps are needed for walk prediction).
    pub recursos: PathBuf,
    pub net: Option<Net>,
    pub my_id: u32,
    pub map: Option<AoMap>,
    /// Server world clock: (ms into the day when received, day length in ms, `now` when it
    /// arrived). None until the server says (`S2C::WorldTime`).
    pub world_time: Option<(u64, u64, f64)>,
    /// Fixed objects of the map (doors, signs...): the map file plus what the server
    /// changed (`S2C::MapObject`). Tile -> obj.dat index.
    pub map_objects: HashMap<(i32, i32), u16>,
    /// obj.dat, loaded the first time a door changes (its `Subtipo` says what it blocks).
    objs: Option<std::sync::Arc<ao_formats::dat::Table<ao_formats::dat::obj::ObjData>>>,
    pub ents: HashMap<u32, Ent>,
    pub stats: Stats,
    pub spells: Vec<SpellDef>,
    pub selected: usize,
    pub casting: bool,
    pub seq: u32,
    /// Current time in seconds, set by the frontend (frame clock) or a bot (virtual clock).
    pub now: f64,
    pub last_step: f64,
    /// Local mirror of server cooldowns, so a legit player never triggers a strike.
    pub cd_until: HashMap<u16, f64>,
    pub pending: HashMap<u16, f64>,
    pub console: VecDeque<(String, Tone)>,
    pub chat: Option<String>,
    pub tampered: bool,
    /// Characters of the logged account (character selection screen).
    pub characters: Vec<CharacterSummary>,
    /// Visual events produced by `poll`, drained by the frontend.
    pub events: Vec<ClientEvent>,
    /// Own inventory, equipment, vitals, skills, spells, bank, ground items.
    pub local: Local,
}

impl Game {
    pub fn new(name: String, server_addr: String, recursos: PathBuf) -> Game {
        Game {
            screen: Screen::Login,
            name_input: name,
            server_addr,
            recursos,
            net: None,
            my_id: 0,
            map: None,
            world_time: None,
            map_objects: HashMap::new(),
            objs: None,
            ents: HashMap::new(),
            stats: Stats { hp: Guarded::new(0), max_hp: Guarded::new(1), mana: Guarded::new(0), max_mana: Guarded::new(1), level: 1, exp: 0, next_exp: 1 },
            spells: Vec::new(),
            selected: 0,
            casting: false,
            seq: 0,
            now: 0.0,
            // Far in the past, so the first step is never held back by the local pace.
            last_step: -1.0e9,
            cd_until: HashMap::new(),
            pending: HashMap::new(),
            console: VecDeque::new(),
            chat: None,
            tampered: false,
            characters: Vec::new(),
            events: Vec::new(),
            local: Local::new(),
        }
    }

    pub fn say(&mut self, text: impl Into<String>, tone: Tone) {
        self.console.push_back((text.into(), tone));
        while self.console.len() > 60 {
            self.console.pop_front();
        }
    }

    pub fn me(&self) -> Option<&Ent> {
        self.ents.get(&self.my_id)
    }

    pub fn occupied(&self, x: i32, y: i32) -> bool {
        self.ents.iter().any(|(id, e)| *id != self.my_id && e.x == x && e.y == y)
    }

    pub fn send(&mut self, msg: &C2S) {
        if let Some(n) = &mut self.net {
            n.send(msg);
        }
    }

    /// Opens the connection without logging in (accounts, or tests).
    pub fn open(&mut self) -> std::io::Result<()> {
        self.net = Some(Net::connect(&self.server_addr)?);
        Ok(())
    }

    /// Development login: plays `name_input` directly (`C2S::Login`).
    pub fn connect(&mut self) {
        match self.open() {
            Ok(()) => {
                let name = self.name_input.clone();
                self.send(&C2S::Login { name, version: PROTOCOL_VERSION });
                self.screen = Screen::Game;
            }
            Err(e) => {
                let msg = format!("No se pudo conectar a {}: {e}", self.server_addr);
                self.say(msg, Tone::Warn);
            }
        }
    }

    /// Reads the network and applies every message; returns them (bots log them). Visual
    /// consequences go to `events`.
    pub fn poll(&mut self) -> Vec<S2C> {
        let Some(net) = &mut self.net else { return Vec::new() };
        let (msgs, alive) = net.poll();
        for m in &msgs {
            self.handle(m.clone());
        }
        if !alive && self.screen == Screen::Game {
            self.screen = Screen::Kicked("Conexion cerrada por el servidor.".into());
        }
        msgs
    }

    /// Integrity check of the obfuscated stats (see `guard`).
    pub fn check_integrity(&mut self) {
        if !self.tampered && !(self.stats.hp.verify() && self.stats.mana.verify()) {
            self.tampered = true;
            self.screen = Screen::Kicked("Modificacion de memoria detectada.".into());
        }
    }

    fn load_map(&mut self, number: u16) {
        match AoMap::load(&self.recursos, number) {
            Ok(m) => {
                self.map_objects = ao_formats::dat::craft::map_objects(&self.recursos, number).into_iter().map(|o| ((o.x, o.y), o.obj)).collect();
                self.map = Some(m);
                self.events.push(ClientEvent::MapEntered { map: number });
            }
            Err(e) => self.say(format!("No se pudo cargar el mapa {number}: {e:?}"), Tone::Warn),
        }
    }

    /// A door opened or closed: redraw it and apply its blocks for walk prediction.
    fn on_map_object(&mut self, x: i32, y: i32, obj: u16) {
        if self.map_objects.get(&(x, y)) == Some(&obj) {
            return;
        }
        let recursos = self.recursos.clone();
        let objs = self.objs.get_or_insert_with(|| std::sync::Arc::new(ao_formats::dat::obj::load(&recursos).map(|l| l.data).unwrap_or_default())).clone();
        self.map_objects.insert((x, y), obj);
        if let (Some(d), Some(map)) = (objs.get(obj as u32), self.map.as_mut()) {
            if d.obj_type == ao_formats::dat::obj::ObjType::Doors {
                ao_rules::doors::apply_door_blocks(&mut map.blocked, x, y, d.subtype, d.locked == 1);
                // SND_PUERTA, or SND_PUERTA_DUCTO for the duct grates.
                let wav = if matches!(d.grh, 11444 | 11445 | 59877 | 59878) { 380 } else { 5 };
                self.events.push(ClientEvent::Sound { wav, at: (x, y) });
            }
        }
    }

    pub fn spell_by_id(&self, id: u16) -> Option<&SpellDef> {
        self.spells.iter().find(|s| s.id == id)
    }

    pub fn handle(&mut self, msg: S2C) {
        match msg {
            S2C::Welcome { id } => self.my_id = id,
            S2C::WorldTime { elapsed_ms, day_ms } => self.world_time = Some((elapsed_ms, day_ms.max(1), self.now)),
            S2C::MapObject { x, y, obj } => self.on_map_object(x, y, obj),
            S2C::Sound { wav, x, y } => self.events.push(ClientEvent::Sound { wav, at: (x, y) }),
            S2C::Speed { id, speed } => {
                if let Some(e) = self.ents.get_mut(&id) {
                    e.speed = speed.max(0.1);
                }
            }
            S2C::Music { midi } => self.events.push(ClientEvent::Music { midi }),
            S2C::SpellBook { spells } => {
                self.spells = spells;
                self.selected = self.selected.min(self.spells.len().saturating_sub(1));
            }
            S2C::EnterMap { map, x, y } => {
                self.local.on_map_change();
                if self.map.as_ref().map(|m| m.number) != Some(map) {
                    self.load_map(map);
                }
                // The previous map's entities are gone; the server resends this map's.
                let me = self.ents.remove(&self.my_id);
                self.ents.clear();
                let mut me = me.unwrap_or_else(|| Ent::new(self.name_input.clone(), EntityKind::Player, 1, 1, x, y, Dir::Down, 100, 100));
                me.snap(x, y);
                self.ents.insert(self.my_id, me);
            }
            S2C::Spawn { id, name, kind, body, head, x, y, dir, hp, max_hp } => {
                if let Some(e) = self.ents.get_mut(&id) {
                    e.name = name;
                    e.body = body;
                    e.head = head;
                    e.hp = hp;
                    e.max_hp = max_hp;
                    e.dir = dir;
                    return;
                }
                self.ents.insert(id, Ent::new(name, kind, body, head, x, y, dir, hp, max_hp));
                self.events.push(ClientEvent::Puff { at: tile_center(x, y) + pt(0.0, 10.0) });
            }
            S2C::Despawn { id } => {
                if let Some(e) = self.ents.remove(&id) {
                    if e.kind == EntityKind::Creature {
                        self.events.push(ClientEvent::DeathBurst { at: e.world_pos() });
                    }
                }
            }
            S2C::Moved { id, x, y, dir } => {
                if let Some(e) = self.ents.get_mut(&id) {
                    e.dir = dir;
                    if (e.x - x).abs() + (e.y - y).abs() == 1 {
                        e.step_to(x, y);
                    } else if (e.x, e.y) != (x, y) {
                        e.snap(x, y);
                    }
                }
            }
            S2C::Correct { x, y, .. } => {
                if let Some(e) = self.ents.get_mut(&self.my_id) {
                    e.snap(x, y);
                }
            }
            S2C::Stats { hp, max_hp, mana, max_mana, level, exp, next_exp } => {
                let s = &mut self.stats;
                s.hp.set(hp);
                s.max_hp.set(max_hp);
                s.mana.set(mana);
                s.max_mana.set(max_mana);
                s.level = level;
                s.exp = exp;
                s.next_exp = next_exp;
            }
            S2C::Effect { spell, from, tx, ty } => {
                // Spells outside my book (NPCs, other players) take their look from the profile.
                let effect = match self.spell_by_id(spell).cloned() {
                    Some(def) => {
                        if from == self.my_id {
                            self.cd_until.insert(spell, self.now + def.cooldown_ms as f64 / 1000.0 + 0.05);
                            self.pending.remove(&spell);
                        }
                        def.effect
                    }
                    None => match ao_formats::profile::for_recursos(&self.recursos).spell_fx.get(&(spell as u32)).and_then(|n| ao_rules::spells::SpellEffect::parse(n)) {
                        Some(e) => e,
                        None => return,
                    },
                };
                let to = tile_center(tx, ty);
                let from_pos = self.ents.get(&from).map(|e| e.world_pos() - pt(0.0, 24.0)).unwrap_or(to);
                self.events.push(ClientEvent::Effect { effect, from: from_pos, to });
            }
            S2C::Hp { id, hp, max_hp } => {
                if let Some(e) = self.ents.get_mut(&id) {
                    let diff = hp - e.hp;
                    if diff != 0 {
                        let p = e.world_pos() - pt(0.0, 56.0);
                        self.events.push(ClientEvent::FloatText { at: p, amount: diff });
                        if diff < 0 {
                            self.events.push(ClientEvent::Blood { at: p + pt(0.0, 30.0) });
                        }
                    }
                    e.hp = hp;
                    e.max_hp = max_hp;
                }
            }
            S2C::AccountResult { ok, message } => self.say(message, if ok { Tone::Good } else { Tone::Warn }),
            S2C::CharacterList { characters } => self.characters = characters,
            S2C::Console { text, color } => self.say(text, color),
            S2C::Kicked { reason } => self.screen = Screen::Kicked(reason),
            S2C::Look { id, body, head, weapon, shield, helmet } => {
                if let Some(e) = self.ents.get_mut(&id) {
                    (e.body, e.head, e.weapon, e.shield, e.helmet) = (body, head, weapon, shield, helmet);
                }
            }
            S2C::Status { id, flags } => {
                if let Some(e) = self.ents.get_mut(&id) {
                    e.status = flags;
                }
            }
            S2C::NpcSay { id, text } => {
                let name = self.ents.get(&id).map(|e| e.name.clone()).unwrap_or_default();
                if let Some(e) = self.ents.get_mut(&id) {
                    e.said = Some((text.clone(), self.now));
                }
                self.say(format!("{name}: {text}"), Tone::Chat);
            }
            S2C::Attack { target, hit, damage, .. } => {
                // Health itself arrives as S2C::Hp; this only marks misses for the frontend.
                if !hit {
                    if let Some(e) = self.ents.get(&target) {
                        self.events.push(ClientEvent::FloatText { at: e.world_pos() - pt(0.0, 56.0), amount: 0 });
                    }
                }
                let _ = damage;
            }
            S2C::Fx { .. } => {}
            other => {
                if !self.local.apply(&other) {
                    self.say(format!("(mensaje sin manejar: {other:?})"), Tone::Warn);
                }
            }
        }
    }

    /// Advances walk animations.
    pub fn update(&mut self, dt: f32) {
        for e in self.ents.values_mut() {
            if e.t < 1.0 {
                e.t += dt * 1000.0 * e.speed / ao_rules::WALK_MS as f32;
                // f32 sums like 0.35 + 0.65 can land just under 1: that step is done.
                if e.t > 0.9999 {
                    e.t = 1.0;
                }
                e.walk_ms += dt * 1000.0;
            } else {
                e.walk_ms = 0.0;
            }
        }
    }
}

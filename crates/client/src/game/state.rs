//! Entities, stats and the handling of every server message.
use crate::assets::Assets;
use crate::guard::Guarded;
use crate::net::Net;
use crate::render::fx::Fx;
use ao_formats::AoMap;
use ao_protocol::{CharacterSummary, EntityKind, Tone, C2S, PROTOCOL_VERSION, S2C};
use ao_rules::spells::SpellDef;
use ao_rules::Dir;
use macroquad::prelude::*;
use std::collections::{HashMap, VecDeque};

pub const TILE: f32 = 32.0;

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
}

impl Ent {
    #[allow(clippy::too_many_arguments)]
    pub fn new(name: String, kind: EntityKind, body: u16, head: u16, x: i32, y: i32, dir: Dir, hp: i32, max_hp: i32) -> Ent {
        Ent { name, kind, body, head, x, y, px: x, py: y, t: 1.0, dir, hp, max_hp, walk_ms: 0.0 }
    }

    /// Interpolated top-left of the character's tile, in world pixels.
    pub fn tile_tl(&self) -> Vec2 {
        let t = self.t.min(1.0);
        let x = self.px as f32 + (self.x - self.px) as f32 * t;
        let y = self.py as f32 + (self.y - self.py) as f32 * t;
        vec2((x - 1.0) * TILE, (y - 1.0) * TILE)
    }

    /// Feet position in world pixels.
    pub fn world_pos(&self) -> Vec2 {
        self.tile_tl() + vec2(16.0, 28.0)
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

pub fn tile_center(x: i32, y: i32) -> Vec2 {
    vec2((x - 1) as f32 * TILE + 16.0, (y - 1) as f32 * TILE + 14.0)
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

#[derive(PartialEq)]
pub enum Screen {
    Login,
    Game,
    Kicked(String),
}

pub struct Game {
    pub screen: Screen,
    pub name_input: String,
    pub server_addr: String,
    pub net: Option<Net>,
    pub my_id: u32,
    pub map: Option<AoMap>,
    pub ents: HashMap<u32, Ent>,
    pub stats: Stats,
    pub spells: Vec<SpellDef>,
    pub selected: usize,
    pub casting: bool,
    pub seq: u32,
    pub last_step: f64,
    /// Local mirror of server cooldowns, so a legit player never triggers a strike.
    pub cd_until: HashMap<u16, f64>,
    pub pending: HashMap<u16, f64>,
    pub console: VecDeque<(String, Color)>,
    pub chat: Option<String>,
    pub tampered: bool,
    /// Characters of the logged account (character selection screen).
    pub characters: Vec<CharacterSummary>,
}

pub fn tone_color(t: Tone) -> Color {
    match t {
        Tone::Combat => Color::new(1.0, 0.45, 0.35, 1.0),
        Tone::Warn => Color::new(1.0, 0.85, 0.3, 1.0),
        Tone::Chat => WHITE,
        Tone::Good => Color::new(0.45, 1.0, 0.5, 1.0),
        Tone::Info => Color::new(0.65, 0.8, 1.0, 1.0),
    }
}

impl Game {
    pub fn new(name: String, server_addr: String) -> Game {
        Game {
            screen: Screen::Login,
            name_input: name,
            server_addr,
            net: None,
            my_id: 0,
            map: None,
            ents: HashMap::new(),
            stats: Stats { hp: Guarded::new(0), max_hp: Guarded::new(1), mana: Guarded::new(0), max_mana: Guarded::new(1), level: 1, exp: 0, next_exp: 1 },
            spells: Vec::new(),
            selected: 0,
            casting: false,
            seq: 0,
            last_step: 0.0,
            cd_until: HashMap::new(),
            pending: HashMap::new(),
            console: VecDeque::new(),
            chat: None,
            tampered: false,
            characters: Vec::new(),
        }
    }

    pub fn say(&mut self, text: impl Into<String>, tone: Tone) {
        self.console.push_back((text.into(), tone_color(tone)));
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

    pub fn connect(&mut self) {
        let name = self.name_input.clone();
        match Net::connect(&self.server_addr) {
            Ok(mut n) => {
                n.send(&C2S::Login { name, version: PROTOCOL_VERSION });
                self.net = Some(n);
                self.screen = Screen::Game;
            }
            Err(e) => {
                let msg = format!("No se pudo conectar a {}: {e}", self.server_addr);
                self.say(msg, Tone::Warn);
            }
        }
    }

    /// Reads the network and applies every message. Returns false if the link dropped.
    pub fn poll(&mut self, assets: &mut Assets, fx: &mut Fx) {
        let Some(net) = &mut self.net else { return };
        let (msgs, alive) = net.poll();
        for m in msgs {
            self.handle(m, assets, fx);
        }
        if !alive && self.screen == Screen::Game {
            self.screen = Screen::Kicked("Conexion cerrada por el servidor.".into());
        }
    }

    /// Integrity check of the obfuscated stats (see `guard`).
    pub fn check_integrity(&mut self) {
        if !self.tampered && !(self.stats.hp.verify() && self.stats.mana.verify()) {
            self.tampered = true;
            self.screen = Screen::Kicked("Modificacion de memoria detectada.".into());
        }
    }

    fn load_map(&mut self, number: u16, assets: &mut Assets) {
        match AoMap::load(&assets.root, number) {
            Ok(m) => {
                let files = assets.preload_map(&m);
                if std::env::var("AO_DEBUG").is_ok() {
                    eprintln!("map {number} '{}' textures={files}", m.name);
                }
                self.map = Some(m);
            }
            Err(e) => self.say(format!("No se pudo cargar el mapa {number}: {e:?}"), Tone::Warn),
        }
    }

    pub fn spell_by_id(&self, id: u16) -> Option<&SpellDef> {
        self.spells.iter().find(|s| s.id == id)
    }

    fn handle(&mut self, msg: S2C, assets: &mut Assets, fx: &mut Fx) {
        match msg {
            S2C::Welcome { id } => self.my_id = id,
            S2C::SpellBook { spells } => {
                self.spells = spells;
                self.selected = self.selected.min(self.spells.len().saturating_sub(1));
            }
            S2C::EnterMap { map, x, y } => {
                if self.map.as_ref().map(|m| m.number) != Some(map) {
                    self.load_map(map, assets);
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
                fx.spawn_puff(tile_center(x, y) + vec2(0.0, 10.0));
            }
            S2C::Despawn { id } => {
                if let Some(e) = self.ents.remove(&id) {
                    if e.kind == EntityKind::Creature {
                        fx.death_burst(e.world_pos());
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
                let Some(def) = self.spell_by_id(spell).cloned() else { return };
                if from == self.my_id {
                    self.cd_until.insert(spell, get_time() + def.cooldown_ms as f64 / 1000.0 + 0.05);
                    self.pending.remove(&spell);
                }
                let to = tile_center(tx, ty);
                let from_pos = self.ents.get(&from).map(|e| e.world_pos() - vec2(0.0, 24.0)).unwrap_or(to);
                fx.cast(def.effect, from_pos, to);
            }
            S2C::Hp { id, hp, max_hp } => {
                if let Some(e) = self.ents.get_mut(&id) {
                    let diff = hp - e.hp;
                    if diff != 0 {
                        let p = e.world_pos() - vec2(0.0, 56.0);
                        let c = if diff < 0 { Color::new(1.0, 0.3, 0.25, 1.0) } else { Color::new(0.4, 1.0, 0.45, 1.0) };
                        fx.float_text(p, if diff < 0 { format!("{diff}") } else { format!("+{diff}") }, c);
                        if diff < 0 {
                            fx.blood(p + vec2(0.0, 30.0));
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
        }
    }

    /// Advances walk animations.
    pub fn update(&mut self, dt: f32) {
        for e in self.ents.values_mut() {
            if e.t < 1.0 {
                e.t = (e.t + dt * 1000.0 / ao_rules::WALK_MS as f32).min(1.0);
                e.walk_ms += dt * 1000.0;
            } else {
                e.walk_ms = 0.0;
            }
        }
    }
}

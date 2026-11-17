//! The world actor: owns all game state and processes events one at a time, so there are
//! no locks and no data races. Gameplay lives in `crate::systems` as `impl World` blocks.
pub mod entities;
pub mod instance;

use crate::config::ServerConfig;
use crate::data::GameData;
use crate::db::{Account, Db};
use crate::net::{ConnId, Event, CLOSE};
use crate::rng::Rng;
use ao_formats::AoMap;
use ao_protocol::{encode, Tone, S2C};
use entities::{EntityId, Player};
use instance::MapInstance;
use std::collections::HashMap;
use std::path::PathBuf;
use std::time::{Duration, Instant};
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};
use tracing::{info, warn};

/// First id given to creatures, so they never collide with connection ids.
pub const NPC_ID_BASE: EntityId = 100_000;

pub struct Session {
    pub tx: UnboundedSender<Vec<u8>>,
    pub player: Option<Player>,
    /// Logged account (character selection or playing).
    pub account: Option<Account>,
    pub failed_logins: u32,
}

impl Session {
    pub fn new(tx: UnboundedSender<Vec<u8>>) -> Session {
        Session { tx, player: None, account: None, failed_logins: 0 }
    }
}

pub struct World {
    pub cfg: ServerConfig,
    pub data: GameData,
    pub recursos: PathBuf,
    /// Where new characters appear: data/balance.toml, or `$AO_START="map,x,y"` for tests.
    pub start: (u16, i32, i32),
    pub sessions: HashMap<ConnId, Session>,
    pub maps: HashMap<u16, MapInstance>,
    pub rng: Rng,
    pub next_npc: EntityId,
    pub db: Db,
}

impl World {
    pub fn new(cfg: ServerConfig, data: GameData, recursos: PathBuf) -> World {
        let b = &data.balance;
        let start = std::env::var("AO_START")
            .ok()
            .and_then(|s| {
                let v: Vec<i32> = s.split(',').filter_map(|p| p.trim().parse().ok()).collect();
                (v.len() == 3).then(|| (v[0] as u16, v[1], v[2]))
            })
            .unwrap_or((b.start_map, b.start_x, b.start_y));
        let db = Db::open(&cfg.database).unwrap_or_else(|e| panic!("no se pudo abrir la base de datos '{}': {e:?}", cfg.database));
        World { cfg, data, recursos, start, sessions: HashMap::new(), maps: HashMap::new(), rng: Rng::new(0xA0_2026), next_npc: NPC_ID_BASE, db }
    }

    // ------------------------------------------------------------ lookups

    pub fn player(&self, id: ConnId) -> Option<&Player> {
        self.sessions.get(&id).and_then(|s| s.player.as_ref())
    }

    /// For ids that the dispatcher already checked are logged in.
    pub fn player_mut(&mut self, id: ConnId) -> &mut Player {
        self.sessions.get_mut(&id).and_then(|s| s.player.as_mut()).expect("logged player")
    }

    pub fn instance(&self, map: u16) -> &MapInstance {
        &self.maps[&map]
    }

    pub fn occupied(&self, map: u16, x: i32, y: i32) -> bool {
        let inst = &self.maps[&map];
        inst.npc_at(x, y).is_some() || inst.players.iter().any(|pid| self.player(*pid).is_some_and(|p| p.x == x && p.y == y))
    }

    pub fn player_at(&self, map: u16, x: i32, y: i32) -> Option<ConnId> {
        self.maps[&map].players.iter().copied().find(|pid| self.player(*pid).is_some_and(|p| p.x == x && p.y == y))
    }

    // ------------------------------------------------------------ output

    pub fn send(&self, id: ConnId, msg: &S2C) {
        if let Some(s) = self.sessions.get(&id) {
            let _ = s.tx.send(encode(msg));
        }
    }

    /// Everyone on `map` (encoded once, sent to each).
    pub fn broadcast(&self, map: u16, msg: &S2C, except: Option<ConnId>) {
        let Some(inst) = self.maps.get(&map) else { return };
        let bytes = encode(msg);
        for id in &inst.players {
            if Some(*id) != except {
                if let Some(s) = self.sessions.get(id) {
                    let _ = s.tx.send(bytes.clone());
                }
            }
        }
    }

    /// Every logged player in the world.
    pub fn broadcast_all(&self, msg: &S2C, except: Option<ConnId>) {
        let bytes = encode(msg);
        for (id, s) in &self.sessions {
            if s.player.is_some() && Some(*id) != except {
                let _ = s.tx.send(bytes.clone());
            }
        }
    }

    pub fn console(&self, id: ConnId, text: impl Into<String>, tone: Tone) {
        self.send(id, &S2C::Console { text: text.into(), color: tone });
    }

    pub fn close(&self, id: ConnId) {
        if let Some(s) = self.sessions.get(&id) {
            let _ = s.tx.send(CLOSE);
        }
    }

    // ------------------------------------------------------------ maps

    /// Loads a map on first use and populates it if it is not a safe zone.
    pub fn ensure_map(&mut self, map: u16) -> bool {
        if self.maps.contains_key(&map) {
            return true;
        }
        match AoMap::load(&self.recursos, map) {
            Ok(m) => {
                info!(map, name = %m.name, safe = m.safe, "mapa cargado");
                let safe = m.safe;
                self.maps.insert(map, MapInstance::new(m));
                if !safe {
                    self.spawn_creatures(map);
                }
                true
            }
            Err(e) => {
                warn!(map, ?e, "no se pudo cargar el mapa");
                false
            }
        }
    }

    /// Closest free standable tile, spiralling out from (cx, cy).
    pub fn free_tile_near(&self, map: u16, cx: i32, cy: i32, radius: i32) -> (i32, i32) {
        let m = &self.maps[&map].map;
        for r in 0..=radius.max(1) * 3 {
            for dy in -r..=r {
                for dx in -r..=r {
                    if dx.abs().max(dy.abs()) != r {
                        continue;
                    }
                    let (x, y) = (cx + dx, cy + dy);
                    if ao_rules::walk::standable(m, x, y) && !self.occupied(map, x, y) {
                        return (x, y);
                    }
                }
            }
        }
        (cx, cy)
    }

    // ------------------------------------------------------------ events

    pub fn on_event(&mut self, ev: Event) {
        match ev {
            Event::Connected { id, tx } => {
                self.sessions.insert(id, Session::new(tx));
            }
            Event::Frame { id, msg } => {
                if self.sessions.contains_key(&id) {
                    self.handle(id, msg);
                }
            }
            Event::Violation { id, reason } => self.kick(id, reason),
            Event::Disconnected { id } => self.logout(id),
        }
    }

    pub fn tick(&mut self, now: Instant, regen_due: bool) {
        self.tick_npcs(now);
        if regen_due {
            self.regen();
        }
    }
}

/// Runs the world actor until the event channel closes.
pub async fn run(mut world: World, mut rx: UnboundedReceiver<Event>) {
    let start_map = world.start.0;
    world.ensure_map(start_map);
    let mut tick = tokio::time::interval(Duration::from_millis(world.cfg.world.tick_ms));
    let mut last_regen = Instant::now();
    loop {
        tokio::select! {
            ev = rx.recv() => match ev {
                Some(ev) => world.on_event(ev),
                None => break,
            },
            _ = tick.tick() => {
                let now = Instant::now();
                let regen_due = now.duration_since(last_regen) >= Duration::from_secs(1);
                if regen_due {
                    last_regen = now;
                }
                world.tick(now, regen_due);
            }
        }
    }
}

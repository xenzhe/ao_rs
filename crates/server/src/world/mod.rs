//! The world actor: owns all game state and processes events one at a time, so there are
//! no locks and no data races. Gameplay lives in `crate::systems` as `impl World` blocks.
//!
//! Time comes from [`World::now`] (the world's [`Clock`]): real time in production, or a
//! virtual clock that only moves when a test sends [`Control::Advance`].
pub mod entities;
pub mod instance;

use crate::clock::{Clock, GameTime, VirtualClock};
use crate::config::ServerConfig;
use crate::data::GameData;
use crate::net::{ConnId, Event, CLOSE};
use crate::probe::Probe;
use crate::rng::Rng;
use crate::Settings;
use ao_formats::AoMap;
use ao_protocol::{encode, Tone, S2C};
use entities::{EntityId, Player};
use instance::MapInstance;
use std::collections::HashMap;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};
use tokio::sync::oneshot;
use tracing::{info, warn};

/// First id given to creatures, so they never collide with connection ids.
pub const NPC_ID_BASE: EntityId = 100_000;
/// Regeneration period.
pub const REGEN_MS: u64 = 1000;

pub struct Session {
    pub tx: UnboundedSender<Vec<u8>>,
    pub addr: SocketAddr,
    pub player: Option<Player>,
}

/// Commands from a test driving a virtual clock.
pub enum Control {
    /// Runs every tick due in the next `ms` of game time, then answers with the new time.
    Advance { ms: u64, done: oneshot::Sender<GameTime> },
}

pub struct World {
    pub cfg: ServerConfig,
    pub data: GameData,
    pub recursos: PathBuf,
    /// Where new characters appear: data/balance.toml or `Settings::start`.
    pub start: (u16, i32, i32),
    pub sessions: HashMap<ConnId, Session>,
    pub maps: HashMap<u16, MapInstance>,
    pub rng: Rng,
    pub next_npc: EntityId,
    pub clock: Clock,
    pub next_tick: GameTime,
    pub next_regen: GameTime,
    pub probe: Option<Arc<Probe>>,
    /// Connection addresses (only kept when probing), alive until `Disconnected`.
    addrs: HashMap<ConnId, SocketAddr>,
}

impl World {
    pub fn new(settings: Settings) -> World {
        let Settings { config, data, recursos, start, seed, clock, probe } = settings;
        let b = &data.balance;
        let start = start.unwrap_or((b.start_map, b.start_x, b.start_y));
        World {
            cfg: config,
            data,
            recursos,
            start,
            sessions: HashMap::new(),
            maps: HashMap::new(),
            rng: Rng::new(seed),
            next_npc: NPC_ID_BASE,
            clock,
            next_tick: GameTime::ZERO,
            next_regen: GameTime(REGEN_MS),
            probe,
            addrs: HashMap::new(),
        }
    }

    /// Current game time. Constant while one event is processed under a virtual clock.
    pub fn now(&self) -> GameTime {
        self.clock.now()
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

    /// Logged player ids in a stable order (determinism: HashMap order is random).
    pub fn player_ids(&self) -> Vec<ConnId> {
        let mut ids: Vec<ConnId> = self.sessions.iter().filter(|(_, s)| s.player.is_some()).map(|(id, _)| *id).collect();
        ids.sort_unstable();
        ids
    }

    // ------------------------------------------------------------ output

    fn push(&self, s: &Session, bytes: Vec<u8>) {
        if let Some(p) = &self.probe {
            p.update(s.addr, |c| c.bytes_out += bytes.len() as u64);
        }
        let _ = s.tx.send(bytes);
    }

    pub fn send(&self, id: ConnId, msg: &S2C) {
        if let Some(s) = self.sessions.get(&id) {
            self.push(s, encode(msg));
        }
    }

    /// Everyone on `map` (encoded once, sent to each).
    pub fn broadcast(&self, map: u16, msg: &S2C, except: Option<ConnId>) {
        let Some(inst) = self.maps.get(&map) else { return };
        let bytes = encode(msg);
        for id in &inst.players {
            if Some(*id) != except {
                if let Some(s) = self.sessions.get(id) {
                    self.push(s, bytes.clone());
                }
            }
        }
    }

    /// Every logged player in the world.
    pub fn broadcast_all(&self, msg: &S2C, except: Option<ConnId>) {
        let bytes = encode(msg);
        for (id, s) in &self.sessions {
            if s.player.is_some() && Some(*id) != except {
                self.push(s, bytes.clone());
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
            Event::Connected { id, addr, tx } => {
                if self.probe.is_some() {
                    self.addrs.insert(id, addr);
                    // Known from now on, even before it sends anything.
                    self.probe_conn(id, |_| {});
                }
                self.sessions.insert(id, Session { tx, addr, player: None });
            }
            Event::Frame { id, msg } => {
                if self.sessions.contains_key(&id) {
                    self.handle(id, msg);
                }
                self.probe_conn(id, |c| c.frames_in += 1);
            }
            Event::Violation { id, reason } => self.kick(id, reason),
            Event::Disconnected { id } => {
                self.logout(id);
                self.probe_conn(id, |c| c.closed = true);
                self.addrs.remove(&id);
            }
        }
    }

    fn probe_conn(&self, id: ConnId, f: impl FnOnce(&mut crate::probe::ConnProbe)) {
        if let (Some(p), Some(addr)) = (&self.probe, self.addrs.get(&id)) {
            p.update(*addr, f);
        }
    }

    /// One world tick at game time `now`: creatures, and regeneration when due.
    pub fn tick(&mut self, now: GameTime) {
        self.tick_npcs(now);
        if now >= self.next_regen {
            self.next_regen = now.after_ms(REGEN_MS);
            self.regen();
        }
    }

    /// Virtual clock: runs every tick whose time falls in `(now, now + ms]`, in order,
    /// and leaves the clock exactly at `now + ms`.
    fn advance(&mut self, clock: &VirtualClock, ms: u64) -> GameTime {
        let target = clock.now().after_ms(ms);
        let step = self.cfg.world.tick_ms.max(1);
        while self.next_tick <= target {
            let t = self.next_tick;
            clock.set(t);
            self.tick(t);
            self.next_tick = t.after_ms(step);
        }
        clock.set(target);
        target
    }
}

/// Runs the world actor until the event channel closes. With a real clock it ticks every
/// `tick_ms`; with a virtual clock it only ticks when `control` says so.
pub async fn run(mut world: World, mut rx: UnboundedReceiver<Event>, control: Option<UnboundedReceiver<Control>>) {
    let start_map = world.start.0;
    world.ensure_map(start_map);
    match (world.clock.clone(), control) {
        (Clock::Virtual(clock), Some(mut control)) => {
            world.next_tick = clock.now().after_ms(world.cfg.world.tick_ms.max(1));
            loop {
                tokio::select! {
                    biased;
                    ev = rx.recv() => match ev {
                        Some(ev) => world.on_event(ev),
                        None => break,
                    },
                    c = control.recv() => match c {
                        Some(Control::Advance { ms, done }) => {
                            let t = world.advance(&clock, ms);
                            let _ = done.send(t);
                        }
                        None => break,
                    },
                }
            }
        }
        _ => {
            let mut tick = tokio::time::interval(Duration::from_millis(world.cfg.world.tick_ms.max(1)));
            loop {
                tokio::select! {
                    ev = rx.recv() => match ev {
                        Some(ev) => world.on_event(ev),
                        None => break,
                    },
                    _ = tick.tick() => {
                        let now = world.now();
                        world.tick(now);
                    }
                }
            }
        }
    }
}

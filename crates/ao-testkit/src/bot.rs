//! A scripted client on top of `client-core`: the same state, prediction and message
//! handling as the real game client, with no window.
use crate::{Shared, WAIT_CAP};
use ao_protocol::{EntityKind, C2S, PROTOCOL_VERSION, S2C};
use ao_rules::{walk, Dir};
use client_core::{ClientEvent, Ent, Game, Screen};
use server::clock::GameTime;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{Duration, Instant};

pub struct Bot {
    /// The client state, exactly as the game client would hold it.
    pub game: Game,
    shared: Arc<Shared>,
    addr: SocketAddr,
    /// Frames sent (for the server barrier).
    sent: u64,
    /// Every message received, in order.
    log: Vec<S2C>,
    /// Every visual event the client produced, in order.
    events: Vec<ClientEvent>,
    clock: GameTime,
}

impl Bot {
    pub(crate) fn connect(shared: Arc<Shared>) -> Bot {
        let mut game = Game::new(String::new(), shared.addr.clone(), shared.recursos.clone());
        game.open().expect("connect to the test server");
        let addr = game.net.as_ref().and_then(|n| n.local_addr()).expect("local address");
        shared.sent.lock().unwrap().insert(addr, 0);
        let clock = shared.clock.now();
        game.now = clock.secs_f64();
        Bot { game, shared, addr, sent: 0, log: Vec::new(), events: Vec::new(), clock }
    }

    // ------------------------------------------------------------ session

    /// Development login (`C2S::Login`) and wait until standing on a map. Account login
    /// (`C2S::AccountLogin` + `PlayCharacter`) will sit next to this as `play(user, pass,
    /// name)`: same shape, send then `wait_until` in the map.
    pub fn login(&mut self, name: &str) -> &mut Self {
        self.game.name_input = name.to_string();
        self.send(&C2S::Login { name: name.into(), version: PROTOCOL_VERSION });
        self.game.screen = Screen::Game;
        self.wait_until(&format!("login de {name}"), |b| b.in_map() || b.closed());
        assert!(self.in_map(), "login de {name} rechazado: {:?}", self.kicked());
        self.sync();
        self
    }

    /// Logged in, map loaded and own entity known.
    pub fn in_map(&self) -> bool {
        self.game.map.is_some() && self.game.my_id != 0 && self.game.me().is_some()
    }

    // ------------------------------------------------------------ accounts

    fn account_results(&self) -> usize {
        self.log.iter().filter(|m| matches!(m, S2C::AccountResult { .. })).count()
    }

    /// Sends an account message and waits for its `AccountResult`; returns (ok, message).
    pub fn account_request(&mut self, msg: &C2S) -> (bool, String) {
        let before = self.account_results();
        self.send(msg);
        self.wait_until("respuesta de cuenta", |b| b.account_results() > before || b.closed());
        self.sync();
        self.log
            .iter()
            .rev()
            .find_map(|m| if let S2C::AccountResult { ok, message } = m { Some((*ok, message.clone())) } else { None })
            .filter(|_| self.account_results() > before)
            .unwrap_or((false, "conexion cerrada".into()))
    }

    pub fn create_account(&mut self, user: &str, pass: &str) -> bool {
        self.account_request(&C2S::CreateAccount { user: user.into(), password: pass.into(), version: PROTOCOL_VERSION }).0
    }

    pub fn account_login(&mut self, user: &str, pass: &str) -> bool {
        self.account_request(&C2S::AccountLogin { user: user.into(), password: pass.into(), version: PROTOCOL_VERSION }).0
    }

    pub fn create_character(&mut self, name: &str, race: u8, class: u8, gender: u8, head: u16) -> bool {
        self.account_request(&C2S::CreateCharacter { name: name.into(), race, class, gender, head }).0
    }

    /// Characters of the logged account, as the selection screen shows them.
    pub fn characters(&self) -> &[ao_protocol::CharacterSummary] {
        &self.game.characters
    }

    /// Plays a character of the logged account and waits until standing on its map.
    pub fn play(&mut self, name: &str) -> &mut Self {
        self.game.name_input = name.to_string();
        self.send(&C2S::PlayCharacter { name: name.into() });
        self.game.screen = Screen::Game;
        self.wait_until(&format!("entrar con {name}"), |b| b.in_map() || b.closed());
        assert!(self.in_map(), "no pudo entrar con {name}: {:?}", self.kicked());
        self.sync();
        self
    }

    // ------------------------------------------------------------ sending

    /// Any message, untouched (cheat tests, account flows).
    pub fn send(&mut self, msg: &C2S) {
        self.pump();
        self.game.send(msg);
        self.count_sent();
    }

    /// Raw bytes, not counted as a frame (garbage, oversized headers).
    pub fn send_bytes(&mut self, bytes: &[u8]) {
        if let Some(n) = &mut self.game.net {
            n.send_bytes(bytes);
        }
    }

    fn count_sent(&mut self) {
        self.sent += 1;
        self.shared.sent.lock().unwrap().insert(self.addr, self.sent);
    }

    // ------------------------------------------------------------ actions

    /// One predicted step, as the client does when an arrow key is held: refused locally
    /// (false) if still animating, too soon, or blocked.
    pub fn walk(&mut self, dir: Dir) -> bool {
        self.pump();
        let now = self.game.now;
        let sent = self.game.try_step(dir, now);
        if sent {
            self.count_sent();
        }
        sent
    }

    /// A step with no local checks (what a cheat would send).
    pub fn walk_raw(&mut self, dir: Dir) {
        let seq = self.game.next_seq();
        self.send(&C2S::Walk { dir, seq });
    }

    /// Selects `spell` and casts it at a tile through the client's own checks
    /// (cooldown, mana, range). False if the client refused locally.
    pub fn cast(&mut self, spell: u16, tx: i32, ty: i32) -> bool {
        self.pump();
        let Some(i) = self.game.spells.iter().position(|s| s.id == spell) else { return false };
        self.game.selected = i;
        let sent = self.game.cast_at(tx, ty);
        if sent {
            self.count_sent();
        }
        sent
    }

    /// A cast with no local checks.
    pub fn cast_raw(&mut self, spell: u16, tx: i32, ty: i32) {
        let seq = self.game.next_seq();
        self.send(&C2S::Cast { spell, tx, ty, seq });
    }

    pub fn say(&mut self, text: &str) {
        self.send(&C2S::Say { text: text.into() });
    }

    // ------------------------------------------------------------ receiving and waiting

    /// Applies the game time to the client (animations, local cooldowns) and reads the
    /// network. Called by every action and wait.
    pub fn pump(&mut self) {
        let now = self.shared.clock.now();
        let dt = now.since(self.clock) as f32 / 1000.0;
        self.clock = now;
        self.game.now = now.secs_f64();
        if dt > 0.0 {
            self.game.update(dt);
        }
        let msgs = self.game.poll();
        self.log.extend(msgs);
        self.events.append(&mut self.game.events);
    }

    /// Pumps until `cond` holds; panics after [`WAIT_CAP`] with `what`.
    pub fn wait_until(&mut self, what: &str, cond: impl Fn(&Bot) -> bool) -> &mut Self {
        if !self.try_wait_until(WAIT_CAP, cond) {
            let tail: Vec<_> = self.log.iter().rev().take(8).collect();
            panic!("{}: no paso '{what}' en {WAIT_CAP:?}; cerrado={}, ultimos mensajes: {tail:?}", self.name(), self.closed());
        }
        self
    }

    /// Pumps until `cond` holds or `cap` passes; returns whether it held.
    pub fn try_wait_until(&mut self, cap: Duration, cond: impl Fn(&Bot) -> bool) -> bool {
        let deadline = Instant::now() + cap;
        loop {
            self.pump();
            if cond(self) {
                return true;
            }
            if Instant::now() >= deadline {
                return false;
            }
            std::thread::sleep(Duration::from_micros(200));
        }
    }

    /// Exact barrier: the server consumed every frame this bot sent, and the bot received
    /// every byte the server queued for it. After this, "nothing happened" is a valid
    /// assertion.
    pub fn sync(&mut self) -> &mut Self {
        let (shared, addr, sent) = (self.shared.clone(), self.addr, self.sent);
        self.wait_until("sync con el servidor", move |b| {
            let Some(p) = shared.probe.get(addr) else { return false };
            let consumed = p.closed || p.frames_in >= sent;
            let net = b.game.net.as_ref();
            let received = net.is_none_or(|n| !n.alive() || n.bytes_in >= p.bytes_out);
            consumed && received
        })
    }

    // ------------------------------------------------------------ what the client sees

    pub fn name(&self) -> &str {
        &self.game.name_input
    }

    pub fn id(&self) -> u32 {
        self.game.my_id
    }

    pub fn me(&self) -> &Ent {
        self.game.me().expect("bot is in a map")
    }

    pub fn pos(&self) -> (i32, i32) {
        let e = self.me();
        (e.x, e.y)
    }

    pub fn map(&self) -> u16 {
        self.game.map.as_ref().map_or(0, |m| m.number)
    }

    pub fn hp(&self) -> i32 {
        self.game.stats.hp.get()
    }

    pub fn mana(&self) -> i32 {
        self.game.stats.mana.get()
    }

    pub fn level(&self) -> u32 {
        self.game.stats.level
    }

    /// Another entity as this client sees it.
    pub fn ent(&self, id: u32) -> Option<&Ent> {
        self.game.ents.get(&id)
    }

    pub fn creatures(&self) -> impl Iterator<Item = (u32, &Ent)> {
        self.game.ents.iter().filter(|(_, e)| e.kind == EntityKind::Creature).map(|(id, e)| (*id, e))
    }

    /// Every message received so far.
    pub fn log(&self) -> &[S2C] {
        &self.log
    }

    pub fn count(&self, pred: impl Fn(&S2C) -> bool) -> usize {
        self.log.iter().filter(|m| pred(m)).count()
    }

    /// Forgets the received log (to assert on what comes next).
    pub fn clear_log(&mut self) {
        self.log.clear();
        self.events.clear();
    }

    pub fn events(&self) -> &[ClientEvent] {
        &self.events
    }

    pub fn console(&self) -> impl Iterator<Item = &str> {
        self.game.console.iter().map(|(t, _)| t.as_str())
    }

    /// The link is down (kicked, or closed by either side).
    pub fn closed(&self) -> bool {
        self.game.net.as_ref().is_none_or(|n| !n.alive())
    }

    /// The reason, if the server kicked this bot.
    pub fn kicked(&self) -> Option<&str> {
        self.log.iter().rev().find_map(|m| if let S2C::Kicked { reason } = m { Some(reason.as_str()) } else { None })
    }

    /// A direction with `steps` free tiles ahead (no walls, exits or entities), on the
    /// map the client has loaded.
    pub fn free_dir(&self, steps: i32) -> Option<Dir> {
        let map = self.game.map.as_ref()?;
        let (x, y) = self.pos();
        [Dir::Right, Dir::Left, Dir::Down, Dir::Up].into_iter().find(|d| {
            let (dx, dy) = d.delta();
            (1..=steps).all(|k| {
                let (tx, ty) = (x + k * dx, y + k * dy);
                walk::can_enter(map, tx, ty, *d) && map.exit(tx, ty).is_none() && !self.game.occupied(tx, ty)
            })
        })
    }
}

impl Drop for Bot {
    fn drop(&mut self) {
        self.shared.sent.lock().unwrap().remove(&self.addr);
    }
}

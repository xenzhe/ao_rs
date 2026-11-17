//! Headless test bench.
//!
//! - [`TestServer`]: the real server in this process, on a free port, with a fixed seed
//!   and a **virtual clock**: game time only moves with [`TestServer::advance`], which runs
//!   every due tick instantly (AI, regeneration, respawns, cooldowns, walk pace).
//! - [`Bot`]: a scripted client built on `client-core` (the same logic as the game client):
//!   it walks with prediction, casts, chats, and exposes what its client sees.
//!
//! Nothing waits for real time to make game time pass. Waits are on conditions, with a
//! cap, and [`Bot::sync`] is an exact barrier (the server consumed everything the bot sent
//! and the bot received everything the server queued for it).
//!
//! ```no_run
//! use ao_testkit::TestServer;
//! let s = TestServer::start();
//! let mut ana = s.bot("Ana");
//! let mut beto = s.bot("Beto");
//! let dir = beto.free_dir(1).unwrap();
//! assert!(beto.walk(dir));
//! let (id, to) = (beto.id(), beto.pos());
//! ana.wait_until("Ana ve caminar a Beto", |b| b.ent(id).is_some_and(|e| (e.x, e.y) == to));
//! s.advance(3000);
//! ```
mod bot;

pub use bot::Bot;
pub use client_core::ClientEvent;
pub use server::clock::GameTime;

use server::clock::{Clock, VirtualClock};
use server::probe::Probe;
use server::world::Control;
use server::Settings;
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::runtime::Runtime;
use tokio::sync::mpsc::UnboundedSender;

/// Cap for every condition wait (real time). Only reached when a test is failing.
pub const WAIT_CAP: Duration = Duration::from_secs(5);

/// State shared by a server and its bots.
pub(crate) struct Shared {
    pub addr: String,
    pub recursos: std::path::PathBuf,
    pub clock: VirtualClock,
    pub probe: Arc<Probe>,
    /// Frames each live bot has sent, by its local address.
    pub sent: Mutex<HashMap<SocketAddr, u64>>,
}

impl Shared {
    /// True when the server has consumed every frame `addr` sent (or the link ended).
    pub fn consumed(&self, addr: SocketAddr, sent: u64) -> bool {
        self.probe.get(addr).is_some_and(|p| p.closed || p.frames_in >= sent)
    }
}

pub struct TestServer {
    shared: Arc<Shared>,
    control: UnboundedSender<Control>,
    data: server::data::GameData,
    start: (u16, i32, i32),
    // Last: dropping the runtime stops the server.
    rt: Runtime,
}

impl TestServer {
    /// Repository defaults on a virtual clock.
    pub fn start() -> TestServer {
        Self::with(|_| {})
    }

    /// New characters appear near this tile.
    pub fn start_at(map: u16, x: i32, y: i32) -> TestServer {
        Self::with(|s| s.start = Some((map, x, y)))
    }

    /// Defaults, then `tweak` (config, data, start, seed...). The clock is always virtual.
    pub fn with(tweak: impl FnOnce(&mut Settings)) -> TestServer {
        let mut settings = Settings::defaults();
        tweak(&mut settings);
        let clock = VirtualClock::new();
        let probe = Arc::new(Probe::default());
        settings.clock = Clock::Virtual(clock.clone());
        settings.probe = Some(probe.clone());
        let data = settings.data.clone();
        let b = &data.balance;
        let start = settings.start.unwrap_or((b.start_map, b.start_x, b.start_y));
        let recursos = settings.recursos.clone();

        let rt = tokio::runtime::Builder::new_multi_thread().worker_threads(2).enable_all().build().expect("tokio runtime");
        let listener = rt.block_on(tokio::net::TcpListener::bind("127.0.0.1:0")).expect("free port");
        let addr = listener.local_addr().unwrap().to_string();
        let handle = {
            let _guard = rt.enter();
            server::spawn(listener, settings)
        };
        let control = handle.control.expect("virtual clock gives a control channel");
        let shared = Arc::new(Shared { addr, recursos, clock, probe, sent: Mutex::new(HashMap::new()) });
        TestServer { shared, control, data, start, rt }
    }

    pub fn addr(&self) -> &str {
        &self.shared.addr
    }

    /// The game data the server runs with.
    pub fn data(&self) -> &server::data::GameData {
        &self.data
    }

    /// Where new characters appear (map, x, y).
    pub fn start_tile(&self) -> (u16, i32, i32) {
        self.start
    }

    pub fn now(&self) -> GameTime {
        self.shared.clock.now()
    }

    /// A connected bot that has not logged in (account flows, pre-login attacks).
    pub fn connect(&self) -> Bot {
        Bot::connect(self.shared.clone())
    }

    /// A bot logged in with the development login and standing on the start map.
    pub fn bot(&self, name: &str) -> Bot {
        let mut b = self.connect();
        b.login(name);
        b
    }

    /// Makes `ms` of game time pass: first lets the server consume everything every bot
    /// has sent, then runs every tick in that span. Returns the new game time.
    pub fn advance(&self, ms: u64) -> GameTime {
        self.settle();
        let (done, rx) = tokio::sync::oneshot::channel();
        self.control.send(Control::Advance { ms, done }).expect("the world task ended (panic?)");
        rx.blocking_recv().expect("the world task ended (panic?)")
    }

    pub fn advance_secs(&self, secs: u64) -> GameTime {
        self.advance(secs * 1000)
    }

    /// Waits until the server has consumed every frame sent by every live bot.
    pub fn settle(&self) {
        let deadline = Instant::now() + WAIT_CAP;
        loop {
            let pending: Vec<(SocketAddr, u64)> = self.shared.sent.lock().unwrap().iter().map(|(a, n)| (*a, *n)).collect();
            if pending.iter().all(|(a, n)| self.shared.consumed(*a, *n)) {
                return;
            }
            assert!(Instant::now() < deadline, "settle: the server did not consume the bots' frames in {WAIT_CAP:?}");
            std::thread::sleep(Duration::from_micros(200));
        }
    }

    /// Runs a future on the server's runtime (rarely needed).
    pub fn block_on<F: std::future::Future>(&self, f: F) -> F::Output {
        self.rt.block_on(f)
    }
}

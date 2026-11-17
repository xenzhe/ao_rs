//! Attacks a real server instance the way a cheat client would, and checks every attempt
//! is rejected (and that a legit client is never punished).
use ao_formats::{recursos_dir, AoMap};
use ao_protocol::*;
use ao_rules::{walk, Dir, WALK_MS};
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::time::{sleep, timeout};

const FIREBALL: u16 = 1;
const LIGHTNING: u16 = 2;
const HEAL: u16 = 3;

struct Bot {
    sock: TcpStream,
    reader: FrameReader,
    seq: u32,
}

impl Bot {
    async fn connect(addr: &str) -> Bot {
        Bot { sock: TcpStream::connect(addr).await.unwrap(), reader: FrameReader::default(), seq: 0 }
    }

    async fn send(&mut self, msg: &C2S) {
        self.sock.write_all(&encode(msg)).await.unwrap();
    }

    async fn raw(&mut self, bytes: &[u8]) {
        let _ = self.sock.write_all(bytes).await;
    }

    /// Collects every message the server sends within `ms`; returns (msgs, closed).
    async fn drain(&mut self, ms: u64) -> (Vec<S2C>, bool) {
        let mut out = Vec::new();
        let mut buf = [0u8; 65536];
        let deadline = tokio::time::Instant::now() + Duration::from_millis(ms);
        loop {
            while let Some(Ok(m)) = self.reader.next::<S2C>(MAX_SERVER_FRAME) {
                out.push(m);
            }
            let left = deadline.saturating_duration_since(tokio::time::Instant::now());
            if left.is_zero() {
                return (out, false);
            }
            match timeout(left, self.sock.read(&mut buf)).await {
                Ok(Ok(0)) | Ok(Err(_)) => {
                    while let Some(Ok(m)) = self.reader.next::<S2C>(MAX_SERVER_FRAME) {
                        out.push(m);
                    }
                    return (out, true);
                }
                Ok(Ok(n)) => self.reader.push(&buf[..n]),
                Err(_) => return (out, false),
            }
        }
    }

    async fn login(&mut self, name: &str) -> (i32, i32) {
        self.send(&C2S::Login { name: name.into(), version: PROTOCOL_VERSION }).await;
        let (msgs, _) = self.drain(300).await;
        msgs.iter()
            .find_map(|m| if let S2C::EnterMap { x, y, .. } = m { Some((*x, *y)) } else { None })
            .expect("enter map")
    }

    async fn walk(&mut self, dir: Dir) {
        self.seq += 1;
        let seq = self.seq;
        self.send(&C2S::Walk { dir, seq }).await;
    }

    async fn cast(&mut self, spell: u16, tx: i32, ty: i32) {
        self.seq += 1;
        let seq = self.seq;
        self.send(&C2S::Cast { spell, tx, ty, seq }).await;
    }
}

async fn start() -> String {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap().to_string();
    tokio::spawn(server::run(listener, server::Settings::defaults()));
    addr
}

/// A direction with 3 legal steps from the spawn tile on the real Ullathorpe map, so any
/// rejection in the tests is due to timing and not to a wall.
fn free_dir(x: i32, y: i32) -> Dir {
    let map = AoMap::load(&recursos_dir(), 1).expect("AO maps");
    for d in [Dir::Right, Dir::Left, Dir::Down, Dir::Up] {
        let (dx, dy) = d.delta();
        if (1..=3).all(|k| walk::can_enter(&map, x + k * dx, y + k * dy, d) && map.exit(x + k * dx, y + k * dy).is_none()) {
            return d;
        }
    }
    panic!("no free direction around the spawn");
}

#[tokio::test]
async fn speedhack_is_rejected_and_kicked() {
    let addr = start().await;
    let mut bot = Bot::connect(&addr).await;
    let (x, y) = bot.login("Speedy").await;
    let dir = free_dir(x, y);
    // 12 steps in a burst: a legit client needs 12 * 200 ms.
    for _ in 0..12 {
        bot.walk(dir).await;
    }
    let (msgs, closed) = bot.drain(500).await;
    let corrections = msgs.iter().filter(|m| matches!(m, S2C::Correct { .. })).count();
    let kicked = msgs.iter().any(|m| matches!(m, S2C::Kicked { .. }));
    assert!(corrections >= 8, "server must reject the extra steps");
    assert!(kicked && closed, "repeat offender must be kicked");
}

#[tokio::test]
async fn legit_walk_is_accepted() {
    let addr = start().await;
    let mut bot = Bot::connect(&addr).await;
    let (x, y) = bot.login("Walker").await;
    let dir = free_dir(x, y);
    for _ in 0..3 {
        bot.walk(dir).await;
        sleep(Duration::from_millis(WALK_MS + 5)).await;
    }
    let (msgs, closed) = bot.drain(200).await;
    assert!(!closed);
    assert!(!msgs.iter().any(|m| matches!(m, S2C::Correct { .. })), "legit pace must never be corrected: {msgs:?}");
}

#[tokio::test]
async fn replayed_packets_are_ignored() {
    let addr = start().await;
    let mut bot = Bot::connect(&addr).await;
    bot.login("Replayer").await;
    for _ in 0..10 {
        bot.send(&C2S::Walk { dir: Dir::Left, seq: 1 }).await;
        sleep(Duration::from_millis(WALK_MS + 5)).await;
    }
    let (msgs, closed) = bot.drain(300).await;
    assert!(closed && msgs.iter().any(|m| matches!(m, S2C::Kicked { .. })), "replay spam must end in a kick");
}

#[tokio::test]
async fn oversized_and_garbage_frames_disconnect() {
    let addr = start().await;
    let mut bot = Bot::connect(&addr).await;
    bot.login("Fuzzer").await;
    bot.raw(&(100_000_000u32).to_le_bytes()).await;
    let (_, closed) = bot.drain(500).await;
    assert!(closed, "oversized frame must close the connection");

    let mut bot2 = Bot::connect(&addr).await;
    bot2.login("Fuzzer2").await;
    bot2.raw(&[8, 0, 0, 0, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff]).await;
    let (_, closed2) = bot2.drain(500).await;
    assert!(closed2, "malformed frame must close the connection");
}

#[tokio::test]
async fn actions_before_login_are_kicked() {
    let addr = start().await;
    let mut bot = Bot::connect(&addr).await;
    bot.cast(LIGHTNING, 0, 0).await;
    let (_, closed) = bot.drain(500).await;
    assert!(closed);
}

#[tokio::test]
async fn illegal_spells_are_refused() {
    let addr = start().await;
    let mut bot = Bot::connect(&addr).await;
    let (x, y) = bot.login("Caster").await;

    bot.cast(FIREBALL, x + 30, y).await; // out of range
    bot.cast(HEAL, x + 1, y + 1).await; // heal on an empty tile
    let (msgs, _) = bot.drain(300).await;
    assert!(!msgs.iter().any(|m| matches!(m, S2C::Effect { .. })), "no spell effect may happen");
    let mana = msgs.iter().filter_map(|m| if let S2C::Stats { mana, .. } = m { Some(*mana) } else { None }).last();
    assert!(mana.is_none() || mana == Some(120), "mana must not be spent");

    // Cooldown abuse: heal self twice in a row.
    bot.cast(HEAL, x, y).await;
    bot.cast(HEAL, x, y).await;
    let (msgs, _) = bot.drain(300).await;
    let effects = msgs.iter().filter(|m| matches!(m, S2C::Effect { .. })).count();
    assert_eq!(effects, 1, "second cast inside the cooldown must be refused");

    // A spell id that does not exist.
    bot.cast(999, x, y).await;
    let (msgs, _) = bot.drain(200).await;
    assert!(!msgs.iter().any(|m| matches!(m, S2C::Effect { .. })));
}

#[tokio::test]
async fn chat_flood_and_control_chars_are_filtered() {
    let addr = start().await;
    let mut bot = Bot::connect(&addr).await;
    bot.login("Talker").await;
    for _ in 0..10 {
        bot.send(&C2S::Say { text: "hola\u{7}\u{1b}[31m".into() }).await;
    }
    let (msgs, _) = bot.drain(300).await;
    let chats: Vec<&String> = msgs.iter().filter_map(|m| if let S2C::Console { text, color: Tone::Chat } = m { Some(text) } else { None }).collect();
    assert!(chats.len() <= 4, "flood must be capped, got {}", chats.len());
    assert!(chats.iter().all(|t| !t.chars().any(|c| c.is_control())), "control chars must be stripped");
}

#[tokio::test]
async fn spell_book_comes_from_data_files() {
    let addr = start().await;
    let mut bot = Bot::connect(&addr).await;
    bot.send(&C2S::Login { name: "Reader".into(), version: PROTOCOL_VERSION }).await;
    let (msgs, _) = bot.drain(300).await;
    let book = msgs.iter().find_map(|m| if let S2C::SpellBook { spells } = m { Some(spells.clone()) } else { None }).expect("spell book");
    assert!(book.iter().any(|s| s.id == FIREBALL && s.name == "Bola de fuego"));
}

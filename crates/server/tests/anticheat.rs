//! Attacks a real server instance the way a cheat client would, and checks every attempt
//! is rejected (and that a legit client is never punished). Virtual clock: no test waits
//! for real time to pass.
use ao_protocol::{Tone, C2S, PROTOCOL_VERSION, S2C};
use ao_rules::{Dir, WALK_MS};
use ao_testkit::TestServer;

const FIREBALL: u16 = 1;
const LIGHTNING: u16 = 2;
const HEAL: u16 = 3;

fn corrections(m: &S2C) -> bool {
    matches!(m, S2C::Correct { .. })
}

fn effects(m: &S2C) -> bool {
    matches!(m, S2C::Effect { .. })
}

#[test]
fn speedhack_is_rejected_and_kicked() {
    let s = TestServer::start();
    let mut bot = s.bot("Speedy");
    let dir = bot.free_dir(3).expect("free direction");
    // 12 steps with no game time between them: a legit client needs 12 * WALK_MS.
    for _ in 0..12 {
        bot.walk_raw(dir);
    }
    bot.sync();
    assert!(bot.count(corrections) >= 8, "server must reject the extra steps");
    assert!(bot.kicked().is_some(), "repeat offender must be kicked");
    bot.wait_until("conexion cerrada", |b| b.closed());
}

#[test]
fn legit_walk_is_accepted() {
    let s = TestServer::start();
    let mut bot = s.bot("Walker");
    let dir = bot.free_dir(3).expect("free direction");
    let (x, y) = bot.pos();
    for _ in 0..3 {
        assert!(bot.walk(dir), "the client must allow a step at legit pace");
        s.advance(WALK_MS);
    }
    bot.sync();
    assert!(!bot.closed());
    assert_eq!(bot.count(corrections), 0, "legit pace must never be corrected: {:?}", bot.log());
    let (dx, dy) = dir.delta();
    assert_eq!(bot.pos(), (x + 3 * dx, y + 3 * dy));
}

#[test]
fn client_prediction_never_outruns_the_server() {
    let s = TestServer::start();
    let mut bot = s.bot("Eager");
    let dir = bot.free_dir(3).expect("free direction");
    assert!(bot.walk(dir));
    assert!(!bot.walk(dir), "the client itself holds the second step until WALK_MS passes");
    s.advance(WALK_MS / 2);
    assert!(!bot.walk(dir));
    s.advance(WALK_MS / 2);
    assert!(bot.walk(dir));
    bot.sync();
    assert_eq!(bot.count(corrections), 0);
}

#[test]
fn replayed_packets_are_ignored() {
    let s = TestServer::start();
    let mut bot = s.bot("Replayer");
    for _ in 0..10 {
        bot.send(&C2S::Walk { dir: Dir::Left, seq: 1 });
        s.advance(WALK_MS + 5);
    }
    bot.sync();
    assert!(bot.kicked().is_some(), "replay spam must end in a kick");
    bot.wait_until("conexion cerrada", |b| b.closed());
}

#[test]
fn oversized_and_garbage_frames_disconnect() {
    let s = TestServer::start();
    let mut bot = s.bot("Fuzzer");
    bot.send_bytes(&(100_000_000u32).to_le_bytes());
    bot.wait_until("cierre por paquete gigante", |b| b.closed());

    let mut bot2 = s.bot("Fuzzer2");
    bot2.send_bytes(&[8, 0, 0, 0, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff]);
    bot2.wait_until("cierre por paquete malformado", |b| b.closed());
}

#[test]
fn actions_before_login_are_kicked() {
    let s = TestServer::start();
    let mut bot = s.connect();
    bot.cast_raw(LIGHTNING, 0, 0);
    bot.wait_until("cierre por accion antes del login", |b| b.closed());
}

#[test]
fn packet_flood_is_kicked() {
    let s = TestServer::start();
    let mut bot = s.bot("Flooder");
    // 60 frames inside one second of game time (limit: security.max_msgs_per_sec = 40).
    for _ in 0..60 {
        bot.say("x");
    }
    bot.wait_until("cierre por flood de paquetes", |b| b.closed());
}

#[test]
fn illegal_spells_are_refused() {
    let s = TestServer::start();
    let mut bot = s.bot("Caster");
    let (x, y) = bot.pos();
    let mana0 = bot.mana();

    bot.cast_raw(FIREBALL, x + 30, y); // out of range
    bot.cast_raw(HEAL, x + 1, y + 1); // heal on an empty tile
    bot.sync();
    assert_eq!(bot.count(effects), 0, "no spell effect may happen");
    assert_eq!(bot.mana(), mana0, "mana must not be spent");

    // Cooldown abuse: heal self twice in a row.
    bot.cast_raw(HEAL, x, y);
    bot.cast_raw(HEAL, x, y);
    bot.sync();
    assert_eq!(bot.count(effects), 1, "second cast inside the cooldown must be refused");

    // A spell id that does not exist.
    bot.cast_raw(999, x, y);
    bot.sync();
    assert_eq!(bot.count(effects), 1);
}

#[test]
fn cooldown_ends_with_game_time() {
    let s = TestServer::start();
    let mut bot = s.bot("Patient");
    let (x, y) = bot.pos();
    let cd = s.data().spell(HEAL).unwrap().cooldown_ms;
    assert!(bot.cast(HEAL, x, y));
    bot.sync();
    assert!(!bot.cast(HEAL, x, y), "the client mirrors the cooldown and does not send");
    s.advance(cd + 100);
    assert!(bot.cast(HEAL, x, y), "after the cooldown the client casts again");
    bot.sync();
    assert_eq!(bot.count(effects), 2);
    assert!(bot.kicked().is_none());
}

#[test]
fn chat_flood_and_control_chars_are_filtered() {
    let s = TestServer::start();
    let mut bot = s.bot("Talker");
    for _ in 0..10 {
        bot.say("hola\u{7}\u{1b}[31m");
    }
    bot.sync();
    let chats: Vec<&String> = bot.log().iter().filter_map(|m| if let S2C::Console { text, color: Tone::Chat } = m { Some(text) } else { None }).collect();
    assert!(chats.len() <= 4, "flood must be capped, got {}", chats.len());
    assert!(!chats.is_empty());
    assert!(chats.iter().all(|t| !t.chars().any(|c| c.is_control())), "control chars must be stripped");
}

#[test]
fn spell_book_comes_from_data_files() {
    let s = TestServer::start();
    let mut bot = s.connect();
    bot.send(&C2S::Login { name: "Reader".into(), version: PROTOCOL_VERSION });
    bot.wait_until("libro de hechizos", |b| !b.game.spells.is_empty());
    assert!(bot.game.spells.iter().any(|s| s.id == FIREBALL && s.name == "Bola de fuego"));
}

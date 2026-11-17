//! World scenarios on the real AO20 maps, driven by bots and a virtual clock: map exits,
//! regeneration, seeing others move. NPC population, AI, respawn and determinism moved to
//! `p1_npcs.rs` (official NPCs replaced the creatures of data/creatures.toml).
use ao_formats::{recursos_dir, AoMap};
use ao_rules::{walk, Dir, WALK_MS};
use ao_testkit::TestServer;

const HEAL: u16 = 3;

#[test]
fn walking_into_an_exit_changes_map() {
    let map = AoMap::load(&recursos_dir(), 1).expect("AO maps");
    let exit = map
        .exits
        .iter()
        .find(|e| walk::standable(&map, e.x, e.y + 1) && walk::can_enter(&map, e.x, e.y, Dir::Up))
        .copied()
        .expect("exit reachable from the south");
    let s = TestServer::start_at(1, exit.x, exit.y + 1);
    let mut bot = s.bot("Viajero");
    assert_eq!(bot.map(), 1, "login enters the start map");
    assert_eq!(bot.pos(), (exit.x, exit.y + 1));
    assert!(bot.walk(Dir::Up));
    bot.wait_until("cambio de mapa", |b| b.map() == exit.map);
    let (x, y) = bot.pos();
    assert!((x - exit.dest_x).abs() <= 2 && (y - exit.dest_y).abs() <= 2, "near the destination tile");
}

#[test]
fn mana_regenerates_with_game_time() {
    let s = TestServer::start();
    let mut bot = s.bot("Mago");
    let (x, y) = bot.pos();
    let b = s.data().balance.clone();
    let heal = s.data().spell(HEAL).unwrap().mana;
    assert!(bot.cast(HEAL, x, y));
    bot.sync();
    assert_eq!(bot.mana(), b.start_mana - heal, "the cast spends mana");

    // No real time passes: without advancing, nothing regenerates.
    bot.sync();
    assert_eq!(bot.mana(), b.start_mana - heal);

    s.advance(1000);
    bot.wait_until("regenera un segundo de mana", |bot| bot.mana() == (b.start_mana - heal + b.mana_regen).min(b.start_mana));
    s.advance(60_000);
    bot.wait_until("mana lleno", |bot| bot.mana() == b.start_mana);
}

#[test]
fn bot_sees_another_bot_walk() {
    let s = TestServer::start();
    let mut ana = s.bot("Ana");
    let mut beto = s.bot("Beto");
    let id = beto.id();
    ana.wait_until("Ana ve a Beto", |b| b.ent(id).is_some());

    let dir = beto.free_dir(2).expect("free direction");
    for _ in 0..2 {
        assert!(beto.walk(dir));
        s.advance(WALK_MS);
    }
    let to = beto.pos();
    ana.wait_until("Ana ve a Beto en su nueva casilla", |b| b.ent(id).is_some_and(|e| (e.x, e.y) == to));
}

//! World scenarios on the real AO20 maps, driven by bots and a virtual clock: map exits,
//! creature population, regeneration, creature death and respawn, seeing others move.
use ao_formats::{recursos_dir, AoMap};
use ao_protocol::S2C;
use ao_rules::spells::SpellEffect;
use ao_rules::{tile_distance, walk, Dir, WALK_MS};
use ao_testkit::{ClientEvent, TestServer};

// Hechizos.dat ids (development characters get them on login).
const DART: u16 = 1;
const INVISIBILITY: u16 = 24;

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
fn non_safe_maps_get_creatures_from_data() {
    let s = TestServer::start_at(2, 50, 50);
    let expected = s.data().balance.creatures_per_map;
    let bot = s.bot("Cazador");
    assert_eq!(bot.map(), 2);
    let seen: Vec<_> = bot.creatures().map(|(_, e)| (e.name.clone(), e.body)).collect();
    assert!(seen.len() * 10 >= expected * 7, "expected about {expected} creatures, got {}", seen.len());
    for (name, body) in &seen {
        assert!(s.data().creatures.iter().any(|d| &d.name == name && d.body == *body), "{name} must come from creatures.toml");
    }
}

#[test]
fn safe_start_map_has_no_creatures() {
    let s = TestServer::start();
    let bot = s.bot("Ciudadano");
    assert_eq!(bot.creatures().count(), 0, "Ullathorpe is a safe zone");
}

#[test]
fn mana_regenerates_with_game_time() {
    let s = TestServer::start();
    let mut bot = s.bot("Mago");
    let (x, y) = bot.pos();
    let b = s.data().balance.clone();
    bot.wait_until("stats", |b| b.mana() > 0);
    let full = bot.mana();
    let cost = bot.game.spell_by_id(INVISIBILITY).unwrap().mana;
    assert!(bot.cast(INVISIBILITY, x, y));
    bot.sync();
    assert_eq!(bot.mana(), full - cost, "the cast spends mana");

    // No real time passes: without advancing, nothing regenerates.
    bot.sync();
    assert_eq!(bot.mana(), full - cost);

    s.advance(1000);
    bot.wait_until("regenera un segundo de mana", |bot| bot.mana() == (full - cost + b.mana_regen).min(full));
    s.advance(200_000);
    bot.wait_until("mana lleno", |bot| bot.mana() == full);
}

#[test]
fn killed_creature_respawns_after_its_time() {
    // Harmless, static, one-hit creatures that come back after 2 s.
    let s = TestServer::with(|st| {
        st.start = Some((2, 50, 50));
        st.data.balance.creatures_per_map = 60;
        for c in &mut st.data.creatures {
            c.hp = 1;
            c.damage = [0, 0];
            c.aggro_range = 0;
            c.wander_range = 0;
            c.respawn_secs = 2;
        }
    });
    let mut bot = s.bot("Verdugo");
    let range = ao_rules::magic::VIEW_RANGE_Y;
    let me = bot.pos();
    let map = bot.game.map.as_ref().unwrap();
    let (nid, tx, ty) = bot
        .creatures()
        .filter(|(_, e)| tile_distance(me, (e.x, e.y)) <= range && walk::line_of_sight(map, me.0, me.1, e.x, e.y))
        .map(|(id, e)| (id, e.x, e.y))
        .min()
        .expect("a creature in range and sight");
    let exp0 = bot.game.stats.exp;

    assert!(bot.cast(DART, tx, ty));
    bot.wait_until("la criatura muere", |b| b.ent(nid).is_none());
    bot.sync();
    assert!(bot.game.stats.exp > exp0, "killing gives experience");
    let ev = bot.events();
    assert!(ev.iter().any(|e| matches!(e, ClientEvent::Effect { effect: SpellEffect::Fireball, .. })), "the client shows the fireball");
    assert!(ev.iter().any(|e| matches!(e, ClientEvent::DeathBurst { .. })), "and the creature's death");
    bot.clear_log();

    s.advance(1500);
    bot.sync();
    assert!(bot.ent(nid).is_none(), "not back before its respawn time");
    s.advance(600);
    bot.wait_until("la criatura reaparece", |b| b.ent(nid).is_some());
    let e = bot.ent(nid).unwrap();
    assert_eq!(e.hp, e.max_hp, "respawns with full health");
    assert!(bot.log().iter().any(|m| matches!(m, S2C::Spawn { id, .. } if *id == nid)));
    assert!(bot.events().iter().any(|e| matches!(e, ClientEvent::Puff { .. })), "the respawn shows a puff");
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

#[test]
fn creatures_move_only_when_game_time_passes() {
    let s = TestServer::start_at(2, 50, 50);
    let mut bot = s.bot("Observador");
    let before: Vec<(u32, i32, i32)> = {
        let mut v: Vec<_> = bot.creatures().map(|(id, e)| (id, e.x, e.y)).collect();
        v.sort_unstable();
        v
    };
    let positions = |b: &ao_testkit::Bot| {
        let mut v: Vec<_> = b.creatures().map(|(id, e)| (id, e.x, e.y)).collect();
        v.sort_unstable();
        v
    };
    bot.sync();
    assert_eq!(positions(&bot), before, "frozen while the clock is stopped");
    s.advance(10_000);
    bot.sync();
    assert_ne!(positions(&bot), before, "creatures wander once game time passes");
}

#[test]
fn same_seed_same_world() {
    let run = || {
        let s = TestServer::start_at(2, 50, 50);
        s.advance(5_000);
        let bot = s.bot("Testigo");
        let mut v: Vec<_> = bot.creatures().map(|(id, e)| (id, e.x, e.y)).collect();
        v.sort_unstable();
        v
    };
    assert_eq!(run(), run(), "fixed seed + virtual clock = reproducible world");
}

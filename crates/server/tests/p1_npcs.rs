//! NPC system (P1, Opus D) driven by bots on the real AO20 maps with a virtual clock:
//! official NPCs on their .csm tiles, static NPCs stay quiet, creatures hunt, banker and
//! distance rules, cheat input, determinism. Kill/respawn, dead players, the chase around
//! a wall and the tick cost are in-crate tests (`systems::npc_ai::tests`).
use ao_formats::dat::npcs::Movement;
use ao_formats::{recursos_dir, AoMap};
use ao_protocol::{C2S, S2C};
use ao_testkit::TestServer;
use std::collections::BTreeMap;

/// Krix <Finanzas Goliath> (banker) and Juan XXI <Sacerdote> in Ullathorpe's .csm.
const BANKER: (i32, i32) = (75, 47);
const PRIEST: (i32, i32) = (78, 67);
/// Bosque Este de Ullathorpe: 34 wandering creatures.
const FOREST: u16 = 12;

fn npcs_dat() -> ao_formats::dat::Table<ao_formats::dat::npcs::Npc> {
    ao_formats::dat::npcs::load(&recursos_dir()).unwrap().data
}

#[test]
fn ullathorpe_has_its_official_npcs_on_their_tiles() {
    let map = AoMap::load(&recursos_dir(), 1).unwrap();
    let dat = npcs_dat();
    let mut expected: Vec<(String, i32, i32, u16, u16)> = map
        .npcs
        .iter()
        .filter_map(|e| dat.get(e.npc as u32).filter(|d| !d.name.is_empty()).map(|d| (d.name.clone(), e.x, e.y, d.body as u16, d.head as u16)))
        .collect();
    expected.sort();

    let s = TestServer::start_at(1, 77, 50);
    s.advance(100);
    let mut bot = s.bot("Turista");
    bot.sync();
    let mut seen: Vec<(String, i32, i32, u16, u16)> = bot.creatures().map(|(_, e)| (e.name.clone(), e.x, e.y, e.body, e.head)).collect();
    seen.sort();
    assert_eq!(seen, expected, "exactly the .csm list with npcs.dat looks");
    let at = |p: (i32, i32)| seen.iter().find(|e| (e.1, e.2) == p).map(|e| e.0.clone());
    assert!(at(BANKER).is_some_and(|n| n.contains("Finanzas")), "banker at {BANKER:?}");
    assert!(at(PRIEST).is_some_and(|n| n.contains("Sacerdote")), "priest at {PRIEST:?}");
}

#[test]
fn static_npcs_never_move_and_send_nothing() {
    let s = TestServer::start_at(1, 77, 50);
    let mut bot = s.bot("Vigia");
    s.advance(100);
    bot.sync();
    let dat = npcs_dat();
    let map = AoMap::load(&recursos_dir(), 1).unwrap();
    let statics: BTreeMap<u32, (i32, i32)> = bot
        .creatures()
        .filter(|(_, e)| map.npcs.iter().any(|m| (m.x, m.y) == (e.x, e.y) && dat.get(m.npc as u32).is_some_and(|d| d.movement == Movement::Static)))
        .map(|(id, e)| (id, (e.x, e.y)))
        .collect();
    assert!(statics.len() > 10, "Ullathorpe is full of static NPCs");
    bot.clear_log();
    for _ in 0..30 {
        s.advance(1000);
    }
    bot.sync();
    for (id, pos) in &statics {
        let e = bot.ent(*id).unwrap();
        assert_eq!((e.x, e.y), *pos);
    }
    let moved = bot.log().iter().filter(|m| matches!(m, S2C::Moved { id, .. } if statics.contains_key(id))).count();
    assert_eq!(moved, 0, "static NPCs do not flood the network");
}

#[test]
fn nobody_in_ullathorpe_attacks_a_player() {
    let dat = npcs_dat();
    let map = AoMap::load(&recursos_dir(), 1).unwrap();
    // The official city keeps a couple of hostile training rats far from the square.
    let near_square = |e: &&ao_formats::csm::MapNpc| (e.x - 50).abs().max((e.y - 50).abs()) <= 15;
    assert!(map.npcs.iter().filter(near_square).filter_map(|e| dat.get(e.npc as u32)).all(|d| d.hostile == 0), "no hostile NPC near the square");
    let s = TestServer::start_at(1, 50, 50);
    let mut bot = s.bot("Paseante");
    let hp0 = bot.hp();
    for _ in 0..30 {
        s.advance(1000);
    }
    bot.sync();
    assert!(bot.hp() >= hp0, "30 s in the city without damage");
}

#[test]
fn creatures_wander_once_game_time_passes() {
    let s = TestServer::start_at(FOREST, 50, 50);
    let mut bot = s.bot("Observador");
    s.advance(100);
    bot.sync();
    let positions = |b: &ao_testkit::Bot| b.creatures().map(|(id, e)| (id, (e.x, e.y))).collect::<BTreeMap<_, _>>();
    let before = positions(&bot);
    assert!(before.len() >= 10, "the forest has its creatures, got {}", before.len());
    bot.sync();
    assert_eq!(positions(&bot), before, "frozen while the clock is stopped");
    s.advance(10_000);
    bot.sync();
    assert_ne!(positions(&bot), before, "creatures move once game time passes");
}

#[test]
fn same_seed_same_world() {
    let run = || {
        let s = TestServer::start_at(FOREST, 50, 50);
        let mut bot = s.bot("Testigo");
        for _ in 0..20 {
            s.advance(1000);
        }
        bot.sync();
        bot.creatures().map(|(id, e)| (id, e.x, e.y, e.dir)).collect::<Vec<_>>()
    };
    let (a, b) = (run(), run());
    assert!(!a.is_empty());
    let mut a = a;
    let mut b = b;
    a.sort_by_key(|e| e.0);
    b.sort_by_key(|e| e.0);
    assert_eq!(format!("{a:?}"), format!("{b:?}"), "fixed seed + virtual clock = reproducible NPCs");
}

#[test]
fn banker_opens_the_vault_from_close_by() {
    let s = TestServer::start_at(1, BANKER.0, BANKER.1 + 2);
    let mut bot = s.bot("Cliente");
    s.advance(100);
    bot.sync();
    assert_eq!(bot.pos(), (BANKER.0, BANKER.1 + 2));
    let banker = bot.creatures().find(|(_, e)| (e.x, e.y) == BANKER).map(|(id, _)| id).expect("banker visible");
    bot.clear_log();
    bot.send(&C2S::Interact { tx: BANKER.0, ty: BANKER.1 });
    bot.sync();
    assert!(bot.log().iter().any(|m| matches!(m, S2C::NpcSay { id, .. } if *id == banker)), "the banker talks");
    let opened = bot.log().iter().any(|m| matches!(m, S2C::BankOpen { .. })) || bot.console().any(|t| t.to_lowercase().contains("boveda"));
    assert!(opened, "open_bank was called: {:?}", bot.log());
    assert!(!bot.console().any(|t| t.contains("lejos")));
}

#[test]
fn banker_refuses_from_too_far() {
    // Manhattan 5 > 4 (EnsureNpcWithinDistance).
    let s = TestServer::start_at(1, BANKER.0, BANKER.1 + 5);
    let mut bot = s.bot("Lejano");
    s.advance(100);
    bot.sync();
    assert_eq!(bot.pos(), (BANKER.0, BANKER.1 + 5));
    bot.clear_log();
    bot.send(&C2S::Interact { tx: BANKER.0, ty: BANKER.1 });
    bot.sync();
    assert!(bot.console().any(|t| t.contains("lejos")), "too far message");
    assert!(!bot.log().iter().any(|m| matches!(m, S2C::BankOpen { .. } | S2C::NpcSay { .. })));
    assert!(!bot.console().any(|t| t.to_lowercase().contains("boveda")));
}

#[test]
fn far_clicks_are_ignored_and_empty_tiles_answered() {
    let s = TestServer::start_at(1, 50, 50);
    let mut bot = s.bot("Despistado");
    // Inside the map but outside the visible area: a stale click, silently ignored.
    bot.send(&C2S::Interact { tx: 50 + 30, ty: 50 });
    bot.sync();
    assert!(!bot.closed());
    bot.clear_log();
    bot.send(&C2S::Interact { tx: 50, ty: 51 });
    bot.sync();
    assert!(bot.console().any(|t| t.contains("No hay nada")), "a normal miss");
}

#[test]
fn trap_interact_outside_the_map_is_struck_and_kicked() {
    let s = TestServer::start_at(1, 50, 50);
    let mut bot = s.bot("Tramposo");
    for i in 0..20 {
        let (tx, ty) = [(i32::MIN, i32::MAX), (i32::MAX, i32::MIN), (0, 0), (101, 50)][i % 4];
        bot.send(&C2S::Interact { tx, ty });
    }
    bot.wait_until("conexion cerrada", |b| b.closed());
    assert!(bot.kicked().is_some(), "repeated impossible clicks end in a kick");
}

#[test]
fn merchant_answers_that_trade_comes_in_p2() {
    let dat = npcs_dat();
    let map = AoMap::load(&recursos_dir(), 1).unwrap();
    let m = map.npcs.iter().find(|e| dat.get(e.npc as u32).is_some_and(|d| d.trades == 1 && !d.name.is_empty())).copied().expect("a merchant in Ullathorpe");
    let s = TestServer::start_at(1, m.x, m.y + 2);
    let mut bot = s.bot("Comprador");
    s.advance(100);
    bot.sync();
    let (x, y) = bot.pos();
    assert!((x - m.x).abs() + (y - m.y).abs() <= 4, "close to the merchant");
    bot.send(&C2S::Interact { tx: m.x, ty: m.y });
    bot.sync();
    // P2: the merchant window opens (systems/trade.rs).
    assert!(bot.game.local.social.trade.shop.is_some(), "{:?}", bot.console().collect::<Vec<_>>());
}

#[test]
fn forest_creatures_hunt_a_player() {
    let s = TestServer::start_at(FOREST, 50, 50);
    let mut bot = s.bot("Presa");
    let me = bot.pos();
    s.advance(100);
    bot.sync();
    bot.clear_log();
    let mut reached = false;
    for _ in 0..60 {
        s.advance(1000);
        bot.sync();
        reached |= bot.log().iter().any(|m| matches!(m, S2C::Moved { x, y, .. } if (x - me.0).abs() + (y - me.1).abs() == 1));
        if reached || bot.pos() != me {
            break;
        }
    }
    assert!(reached, "some creature walks up to the player");
}

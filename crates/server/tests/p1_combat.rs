//! P1 combat (SistemaCombate.bas) on ao-testkit: players fighting until one is a ghost,
//! safe zones, killing a creature, attack interval and the cheats around it.
use ao_formats::recursos_dir;
use ao_protocol::{C2S, S2C};
use ao_rules::combat::{self as rules, ATTACK_INTERVAL_MS, DEAD_BODY};
use ao_rules::{tile_distance, Dir};
use ao_testkit::{Bot, TestServer};

/// Map 2 is not a safe zone; map 1 (Ullathorpe) is.
const FIELD: (u16, i32, i32) = (2, 50, 50);
const CITY: (u16, i32, i32) = (1, 50, 50);
const WARRIOR: u8 = 3;

/// Characters appear at `at`, start with `hp`, nobody regenerates and no NPCs.
fn server(at: (u16, i32, i32), hp: i32) -> TestServer {
    TestServer::with(|st| {
        st.npcs = false;
        st.start = Some(at);
        let b = &mut st.data.balance;
        (b.start_map, b.start_x, b.start_y) = at;
        b.start_hp = hp;
        b.hp_regen = 0;
        b.creatures_per_map = 0;
    })
}

/// A level 1 human warrior, stripped of its official starting equipment so the fight is
/// bare-handed (1..2 damage, see ao_rules::combat tests) and naked (no stamina regen, no
/// armor absorbing hits). In AO, equipping an equipped item takes it off.
fn warrior(s: &TestServer, name: &str) -> Bot {
    let mut c = equipped_warrior(s, name);
    let equipped: Vec<u8> = c.game.local.inventory.iter().enumerate().filter(|(_, s)| s.is_some_and(|i| i.equipped)).map(|(i, _)| (i + 1) as u8).collect();
    for slot in equipped {
        c.send(&C2S::EquipItem { slot });
    }
    c.wait_until("desnudo", |b| b.game.local.inventory.iter().flatten().all(|i| !i.equipped));
    c
}

/// A level 1 human warrior with its official starting equipment (EquipamientoInicial.dat):
/// wearing armor, so its stamina regenerates between swings.
fn equipped_warrior(s: &TestServer, name: &str) -> Bot {
    let mut c = s.connect();
    assert!(c.create_account(&name.to_lowercase(), "secreta1"));
    assert!(c.create_character(name, 1, WARRIOR, 1, 1));
    c.play(name);
    c.wait_until("equipo inicial recibido", |b| b.game.local.inventory.iter().flatten().any(|i| i.equipped));
    // Citizens log in with the seguro on (ConnectUser): switch it off to fight.
    c.send(&C2S::Faction(ao_protocol::FactionC2S::ToggleSafe));
    c.sync();
    c
}

fn dir_to(from: (i32, i32), to: (i32, i32)) -> Dir {
    Dir::towards(to.0 - from.0, to.1 - from.1)
}

fn face(bot: &mut Bot, to: (i32, i32)) {
    let dir = dir_to(bot.pos(), to);
    bot.send(&C2S::Turn { dir });
    bot.sync();
}

fn melee(bot: &mut Bot) {
    let seq = bot.game.next_seq();
    bot.send(&C2S::Melee { seq });
}

/// Two warriors orthogonally adjacent, `a` facing `b`; both naked (bare hands, no stamina
/// regeneration).
fn duel(s: &TestServer) -> (Bot, Bot) {
    let a = warrior(s, "Atacante");
    duel_with(s, a)
}

/// Same, with the attacker given by the caller.
fn duel_with(s: &TestServer, mut a: Bot) -> (Bot, Bot) {
    let mut b = warrior(s, "Victima");
    let (pa, pb) = (a.pos(), b.pos());
    assert_eq!(tile_distance(pa, pb), 1, "spawned next to each other");
    if pa.0 != pb.0 && pa.1 != pb.1 {
        // Diagonal: step into the attacker's column.
        assert!(b.walk(dir_to(pb, (pa.0, pb.1))));
        s.advance(ao_rules::WALK_MS);
        b.sync();
    }
    a.sync();
    let target = b.pos();
    face(&mut a, target);
    (a, b)
}

fn attacks_by(bot: &Bot, attacker: u32) -> Vec<(bool, i32)> {
    bot.log().iter().filter_map(|m| if let S2C::Attack { attacker: x, hit, damage, .. } = m { (*x == attacker).then_some((*hit, *damage)) } else { None }).collect()
}

#[test]
fn players_fight_until_one_is_a_ghost() {
    let s = server(FIELD, 3);
    // The attacker keeps its starting equipment (stamina regenerates with armor on); the
    // victim is naked so nothing absorbs the hits.
    let attacker = equipped_warrior(&s, "Atacante");
    let (mut a, mut b) = duel_with(&s, attacker);
    let bid = b.id();
    // Official creation gives Constitution (~20) hit points; bare hands hit 0..2.
    for _ in 0..300 {
        if b.hp() == 0 {
            break;
        }
        melee(&mut a);
        s.advance(ATTACK_INTERVAL_MS);
        a.sync();
        b.sync();
    }
    assert_eq!(b.hp(), 0, "the victim died: {:?}", attacks_by(&a, a.id()));
    let swings = attacks_by(&a, a.id());
    assert!(swings.iter().any(|(hit, _)| *hit), "some swing hit");
    assert!(swings.iter().all(|(_, d)| (0..=40).contains(d)), "a level 1 warrior's hits stay small: {swings:?}");
    let ghost = a.ent(bid).expect("still visible");
    assert_eq!((ghost.body, ghost.head), (DEAD_BODY, 0), "official ghost body iCuerpoMuerto");
    assert_eq!(b.me().body, DEAD_BODY);
    assert!(a.console().any(|t| t.contains("Has matado a Victima")));
    assert!(b.console().any(|t| t.contains("Atacante te ha matado")));

    // A ghost is no target and cannot attack back.
    let hp_a = a.hp();
    a.clear_log();
    b.clear_log();
    melee(&mut a);
    s.advance(ATTACK_INTERVAL_MS);
    a.sync();
    assert!(attacks_by(&a, a.id()).iter().all(|(hit, _)| !hit));
    assert!(a.console().any(|t| t.contains("espiritu")));
    let target = a.pos();
    face(&mut b, target);
    melee(&mut b);
    s.advance(ATTACK_INTERVAL_MS);
    b.sync();
    a.sync();
    assert!(attacks_by(&a, bid).is_empty(), "ghosts do not swing");
    assert!(b.console().any(|t| t.contains("muerto")));
    assert_eq!(a.hp(), hp_a);
    assert!(b.kicked().is_none(), "trying while dead is not a cheat");
}

#[test]
fn no_pvp_in_safe_zones() {
    let s = server(CITY, 3);
    let (mut a, mut b) = duel(&s);
    let hp_before = b.hp();
    for _ in 0..4 {
        melee(&mut a);
        s.advance(ATTACK_INTERVAL_MS);
    }
    a.sync();
    b.sync();
    assert_eq!(b.hp(), hp_before);
    assert!(attacks_by(&a, a.id()).iter().all(|(hit, _)| !hit));
    assert!(a.console().any(|t| t.contains("zona segura")));
}

#[test]
fn attack_interval_ignores_spam() {
    let s = server(FIELD, 100);
    let (mut a, mut b) = duel(&s);
    for _ in 0..5 {
        melee(&mut a);
    }
    a.sync();
    b.sync();
    assert_eq!(attacks_by(&a, a.id()).len(), 1, "one attack per IntervaloUserPuedeAtacar");
    s.advance(ATTACK_INTERVAL_MS - 1);
    melee(&mut a);
    a.sync();
    assert_eq!(attacks_by(&a, a.id()).len(), 1, "still too soon");
    s.advance(1);
    melee(&mut a);
    a.sync();
    assert_eq!(attacks_by(&a, a.id()).len(), 2);
    assert!(a.kicked().is_none());

    // A replayed sequence number is a cheat (strike).
    s.advance(ATTACK_INTERVAL_MS);
    a.send(&C2S::Melee { seq: 1 });
    a.sync();
    assert_eq!(attacks_by(&a, a.id()).len(), 2, "replay refused");
}

#[test]
fn stamina_runs_out() {
    let s = server(FIELD, 1000);
    let (mut a, _b) = duel(&s);
    for _ in 0..12 {
        melee(&mut a);
        s.advance(ATTACK_INTERVAL_MS);
    }
    a.sync();
    let n = attacks_by(&a, a.id()).len();
    assert!((4..=12).contains(&n), "40 stamina at 1..10 per swing: {n}");
    assert!(a.console().any(|t| t.contains("cansado")));
}

#[test]
fn warrior_kills_a_creature_and_gets_its_official_experience() {
    // An official Gallina (npcs.dat 501: hostile, 10 HP, GiveEXP=5) alone on one of the
    // first maps; the warrior appears on a free tile next to it and the hen comes for him.
    let root = recursos_dir();
    let (num, map, hen) = (1..=80u16)
        .filter_map(|n| ao_formats::AoMap::load(&root, n).ok().map(|m| (n, m)))
        .find_map(|(n, map)| {
            // A lone hen: no other NPC of the map within 12 tiles to join the fight.
            let hen = map.npcs.iter().filter(|h| h.npc == 501).find(|h| map.npcs.iter().all(|o| (o.x, o.y) == (h.x, h.y) || tile_distance((o.x, o.y), (h.x, h.y)) > 12)).cloned()?;
            Some((n, map, hen))
        })
        .expect("a lone Gallina on the first maps");
    let spot = Dir::ALL
        .iter()
        .map(|d| (hen.x + d.delta().0, hen.y + d.delta().1))
        .find(|&(x, y)| ao_rules::walk::standable(&map, x, y))
        .expect("free tile next to the hen");
    let at = (num, spot.0, spot.1);
    let s = TestServer::with(|st| {
        st.start = Some(at);
        let b = &mut st.data.balance;
        (b.start_map, b.start_x, b.start_y) = at;
    });
    let mut bot = equipped_warrior(&s, "Cazador");
    let exp0 = bot.game.stats.exp;
    let mut killed = false;
    for _ in 0..400 {
        if bot.console().any(|t| t.contains("Has matado a Gallina")) {
            killed = true;
            break;
        }
        let me = bot.pos();
        let next = bot.creatures().find(|(_, e)| e.name == "Gallina" && (e.x - me.0).abs() + (e.y - me.1).abs() == 1).map(|(_, e)| (e.x, e.y));
        if let Some(t) = next {
            face(&mut bot, t);
            melee(&mut bot);
        }
        s.advance(ATTACK_INTERVAL_MS);
        bot.sync();
        if bot.hp() <= 0 {
            let me = bot.id();
            let hits: Vec<String> = bot.log().iter().filter_map(|m| match m {
                S2C::Attack { attacker, target, hit: true, damage, .. } if *target == me => Some(format!("{}:{damage}", bot.ent(*attacker).map_or("?".into(), |e| e.name.clone()))),
                _ => None,
            }).collect();
            panic!("the warrior died; mine {:?}; theirs {hits:?}", attacks_by(&bot, me));
        }
    }
    assert!(killed, "the hen died: {:?}", attacks_by(&bot, bot.id()));
    // GiveEXP=5, all of it for taking its whole life.
    let b = ao_formats::dat::balance::load(&recursos_dir()).unwrap().data;
    let want = rules::add_exp(&b, 1, exp0, 5);
    assert_eq!((bot.level(), bot.game.stats.exp), (want.level, want.exp));
}

#[test]
fn ranged_without_bow_or_ammo_and_out_of_sight() {
    let s = server(FIELD, 100);
    let (mut a, b) = duel(&s);
    let (x, y) = b.pos();
    let seq = a.game.next_seq();
    a.send(&C2S::Shoot { tx: x, ty: y, seq });
    a.sync();
    assert!(a.console().any(|t| t.contains("arma de proyectiles")));
    assert!(attacks_by(&a, a.id()).is_empty());

    // Far outside the visible window: only a cheat sends that.
    let (px, py) = a.pos();
    for k in 0..40 {
        let seq = a.game.next_seq();
        a.send(&C2S::Shoot { tx: px + 30, ty: py, seq });
        if k % 8 == 0 {
            a.sync();
        }
        if a.closed() {
            break;
        }
    }
    a.wait_until("expulsado por disparar fuera de la vista", |b| b.closed());
}

#[test]
fn swinging_at_an_empty_tile_hits_nothing() {
    let s = server(FIELD, 3);
    let mut a = warrior(&s, "Solo");
    let dir = a.free_dir(1).expect("room");
    a.send(&C2S::Turn { dir });
    melee(&mut a);
    a.sync();
    let swings: Vec<_> = a.log().iter().filter(|m| matches!(m, S2C::Attack { target: 0, hit: false, .. })).collect();
    assert_eq!(swings.len(), 1, "a swing at nothing");
}

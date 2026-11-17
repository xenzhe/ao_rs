//! P1 magic: official Hechizos.dat spells cast by bots on a virtual clock. Development
//! characters get a temporary kit (Dardo Mágico, Curar Heridas Leves, Curar Veneno,
//! Paralizar, Inmovilizar, Remover Parálisis, Invisibilidad Menor, Resucitar) with Magic
//! 100, mana >= 1000 and stamina >= 500.
use ao_protocol::{status, C2S, S2C};
use ao_rules::magic::{MEDITATE_PULSE_MS, MEDITATE_WARMUP_MS, VIEW_RANGE_X};
use ao_rules::{tile_distance, Dir};
use ao_testkit::{Bot, TestServer};

const DART: u16 = 1;
const PVP_ARROW: u16 = 2; // exists in Hechizos.dat, not in the kit
const HEAL: u16 = 12;
const PARALYZE: u16 = 25;
const INVISIBILITY: u16 = 24;

/// Map 2 (not a safe zone) with harmless, static creatures.
fn field() -> TestServer {
    TestServer::with(|st| {
        st.start = Some((2, 50, 50));
        st.data.balance.creatures_per_map = 60;
        for c in &mut st.data.creatures {
            c.damage = [0, 0];
            c.aggro_range = 0;
            c.wander_range = 0;
        }
    })
}

fn effects(m: &S2C) -> bool {
    matches!(m, S2C::Effect { .. })
}

/// Last `S2C::Status` flags this bot received about `id`.
fn status_of(bot: &Bot, id: u32) -> Option<u32> {
    bot.log().iter().rev().find_map(|m| match m {
        S2C::Status { id: i, flags } if *i == id => Some(*flags),
        _ => None,
    })
}

fn said(bot: &Bot, text: &str) -> bool {
    bot.console().any(|l| l.contains(text))
}

fn ready(bot: &mut Bot) {
    bot.wait_until("libro de hechizos", |b| !b.game.spells.is_empty() && b.mana() > 0);
    // Citizens log in with the seguro on (ConnectUser): switch it off to cast on others.
    bot.send(&C2S::Faction(ao_protocol::FactionC2S::ToggleSafe));
    bot.sync();
}

#[test]
fn damage_spell_hurts_a_creature() {
    let s = field();
    let mut ana = s.bot("Ana");
    ready(&mut ana);
    let me = ana.pos();
    let (nid, tx, ty, hp0) = ana
        .creatures()
        .filter(|(_, e)| tile_distance(me, (e.x, e.y)) <= 9)
        .map(|(id, e)| (id, e.x, e.y, e.hp))
        .min()
        .expect("a creature in range");
    let mana0 = ana.mana();
    assert!(ana.cast(DART, tx, ty));
    ana.wait_until("la criatura recibe el dardo", |b| b.ent(nid).is_none_or(|e| e.hp < hp0));
    ana.sync();
    assert_eq!(ana.mana(), mana0 - 10, "Dardo Mágico costs ManaRequerido=10");
    assert_eq!(ana.count(effects), 1);
}

#[test]
fn damage_spell_hurts_a_player_outside_safe_zones_only() {
    let s = field();
    let mut ana = s.bot("Ana");
    let mut beto = s.bot("Beto");
    ready(&mut ana);
    ready(&mut beto);
    let (bx, by) = beto.pos();
    let hp0 = beto.hp();
    assert!(ana.cast(DART, bx, by));
    beto.wait_until("Beto recibe el dardo", |b| b.hp() < hp0);
    // Dardo Mágico: 5-10 + 3% per level (level 1 rounds to the same range).
    assert!((5..=10).contains(&(hp0 - beto.hp())), "official damage range, got {}", hp0 - beto.hp());

    // Ullathorpe is a safe map: the same cast is refused there.
    let safe = TestServer::start();
    let mut c = safe.bot("Carla");
    let mut d = safe.bot("Dario");
    ready(&mut c);
    ready(&mut d);
    let (dx, dy) = d.pos();
    let hp = d.hp();
    c.cast_raw(DART, dx, dy);
    c.sync();
    d.sync();
    assert_eq!(c.count(effects), 0);
    assert_eq!(d.hp(), hp);
    assert!(said(&c, "zona segura"));
}

#[test]
fn healing_restores_hp() {
    let s = field();
    let mut ana = s.bot("Ana");
    let mut beto = s.bot("Beto");
    ready(&mut ana);
    ready(&mut beto);
    let (bx, by) = beto.pos();
    let full = beto.hp();
    assert!(ana.cast(DART, bx, by));
    beto.wait_until("herido", |b| b.hp() < full);
    let hurt = beto.hp();
    assert!(beto.cast(HEAL, bx, by), "heal self");
    beto.wait_until("curado", |b| b.hp() > hurt);
    assert!(said(&beto, "Te has restaurado"));
    assert!(beto.hp() <= full);
}

#[test]
fn paralysis_is_broadcast_and_expires() {
    let s = field();
    let mut ana = s.bot("Ana");
    let mut beto = s.bot("Beto");
    ready(&mut ana);
    ready(&mut beto);
    let (bid, (bx, by)) = (beto.id(), beto.pos());
    assert!(ana.cast(PARALYZE, bx, by));
    beto.wait_until("Beto ve su paralisis", |b| status_of(b, bid).is_some_and(|f| f & status::PARALYZED != 0));
    ana.wait_until("Ana la ve", |b| status_of(b, bid).is_some_and(|f| f & status::PARALYZED != 0));

    // Paralizar: Duration=13 s for a mage (dev characters are class 1).
    s.advance(12_000);
    beto.sync();
    assert!(status_of(&beto, bid).is_some_and(|f| f & status::PARALYZED != 0), "still paralysed");
    s.advance(1_100);
    beto.wait_until("la paralisis expira", |b| status_of(b, bid).is_some_and(|f| f & status::PARALYZED == 0));
}

/// Needs the movement system to ask `World::can_move` (integration point, see report).
#[test]
fn paralysed_players_cannot_walk() {
    let s = field();
    let mut ana = s.bot("Ana");
    let mut beto = s.bot("Beto");
    ready(&mut ana);
    ready(&mut beto);
    let (bx, by) = beto.pos();
    assert!(ana.cast(PARALYZE, bx, by));
    beto.wait_until("paralizado", |b| status_of(b, b.id()).is_some_and(|f| f & status::PARALYZED != 0));
    let dir = beto.free_dir(1).expect("free direction");
    beto.walk_raw(dir);
    beto.sync();
    assert_eq!(beto.pos(), (bx, by));
}

#[test]
fn meditation_regenerates_mana_faster() {
    let s = field();
    let mut ana = s.bot("Ana");
    ready(&mut ana);
    let (x, y) = ana.pos();
    assert!(ana.cast(INVISIBILITY, x, y));
    ana.sync();
    let low = ana.mana();
    let regen = s.data().balance.mana_regen;

    // Without meditating: only the base regeneration.
    s.advance(2000);
    ana.sync();
    let plain = ana.mana() - low;
    assert!(plain <= 2 * regen + regen, "plain regeneration, got {plain}");

    let before = ana.mana();
    ana.send(&C2S::Meditate);
    ana.sync();
    let aid = ana.id();
    assert!(ana.log().iter().any(|m| matches!(m, S2C::Fx { id, fx, .. } if *id == aid && *fx == 153)), "meditation FX (level 1-12)");
    assert!(status_of(&ana, aid).is_some_and(|f| f & status::MEDITATING != 0));
    // +100 ms: the world ticks every 50 ms, so each pulse lands on the next tick.
    s.advance(MEDITATE_WARMUP_MS + 2 * MEDITATE_PULSE_MS + 100);
    ana.sync();
    // DoMeditar, skill 0: 3.5% of max mana per pulse, 3 pulses in that span.
    let max = ana.game.stats.max_mana.get();
    let pulse = (max as f64 * 0.035).round() as i32;
    let gained = ana.mana() - before;
    assert!(gained >= 3 * pulse, "meditation: {gained} >= {}", 3 * pulse);
    assert!(gained > 3 * plain, "faster than plain regeneration");

    // Walking breaks meditation.
    let dir = ana.free_dir(1).unwrap_or(Dir::Down);
    s.advance(300);
    assert!(ana.walk(dir));
    s.advance(100);
    ana.wait_until("deja de meditar", |b| status_of(b, aid).is_some_and(|f| f & status::MEDITATING == 0));
}

#[test]
fn legit_retries_of_refused_casts_are_never_punished() {
    let s = TestServer::start();
    let mut ana = s.bot("Ana");
    ready(&mut ana);
    let (x, y) = ana.pos();
    // Full HP: every heal is refused ("no tiene heridas"); the client retries at its pace.
    for _ in 0..20 {
        ana.cast(HEAL, x, y);
        s.advance(700);
    }
    ana.sync();
    assert_eq!(ana.count(effects), 0);
    assert!(ana.kicked().is_none());
}

#[test]
fn illegal_casts_are_refused() {
    let s = field();
    let mut ana = s.bot("Ana");
    let mut beto = s.bot("Beto");
    let mut carla = s.bot("Carla");
    ready(&mut ana);
    ready(&mut beto);
    ready(&mut carla);
    let (x, y) = ana.pos();

    // Spell not learned / id that does not exist: modified client.
    ana.cast_raw(PVP_ARROW, x, y);
    ana.cast_raw(9999, x, y);
    // Outside the vision window: a legit misclick, no strike; far beyond it: cheat.
    ana.cast_raw(DART, x + VIEW_RANGE_X + 1, y);
    ana.cast_raw(DART, x + 30, y);
    ana.sync();
    assert_eq!(ana.count(effects), 0);
    assert!(said(&ana, "demasiado lejos"));

    // Invalid target: Dardo on an empty tile.
    s.advance(1300);
    let empty = (1..=3).map(|d| (x, y - d)).find(|t| !ana.game.occupied(t.0, t.1)).unwrap_or((x, y - 3));
    ana.cast_raw(DART, empty.0, empty.1);
    ana.sync();
    assert_eq!(ana.count(effects), 0);
    assert!(said(&ana, "Target inválido"));

    // Interval spam: the same spell twice in a row.
    s.advance(1300);
    ana.cast_raw(INVISIBILITY, x, y);
    ana.cast_raw(INVISIBILITY, x, y);
    ana.sync();
    assert_eq!(ana.count(effects), 1, "the second cast inside IntervaloLanzaHechizo is refused");

    // Not enough mana: 2 x 400 spent, Paralizar needs 375 more.
    s.advance(1300);
    let (bx, by) = beto.pos();
    assert!(ana.cast(INVISIBILITY, bx, by));
    ana.sync();
    s.advance(1300);
    ana.sync();
    let (cx, cy) = carla.pos();
    let left = ana.mana();
    assert!(left < 375, "mana left {left}");
    ana.cast_raw(PARALYZE, cx, cy);
    ana.sync();
    assert!(said(&ana, "suficiente maná"));
    assert_eq!(ana.count(effects), 2);
    assert_eq!(ana.mana(), left, "a refused cast costs nothing");
    assert!(ana.kicked().is_none(), "5 strikes stay below the kick limit");
}

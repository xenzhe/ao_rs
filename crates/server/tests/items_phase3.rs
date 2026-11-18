//! Item uses ported in phase 3 (`UseInvItem`): timed strength potion, black potion,
//! empty bottle filled at the water and `/hogar` for ghosts.
use ao_formats::{csm::FLAG_WATER, paths::recursos_dir, AoMap};
use ao_protocol::C2S;
use ao_rules::walk;
use ao_testkit::{Bot, TestServer};

const STRENGTH_POTION: u16 = 39; // DuracionEfecto=85, +2..5
const BLACK_POTION: u16 = 125; // TipoPocion=21
const EMPTY_BOTTLE: u16 = 534; // IndexAbierta=533
const FULL_BOTTLE: u16 = 533;

fn slot_of(b: &Bot, obj: u16) -> Option<u8> {
    b.game.local.inventory.iter().position(|i| i.is_some_and(|i| i.obj == obj)).map(|p| p as u8 + 1)
}

fn give_and_use(s: &TestServer, b: &mut Bot, obj: u16) {
    b.say(&format!("/dar {obj} 1"));
    b.wait_until("objeto", |b| slot_of(b, obj).is_some());
    s.advance(1000);
    b.send(&C2S::UseItem { slot: slot_of(b, obj).unwrap() });
    s.advance(100);
    b.sync();
}

#[test]
fn strength_potion_lasts_its_time_then_wears_off() {
    let s = TestServer::start();
    let mut b = s.bot("Forzudo");
    b.wait_until("atributos", |b| b.game.local.attributes[0] > 0);
    let base = b.game.local.attributes[0];
    give_and_use(&s, &mut b, STRENGTH_POTION);
    b.wait_until("mas fuerte", |b| b.game.local.attributes[0] > base);
    let up = b.game.local.attributes[0];
    assert!((base + 2..=base + 5).contains(&up), "+2..5 strength: {base} -> {up}");
    s.advance(80_000);
    b.sync();
    assert_eq!(b.game.local.attributes[0], up, "still on after 80 s");
    s.advance(6_000);
    b.wait_until("se pasa", |b| b.game.local.attributes[0] == base);
}

#[test]
fn black_potion_kills_and_hogar_takes_the_ghost_home() {
    let s = TestServer::start_at(1, 50, 50);
    let mut b = s.bot("Fantasma");
    b.say("/hogar");
    b.wait_until("solo muertos", |b| b.console().any(|l| l.contains("Debes estar muerto")));
    give_and_use(&s, &mut b, BLACK_POTION);
    b.wait_until("muerto", |b| b.hp() <= 0);
    b.say("/ir 2 50 50");
    b.wait_until("lejos", |b| b.map() == 2);
    b.say("/hogar");
    b.wait_until("viaja", |b| b.console().any(|l| l.contains("Viajaras a tu hogar")));
    for _ in 0..160 {
        s.advance(1000);
    }
    b.wait_until("en casa", |b| b.map() == 1);
}

/// `otChest` subtype 2: "Cofre de Pociones" (obj 206) gives `CantEntrega` of its items.
#[test]
fn a_chest_gives_its_items_and_is_spent() {
    let s = TestServer::start();
    let mut b = s.bot("Afortunado");
    let before = b.game.local.inventory.iter().flatten().count();
    give_and_use(&s, &mut b, 206);
    b.wait_until("abierto", |b| slot_of(b, 206).is_none() && b.game.local.inventory.iter().flatten().count() > before);
}

#[test]
fn an_empty_bottle_is_filled_next_to_water_only() {
    // A dry tile with water within two tiles, on the first maps.
    let root = recursos_dir();
    let (map, at) = (1..=40u16)
        .filter_map(|n| AoMap::load(&root, n).ok().map(|m| (n, m)))
        .find_map(|(n, m)| {
            (5..95).flat_map(|y| (5..95).map(move |x| (x, y))).find(|&(x, y)| walk::standable(&m, x, y) && m.exit(x, y).is_none() && (-1..=1).any(|d| m.blocked(x + d, y + 1) & FLAG_WATER != 0)).map(|p| (n, p))
        })
        .expect("a shore");
    let s = TestServer::start();
    let mut b = s.bot("Sediento");
    give_and_use(&s, &mut b, EMPTY_BOTTLE);
    b.wait_until("lejos del agua", |b| b.console().any(|l| l.contains("acercarte mas al agua")));
    b.say(&format!("/ir {map} {} {}", at.0, at.1));
    b.wait_until("orilla", |b| b.map() == map && b.pos() == at);
    s.advance(1000);
    b.send(&C2S::UseItem { slot: slot_of(&b, EMPTY_BOTTLE).unwrap() });
    b.wait_until("llena", |b| slot_of(b, FULL_BOTTLE).is_some() && slot_of(b, EMPTY_BOTTLE).is_none());
}

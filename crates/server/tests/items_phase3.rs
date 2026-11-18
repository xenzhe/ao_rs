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

/// `OtQuest`: "Mensaje embotellado" (obj 509) offers quest 417 without an NPC; the quest
/// rules still apply (level 25) and the object is only spent when the quest starts.
#[test]
fn a_quest_object_offers_its_quest() {
    use ao_protocol::social::quest::{QuestC2S, QuestS2C};
    let s = TestServer::start();
    let mut b = s.bot("Lector");
    give_and_use(&s, &mut b, 509);
    b.wait_until("oferta", |b| b.log().iter().any(|m| matches!(m, ao_protocol::S2C::Quest(QuestS2C::Offer { quests, .. }) if quests.iter().any(|q| q.info.id == 417))));
    // It asks for level 25: a new character is told so and keeps the object.
    b.send(&C2S::Quest(QuestC2S::Accept { quest: 417 }));
    b.wait_until("rechazada", |b| b.console().any(|l| l.contains("nivel 25")));
    assert!(slot_of(&b, 509).is_some(), "the object is only spent when the quest starts");
    // A forged accept for a quest the object did not offer does nothing.
    b.send(&C2S::Quest(QuestC2S::Accept { quest: 418 }));
    b.sync();
    assert!(!b.console().any(|l| l.contains("Has aceptado")));
}

/// Campfires: wood on the floor + a dagger -> unlit campfire (1 in 3 with no survival
/// skill), a click lights it (1 in 4), never in a safe map.
#[test]
fn wood_and_a_dagger_make_a_campfire_that_a_click_lights() {
    const WOOD: u16 = 58;
    const DAGGER: u16 = 15;
    const UNLIT: u16 = 136;
    const LIT: u16 = 63;
    let s = TestServer::start_at(2, 50, 50);
    let mut b = s.bot("Explorador");
    let here = b.pos();
    b.say(&format!("/dar {WOOD} 9"));
    b.say(&format!("/dar {DAGGER} 1"));
    b.wait_until("leña y daga", |b| slot_of(b, WOOD).is_some() && slot_of(b, DAGGER).is_some());
    s.advance(500);
    b.send(&C2S::DropItem { slot: slot_of(&b, WOOD).unwrap(), amount: 9 });
    b.wait_until("leña en el suelo", |b| b.game.local.ground.get(&here).is_some_and(|g| g.0 == WOOD));
    b.send(&C2S::Interact { tx: here.0, ty: here.1 });
    for _ in 0..60 {
        if b.game.local.ground.get(&here).is_some_and(|g| g.0 == UNLIT) {
            break;
        }
        s.advance(400);
        b.send(&C2S::UseItem { slot: slot_of(&b, DAGGER).unwrap() });
        b.sync();
    }
    assert_eq!(b.game.local.ground.get(&here).map(|g| g.0), Some(UNLIT), "the wood became an unlit campfire");
    for _ in 0..80 {
        if b.game.local.ground.get(&here).is_some_and(|g| g.0 == LIT) {
            break;
        }
        s.advance(400);
        b.send(&C2S::Interact { tx: here.0, ty: here.1 });
        b.sync();
    }
    assert_eq!(b.game.local.ground.get(&here).map(|g| g.0), Some(LIT), "lit: {:?}", b.console().collect::<Vec<_>>());
}

/// Collectible cards: "Carta Xin" (obj 64, card 450, bronze) adds 1 to the collection and
/// is spent; `/cartas` shows it.
#[test]
fn a_collectible_card_joins_the_collection() {
    let s = TestServer::start_at(1, 50, 50);
    let mut b = s.bot("Coleccionista");
    give_and_use(&s, &mut b, 64);
    b.wait_until("en la coleccion", |b| b.console().any(|l| l.contains("(1/10)")));
    assert!(slot_of(&b, 64).is_none(), "the card is spent");
    b.say("/cartas");
    b.wait_until("lista", |b| b.console().any(|l| l.starts_with("Colección de cartas:") && l.contains("1/10")));
}

/// Skins: "Máscara Ying Yang" (obj 1007, helmet skin, Anim=101) changes the helmet drawn
/// and using it again takes it off.
#[test]
fn a_helmet_skin_changes_the_look_and_comes_off() {
    let s = TestServer::start_at(1, 50, 50);
    let mut b = s.bot("Enmascarado");
    let me = b.id();
    let helmet = |b: &Bot| {
        b.log().iter().rev().find_map(|m| match m {
            ao_protocol::S2C::Look { id, helmet, .. } if *id == me => Some(*helmet),
            _ => None,
        })
    };
    give_and_use(&s, &mut b, 1007);
    b.wait_until("mascara", |b| helmet(b) == Some(101));
    s.advance(1000);
    b.send(&C2S::UseItem { slot: slot_of(&b, 1007).unwrap() });
    b.wait_until("sin mascara", |b| helmet(b) == Some(0));
    assert!(slot_of(&b, 1007).is_some(), "a skin is not spent");
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

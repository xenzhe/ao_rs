//! Doors of the official maps (`WorldActions.HandleDoorAction`): a closed door blocks the
//! way in, a click opens it (everyone on the map sees it), a locked one stays shut.
use ao_protocol::{C2S, S2C};
use ao_rules::{Dir, WALK_MS};
use ao_testkit::TestServer;

// Ullathorpe (map 1): "Puerta cerrada" (obj 2334, Subtipo 0) at 72,35 and the locked
// "Puerta 1 cerrada con llave" (obj 53) at 60,23.
const DOOR: (i32, i32) = (72, 35);
const LOCKED: (i32, i32) = (60, 23);

fn door_msgs(b: &ao_testkit::Bot, at: (i32, i32)) -> usize {
    b.count(|m| matches!(m, S2C::MapObject { x, y, .. } if (*x, *y) == at))
}

#[test]
fn a_closed_door_blocks_and_a_click_opens_it_for_everyone() {
    let s = TestServer::start();
    let mut a = s.bot("Portero");
    let mut b = s.bot("Vecino");
    for bot in [&mut a, &mut b] {
        bot.say(&format!("/ir 1 {} {}", DOOR.0, DOOR.1 + 2));
        bot.wait_until("en la puerta", |b| b.map() == 1 && b.pos().1 == DOOR.1 + 2);
    }
    // Up to the tile in front of the door: the closed door stops the next step.
    let (x, _) = a.pos();
    assert_eq!(x, DOOR.0, "landed in front of the door");
    assert!(a.walk(Dir::Up));
    s.advance(WALK_MS);
    a.sync();
    assert_eq!(a.pos(), (DOOR.0, DOOR.1 + 1));
    assert!(!a.walk(Dir::Up), "a closed door blocks the way in");

    a.send(&C2S::Interact { tx: DOOR.0, ty: DOOR.1 });
    a.wait_until("la puerta se abre", |b| door_msgs(b, DOOR) == 1);
    b.wait_until("el vecino lo ve", |b| door_msgs(b, DOOR) == 1);

    for _ in 0..2 {
        s.advance(WALK_MS);
        assert!(a.walk(Dir::Up), "the open door lets the player in");
        a.sync();
    }
    assert_eq!(a.pos(), (DOOR.0, DOOR.1 - 1), "inside the house");

    // Closing it again from inside, and a newcomer gets the door as it is.
    a.send(&C2S::Interact { tx: DOOR.0, ty: DOOR.1 });
    a.wait_until("la puerta se cierra", |b| door_msgs(b, DOOR) == 2);
    let mut c = s.bot("Visita");
    c.say("/ir 1 50 50");
    c.wait_until("llega", |b| b.map() == 1 && door_msgs(b, DOOR) == 1);
}

/// Inventory keys (`Clave` >= 1000): the catacombs door of map 40 (obj 3543, locked) opens
/// with "Llave catas Ullathorpe" (obj 3544), which is spent; the wrong key does nothing.
#[test]
fn the_right_inventory_key_unlocks_a_locked_door() {
    const CATAS: (i32, i32) = (22, 62);
    let s = TestServer::start();
    let mut a = s.bot("Llavero");
    a.say(&format!("/ir 40 {} {}", CATAS.0, CATAS.1 + 2));
    a.wait_until("en la puerta", |b| b.map() == 40 && b.pos().1 == CATAS.1 + 2);
    a.send(&C2S::Interact { tx: CATAS.0, ty: CATAS.1 });
    a.wait_until("cerrada", |b| b.console().any(|l| l.contains("cerrada con llave")));

    let slot_of = |b: &ao_testkit::Bot, obj: u16| b.game.local.inventory.iter().position(|i| i.is_some_and(|i| i.obj == obj)).map(|p| p as u8 + 1);
    // A key of another door (Gran Consejo de Banderbill).
    a.say("/dar 302 1");
    a.wait_until("llave ajena", |b| slot_of(b, 302).is_some());
    a.send(&C2S::UseItem { slot: slot_of(&a, 302).unwrap() });
    a.wait_until("no sirve", |b| b.console().any(|l| l.contains("La llave no sirve")));
    assert_eq!(door_msgs(&a, CATAS), 0);

    a.say("/dar 3544 1");
    a.wait_until("llave buena", |b| slot_of(b, 3544).is_some());
    s.advance(500);
    a.send(&C2S::UseItem { slot: slot_of(&a, 3544).unwrap() });
    a.wait_until("abierta con llave", |b| b.console().any(|l| l.contains("Has abierto la puerta")));
    a.wait_until("se gasta", |b| b.game.local.count_of(3544) == 0);
    assert_eq!(door_msgs(&a, CATAS), 1, "now an ordinary closed door");
    // And now a plain click opens it.
    a.send(&C2S::Interact { tx: CATAS.0, ty: CATAS.1 });
    a.wait_until("se abre", |b| door_msgs(b, CATAS) == 2);
}

#[test]
fn a_locked_door_does_not_open_and_far_clicks_do_nothing() {
    let s = TestServer::start();
    let mut a = s.bot("Ladron");
    a.say(&format!("/ir 1 {} {}", LOCKED.0, LOCKED.1 + 2));
    a.wait_until("en la puerta", |b| b.map() == 1 && b.pos().1 == LOCKED.1 + 2);
    a.send(&C2S::Interact { tx: LOCKED.0, ty: LOCKED.1 });
    a.wait_until("aviso", |b| b.console().any(|l| l.contains("cerrada con llave")));
    assert_eq!(door_msgs(&a, LOCKED), 0);

    a.say(&format!("/ir 1 {} {}", DOOR.0, DOOR.1 + 5));
    a.wait_until("lejos", |b| b.pos().1 == DOOR.1 + 5);
    a.send(&C2S::Interact { tx: DOOR.0, ty: DOOR.1 });
    a.wait_until("aviso", |b| b.console().any(|l| l.contains("demasiado lejos")));
    assert_eq!(door_msgs(&a, DOOR), 0);
}

//! Walking speed (`ActualizarVelocidadDeUsuario`) and mounts (`DoMontar`): the server paces
//! steps by speed, so a rider may walk faster than someone on foot, and a ghost faster
//! still, without the speed-hack check kicking in.
use ao_protocol::{C2S, S2C};
use ao_rules::{step_ms, Dir};
use ao_testkit::{Bot, TestServer};

const SADDLE: u16 = 655; // Montura de Rata, Velocidad=1 (x1.05 for a level-1 rider)
const BLACK_POTION: u16 = 125;

fn slot_of(b: &Bot, obj: u16) -> Option<u8> {
    b.game.local.inventory.iter().position(|i| i.is_some_and(|i| i.obj == obj)).map(|p| p as u8 + 1)
}

fn my_speed(b: &Bot) -> f32 {
    b.log().iter().rev().find_map(|m| match m {
        S2C::Speed { id, speed } if *id == b.id() => Some(*speed),
        _ => None,
    }).unwrap_or(1.0)
}

/// Walks `steps` steps back and forth at `ms` per step; returns whether the bot got kicked.
fn walk_at(s: &TestServer, b: &mut Bot, ms: u64, steps: usize) -> bool {
    let dir = b.free_dir(2).expect("room to walk");
    let back = match dir {
        Dir::Up => Dir::Down,
        Dir::Down => Dir::Up,
        Dir::Left => Dir::Right,
        Dir::Right => Dir::Left,
    };
    for i in 0..steps {
        b.walk_raw(if i % 2 == 0 { dir } else { back });
        s.advance(ms);
        b.sync();
    }
    b.sync();
    b.kicked().is_some()
}

#[test]
fn a_rider_walks_faster_and_a_walker_at_that_pace_is_a_speedhack() {
    let s = TestServer::start_at(2, 50, 50);
    let mut rider = s.bot("Jinete");
    rider.say(&format!("/dar {SADDLE} 1"));
    rider.wait_until("montura", |b| slot_of(b, SADDLE).is_some());
    s.advance(1000);
    rider.send(&C2S::UseItem { slot: slot_of(&rider, SADDLE).unwrap() });
    s.advance(500);
    rider.wait_until("montado", |b| (my_speed(b) - 1.05).abs() < 1e-3);
    // Mounted: no weapon or shield shown, body of the mount.
    assert!(rider.console().any(|l| l.contains("Has montado")));
    let fast = step_ms(1.05);
    assert!(fast < ao_rules::WALK_MS);
    assert!(!walk_at(&s, &mut rider, fast, 40), "a rider at its pace is not a cheat");

    // Someone on foot sending steps at a much faster pace than a walker is.
    let mut cheat = s.bot("Tramposo");
    assert!(walk_at(&s, &mut cheat, step_ms(1.6), 40), "x1.6 on foot is a speed-hack");
}

#[test]
fn ghosts_walk_at_one_point_four() {
    let s = TestServer::start_at(2, 50, 50);
    let mut g = s.bot("Espectro");
    g.say(&format!("/dar {BLACK_POTION} 1"));
    g.wait_until("pocion", |b| slot_of(b, BLACK_POTION).is_some());
    s.advance(1000);
    g.send(&C2S::UseItem { slot: slot_of(&g, BLACK_POTION).unwrap() });
    g.wait_until("muerto", |b| b.hp() <= 0);
    s.advance(500);
    g.wait_until("velocidad de fantasma", |b| (my_speed(b) - 1.4).abs() < 1e-3);
    assert!(!walk_at(&s, &mut g, step_ms(1.4), 40), "a ghost at x1.4 is not a cheat");
}

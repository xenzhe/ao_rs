//! Map entry rules (`CheckMapRestrictions`): a level-1 character is stopped at the exit
//! into a level-ranged dungeon and goes through the one into a newbie map.
use ao_formats::{paths::recursos_dir, AoMap};
use ao_rules::{walk, Dir, WALK_MS};
use ao_testkit::TestServer;

/// An exit on some map leading to a map that matches `want`, with a standable tile next to
/// it from which one step lands on the exit: (map, stand, dir, destination map).
fn exit_into(want: impl Fn(&AoMap) -> bool) -> (u16, (i32, i32), Dir, u16) {
    let root = recursos_dir();
    let maps: Vec<(u16, AoMap)> = (1..=300u16).filter_map(|n| AoMap::load(&root, n).ok().map(|m| (n, m))).collect();
    for (n, m) in &maps {
        for e in &m.exits {
            let Some((_, dest)) = maps.iter().find(|(d, _)| *d == e.map) else { continue };
            if !want(dest) {
                continue;
            }
            for d in Dir::ALL {
                let (dx, dy) = d.delta();
                let stand = (e.x - dx, e.y - dy);
                if walk::standable(m, stand.0, stand.1) && m.exit(stand.0, stand.1).is_none() && walk::can_enter(m, e.x, e.y, d) {
                    return (*n, stand, d, e.map);
                }
            }
        }
    }
    panic!("no such exit");
}

fn try_cross(s: &TestServer, name: &str, (map, stand, dir, _): (u16, (i32, i32), Dir, u16)) -> ao_testkit::Bot {
    let mut b = s.bot(name);
    b.say(&format!("/ir {map} {} {}", stand.0, stand.1));
    b.wait_until("en la salida", |b| b.map() == map && b.pos() == stand);
    s.advance(WALK_MS);
    b.walk(dir);
    for _ in 0..10 {
        s.advance(WALK_MS);
        b.sync();
    }
    b
}

#[test]
fn level_ranged_maps_stop_low_characters_newbie_maps_let_them_in() {
    let s = TestServer::start();
    let high = exit_into(|m| m.min_level > 1);
    let b = try_cross(&s, "Novato", high);
    assert_ne!(b.map(), high.3, "a level-1 character does not enter a level {}+ map", AoMap::load(&recursos_dir(), high.3).unwrap().min_level);
    assert!(b.console().any(|l| l.contains("Necesitas ser al menos nivel")), "{:?}", b.console().collect::<Vec<_>>());

    let newbie = exit_into(|m| m.restrict & ao_formats::csm::RESTRICT_NEWBIE != 0 && m.min_level == 0);
    let b = try_cross(&s, "Novata", newbie);
    assert_eq!(b.map(), newbie.3, "a newbie goes into a newbie map");
}

//! P4 work (mining, woodcutting, fishing, smelting) with bots on the real maps: the resource
//! tiles are found in the official `.csm` objects (`ao_rules::work::parse_map_objects`) and
//! the water in the map layers. Includes the cheats: far, without the tool, on a tile
//! without the resource, interval spam, walking away, smelting as a non-worker.
use ao_formats::csm::{map_path, BLOCK_ALL, FLAG_WATER};
use ao_formats::{recursos_dir, AoMap};
use ao_protocol::p4::work::WorkKind;
use ao_protocol::{Tone, WorkC2S, C2S};
use ao_rules::work::{parse_map_objects, MIN_STA_REQUIRED, WORK_INTERVAL_MS};
use ao_rules::Dir;
use ao_testkit::{Bot, TestServer};

const WORKER: u8 = 9;
/// Hacha de Pino Nudoso (level 1), Piquete de Minero, Caña de Pesca.
const PINE_AXE: u16 = 45;
const PICKAXE: u16 = 187;
const ROD: u16 = 881;
const IRON_ORE: u16 = 192;

fn server() -> TestServer {
    TestServer::with(|s| {
        s.start = Some((1, 50, 50));
        s.npcs = false;
    })
}

fn worker(s: &TestServer, name: &str) -> Bot {
    let mut c = s.connect();
    assert!(c.create_account(&name.to_lowercase(), "secreta1"));
    assert!(c.create_character(name, 1, WORKER, 1, 1));
    c.play(name);
    c.wait_until("en el mundo", |b| b.game.me().is_some() && b.game.local.max_sta > 0);
    c
}

fn say(s: &TestServer, bot: &mut Bot, line: &str) {
    bot.say(line);
    s.advance(50);
    bot.sync();
}

fn send(s: &TestServer, bot: &mut Bot, msg: WorkC2S) {
    bot.send(&C2S::Work(msg));
    s.advance(50);
    bot.sync();
}

fn equip(s: &TestServer, bot: &mut Bot, obj: u16) {
    say(s, bot, &format!("/dar {obj} 1"));
    let slot = bot.game.local.inventory.iter().position(|i| i.is_some_and(|i| i.obj == obj)).expect("given") as u8 + 1;
    bot.send(&C2S::EquipItem { slot });
    s.advance(50);
    bot.sync();
    assert!(bot.game.local.slot(slot).is_some_and(|i| i.equipped), "tool {obj} equipped");
}

fn count(bot: &Bot, obj: u16) -> u32 {
    bot.game.local.count_of(obj)
}

fn said(bot: &Bot, text: &str) -> bool {
    bot.game.console.iter().any(|(t, _): &(String, Tone)| t.contains(text))
}

fn working(bot: &Bot) -> bool {
    bot.game.local.p4.work.active.is_some()
}

fn walkable(m: &AoMap, x: i32, y: i32) -> bool {
    ao_formats::csm::on_map(x, y) && m.blocked(x, y) & (BLOCK_ALL | FLAG_WATER) == 0 && m.exit(x, y).is_none()
}

/// A resource object of type `pick` (obj ids) on `map` with a free orthogonal neighbour:
/// (resource tile, where to stand).
fn spot(map: u16, objs: &[u16]) -> ((i32, i32), (i32, i32)) {
    let root = recursos_dir();
    let m = AoMap::load(&root, map).unwrap();
    let all = parse_map_objects(&std::fs::read(map_path(&root, map)).unwrap()).unwrap();
    for o in all.iter().filter(|o| objs.contains(&o.obj)) {
        for d in Dir::ALL {
            let (dx, dy) = d.delta();
            let (sx, sy) = (o.x + dx, o.y + dy);
            if walkable(&m, sx, sy) && !all.iter().any(|b| (b.x, b.y) == (sx, sy)) {
                return ((o.x, o.y), (sx, sy));
            }
        }
    }
    panic!("no resource {objs:?} reachable on map {map}");
}

/// Water tile with a dry walkable neighbour on `map`.
fn shore(map: u16) -> ((i32, i32), (i32, i32)) {
    let m = AoMap::load(&recursos_dir(), map).unwrap();
    for y in 10..90 {
        for x in 2..100 {
            if m.blocked(x, y) & FLAG_WATER == 0 {
                continue;
            }
            for d in Dir::ALL {
                let (dx, dy) = d.delta();
                if walkable(&m, x + dx, y + dy) {
                    return ((x, y), (x + dx, y + dy));
                }
            }
        }
    }
    panic!("no shore on map {map}");
}

fn go(s: &TestServer, bot: &mut Bot, map: u16, at: (i32, i32)) {
    say(s, bot, &format!("/ir {map} {} {}", at.0, at.1));
    bot.wait_until("teleport", |b| b.map() == map);
    assert_eq!(bot.pos(), at, "standing next to the resource");
}

#[test]
fn woodcutting_with_traps() {
    let s = server();
    let mut bot = worker(&s, "Lenador");
    // Choppable trees of Ullathorpe (obj 59, VidaUtil 600).
    let (tree, stand) = spot(1, &[59]);
    go(&s, &mut bot, 1, stand);

    // Without a tool: refused with the official message.
    send(&s, &mut bot, WorkC2S::Start { kind: WorkKind::Chop, x: tree.0, y: tree.1 });
    assert!(!working(&bot) && said(&bot, "deberías equipártela"));

    equip(&s, &mut bot, PINE_AXE);
    // The pickaxe job with an axe: refused (the server checks the tool against the job).
    send(&s, &mut bot, WorkC2S::Start { kind: WorkKind::Mine, x: tree.0, y: tree.1 });
    assert!(!working(&bot));
    // A tile without a tree (the one the bot stands on).
    send(&s, &mut bot, WorkC2S::Start { kind: WorkKind::Chop, x: stand.0, y: stand.1 });
    assert!(!working(&bot) && said(&bot, "No hay nada para trabajar"));
    // The real tree, but from two tiles away.
    let back = (stand.0 * 2 - tree.0, stand.1 * 2 - tree.1);
    assert!(walkable(bot.game.map.as_ref().unwrap(), back.0, back.1), "a tile behind {stand:?}");
    go(&s, &mut bot, 1, back);
    send(&s, &mut bot, WorkC2S::Start { kind: WorkKind::Chop, x: tree.0, y: tree.1 });
    assert!(!working(&bot) && said(&bot, "Esta demasiado lejos"));
    go(&s, &mut bot, 1, stand);
    bot.game.console.clear();

    // Legit: the work starts and runs once every 4 s, costing 5 stamina per attempt.
    let sta0 = bot.game.local.sta;
    send(&s, &mut bot, WorkC2S::Start { kind: WorkKind::Chop, x: tree.0, y: tree.1 });
    assert!(working(&bot));
    // Spam: ten more starts inside one interval do not add attempts.
    for _ in 0..10 {
        bot.send(&C2S::Work(WorkC2S::Start { kind: WorkKind::Chop, x: tree.0, y: tree.1 }));
    }
    s.advance(WORK_INTERVAL_MS);
    bot.sync();
    let w = &bot.game.local.p4.work;
    assert_eq!(w.misses + w.hits, 1, "exactly one attempt");
    assert_eq!(bot.game.local.sta, sta0 - MIN_STA_REQUIRED);

    // Keep working until a hit (a level-1 Worker gets `GetExtractResourceForLevel(1)` = 0
    // wood, like the VB) or until stamina runs out ("muy cansado").
    for _ in 0..30 {
        if bot.game.local.p4.work.hits > 0 || !working(&bot) {
            break;
        }
        s.advance(WORK_INTERVAL_MS);
        bot.sync();
    }
    let w = &bot.game.local.p4.work;
    assert!(w.hits > 0 || said(&bot, "Estás muy cansado"), "hit or tired: {w:?}");
    assert_eq!(count(&bot, 58), w.gathered_total);

    // Walking away ends the work: no more stamina spent.
    if !working(&bot) {
        // Rest a bit (regeneration) and start again.
        s.advance_secs(60);
        bot.sync();
        send(&s, &mut bot, WorkC2S::Start { kind: WorkKind::Chop, x: tree.0, y: tree.1 });
    }
    assert!(working(&bot));
    let away = Dir::ALL.into_iter().find(|d| {
        let m = bot.game.map.as_ref().unwrap();
        let (dx, dy) = d.delta();
        walkable(m, stand.0 + dx, stand.1 + dy)
    });
    if let Some(d) = away {
        assert!(bot.walk(d));
        s.advance(WORK_INTERVAL_MS);
        bot.sync();
        assert!(!working(&bot), "walking stops the work");
    }
}

#[test]
fn fishing_needs_water_and_the_shore() {
    let s = server();
    let mut bot = worker(&s, "Pescador");
    equip(&s, &mut bot, ROD);
    // Map 9 (north of Ullathorpe) has a coast.
    let (water, stand) = shore(9);
    go(&s, &mut bot, 9, stand);
    // Fishing on dry land.
    send(&s, &mut bot, WorkC2S::Start { kind: WorkKind::Fish, x: stand.0, y: stand.1 });
    assert!(!working(&bot) && said(&bot, "Zona de pesca no Autorizada"));
    send(&s, &mut bot, WorkC2S::Start { kind: WorkKind::Fish, x: water.0, y: water.1 });
    assert!(working(&bot));
    let sta0 = bot.game.local.sta;
    s.advance(WORK_INTERVAL_MS);
    bot.sync();
    assert_eq!(bot.game.local.sta, sta0 - MIN_STA_REQUIRED);
    let w = &bot.game.local.p4.work;
    assert!(w.misses == 1 || w.gathered_total == 1, "one cast: {w:?}");
    // Explicit stop.
    send(&s, &mut bot, WorkC2S::Stop);
    assert!(!working(&bot));
}

#[test]
fn mining_and_smelting_rules() {
    let s = server();
    let mut bot = worker(&s, "Minero");
    equip(&s, &mut bot, PICKAXE);
    // Iron deposits of map 33.
    let (ore, stand) = spot(33, &[188, 189, 190, 191]);
    go(&s, &mut bot, 33, stand);
    send(&s, &mut bot, WorkC2S::Start { kind: WorkKind::Mine, x: ore.0, y: ore.1 });
    assert!(working(&bot));
    let sta0 = bot.game.local.sta;
    s.advance(WORK_INTERVAL_MS);
    bot.sync();
    assert_eq!(bot.game.local.sta, sta0 - MIN_STA_REQUIRED);

    // Smelting in the forge of Ullathorpe (32, 62) without the 25 Mining points: refused.
    say(&s, &mut bot, &format!("/dar {IRON_ORE} 100"));
    let slot = bot.game.local.inventory.iter().position(|i| i.is_some_and(|i| i.obj == IRON_ORE)).unwrap() as u8 + 1;
    go(&s, &mut bot, 1, (33, 63));
    // A forge three tiles away is fine, four is too far.
    send(&s, &mut bot, WorkC2S::Start { kind: WorkKind::Smelt { slot }, x: 32 + 4, y: 63 });
    assert!(!working(&bot));
    send(&s, &mut bot, WorkC2S::Start { kind: WorkKind::Smelt { slot }, x: 32, y: 62 });
    assert!(working(&bot));
    s.advance(WORK_INTERVAL_MS);
    bot.sync();
    assert!(!working(&bot) && said(&bot, "Necesitas 25 puntos en minería"));
    assert_eq!(count(&bot, IRON_ORE), 100, "nothing spent");

    // A non-worker cannot smelt at all.
    let mut mage = s.bot("Mago");
    say(&s, &mut mage, &format!("/dar {IRON_ORE} 100"));
    say(&s, &mut mage, "/ir 1 33 63");
    let slot = mage.game.local.inventory.iter().position(|i| i.is_some_and(|i| i.obj == IRON_ORE)).unwrap() as u8 + 1;
    send(&s, &mut mage, WorkC2S::Start { kind: WorkKind::Smelt { slot }, x: 32, y: 62 });
    assert!(!working(&mage) && said(&mage, "Tu clase no tiene el conocimiento"));
}

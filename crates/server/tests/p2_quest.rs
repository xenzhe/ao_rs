//! P2 quests over the wire: real quest NPCs of the official maps, the official `Quests.DAT`,
//! bots with accounts. Offers, requirements, acceptance, hand-in, rewards paid once, hunting
//! the creatures a quest asks for, persistence across a relog and the ways a client could
//! cheat. (The long quests are also walked through, with kills fed to the hook, by the
//! unit tests in `systems/quest.rs`.)
//!
//! Characters that need a level, a class or items are written to a file database before the
//! server starts, which is the only way to have a level-6 worker without playing to level 6.
use ao_protocol::social::quest::OfferedQuest;
use ao_protocol::{QuestC2S, QuestS2C, C2S, S2C};
use ao_rules::combat::ATTACK_INTERVAL_MS;
use ao_rules::pathfinding::{seek_path, Query};
use ao_rules::{tile_distance, walk, Dir, WALK_MS};
use ao_testkit::{Bot, TestServer};
use server::db::{CharacterState, Db, NewCharacter};
use server::world::quest::{ActiveQuest, QuestSave};

const PASS: &str = "secreta1";
const WARRIOR: u8 = 3;
const WORKER: u8 = 9;

struct Seed {
    user: &'static str,
    name: &'static str,
    class: u8,
    level: u32,
    hp: i32,
    at: (u16, i32, i32),
    /// (slot, obj, amount)
    items: Vec<(u8, u16, u32)>,
    /// Stamina: fighting costs it, and a long hunt needs plenty.
    sta: i32,
    /// A weapon in the hand (obj.dat id): bare fists do 1 or 2 damage.
    weapon: Option<u16>,
    /// Quests already in progress (slot, quest id, kill counters).
    active: Vec<(u8, u16, Vec<u16>)>,
}

impl Seed {
    fn new(name: &'static str, class: u8, level: u32, at: (u16, i32, i32)) -> Seed {
        Seed { user: name, name, class, level, hp: 300, at, items: Vec::new(), sta: 300, weapon: None, active: Vec::new() }
    }
}

/// A server whose database already holds these account characters.
fn server_with(tag: &str, seeds: &[Seed]) -> TestServer {
    let path = std::env::temp_dir().join(format!("ao_p2_quest_{}_{tag}.db", std::process::id()));
    for ext in ["", "-wal", "-shm"] {
        let _ = std::fs::remove_file(format!("{}{ext}", path.display()));
    }
    {
        let db = Db::open(path.to_str().unwrap()).unwrap();
        for s in seeds {
            let acc = db.create_account(s.user, PASS).unwrap();
            let c = db.create_character(acc.id, &NewCharacter { name: s.name, race: 1, class: s.class, gender: 1, body: 1, head: 1, hp: s.hp, mana: 0, map: s.at.0, x: s.at.1, y: s.at.2 }).unwrap();
            let mut rec = c.clone();
            rec.level = s.level;
            let mut inventory: Vec<(u8, u16, u32, bool)> = s.items.iter().map(|(slot, obj, n)| (*slot, *obj, *n, false)).collect();
            if let Some(w) = s.weapon {
                inventory.push((24, w, 1, true));
            }
            let state = CharacterState { inventory, sta: s.sta, max_sta: s.sta, ..CharacterState::default() };
            db.save_full(&rec, &state).unwrap();
            let active = s.active.iter().map(|(slot, quest, killed)| (*slot, ActiveQuest { quest: *quest, killed: killed.clone(), targeted: vec![] })).collect();
            db.save_quests(c.id, &QuestSave { active, done: vec![] }).unwrap();
        }
    }
    // Written against the public repo maps (Makrom 407 with six quests in Ullathorpe).
    TestServer::with(|st| {
        st.config.database = path.to_string_lossy().into_owned();
        if let Some(p) = ao_formats::paths::public_recursos_dir() {
            st.recursos = p;
        }
    })
}

fn play(s: &TestServer, user: &str, name: &str) -> Bot {
    let mut c = s.connect();
    assert!(c.account_login(user, PASS), "login de {user}");
    c.play(name);
    // The first tick with a player on the map creates its official NPCs.
    s.advance(100);
    c.sync();
    c
}

/// A development bot standing next to Makrom on map 1.
fn novice(s: &TestServer, name: &str) -> Bot {
    let mut b = s.bot(name);
    s.advance(100);
    b.sync();
    b
}

/// Leaves the world and comes back (the server saves on the way out).
fn relog(s: &TestServer, bot: Bot, user: &str, name: &str) -> Bot {
    drop(bot);
    s.settle();
    s.advance(100);
    play(s, user, name)
}

/// The client's quest state (`QuestState` of client-core, which this crate cannot name).
macro_rules! quest {
    ($b:expr) => {
        ($b).game.local.social.quest
    };
}

/// The nearest NPC of that name (the map has several Makroms).
fn npc_named(b: &Bot, prefix: &str) -> (u32, i32, i32) {
    let me = b.pos();
    b.creatures()
        .filter(|(_, e)| e.name.starts_with(prefix))
        .min_by_key(|(id, e)| (tile_distance(me, (e.x, e.y)), *id))
        .map(|(id, e)| (id, e.x, e.y))
        .unwrap_or_else(|| panic!("no veo a {prefix}: {:?}", b.creatures().map(|(_, e)| e.name.clone()).collect::<Vec<_>>()))
}

/// Double click on the NPC: the interaction system calls the quest hook.
fn talk(b: &mut Bot, npc: &str) {
    let (_, x, y) = npc_named(b, npc);
    b.send(&C2S::Interact { tx: x, ty: y });
    b.sync();
}

fn console_has(b: &Bot, needle: &str) -> bool {
    b.console().any(|t| t.contains(needle))
}

fn offer_states(b: &Bot) -> Vec<(u16, u8)> {
    quest!(b).offer.as_ref().map(|(_, q)| q.iter().map(|o: &OfferedQuest| (o.info.id, o.state)).collect()).unwrap_or_default()
}

fn quest_msgs(b: &Bot) -> usize {
    b.count(|m| matches!(m, S2C::Quest(_)))
}

fn completed(b: &Bot) -> Vec<u16> {
    b.log().iter().filter_map(|m| if let S2C::Quest(QuestS2C::Completed { quest }) = m { Some(*quest) } else { None }).collect()
}

fn obj_count(b: &Bot, obj: u16) -> u32 {
    b.game.local.inventory.iter().flatten().filter(|i| i.obj == obj).map(|i| i.amount).sum()
}

/// Makrom (NPC 407, map 1 next to the Ullathorpe gate) offers the six "Dungeon Newbie"
/// quests; a level 1 character can only start the first, and everything the client does
/// with it goes through the server's checks.
#[test]
fn makrom_offers_six_quests_and_a_novice_can_take_only_the_first() {
    let s = TestServer::public_at(1, 54, 58);
    let mut b = novice(&s, "Novato");
    talk(&mut b, "Makrom");
    b.wait_until("la lista de misiones", |b| quest!(b).offer.is_some());
    assert_eq!(offer_states(&b), vec![(407, 0), (408, 3), (409, 3), (410, 3), (411, 3), (412, 3)], "available, then locked");
    let info = &quest!(&b).offer.as_ref().unwrap().1[0].info;
    assert_eq!((info.name.as_str(), info.required_level, info.kill.len(), info.reward_exp, info.reward_gold), ("Una nueva aventura", 1, 4, 300, 70));
    assert!(quest!(&b).symbols.values().any(|s| *s == 1), "the NPC shows it has a quest for me");

    // 408 needs quest 407 and level 3: refused with the reason, nothing starts.
    b.send(&C2S::Quest(QuestC2S::Accept { quest: 408 }));
    b.sync();
    assert!(console_has(&b, "Debes completar la misión Una nueva aventura"), "{:?}", b.console().collect::<Vec<_>>());
    assert!(quest!(&b).active.is_empty());

    b.send(&C2S::Quest(QuestC2S::Accept { quest: 407 }));
    b.sync();
    assert_eq!(quest!(&b).active, vec![407]);
    assert!(console_has(&b, "Has aceptado la misión Una nueva aventura"));
    let d = quest!(&b).quest_in(1).expect("details of the new quest");
    assert_eq!((d.info.id, d.killed.clone()), (407, vec![0, 0, 0, 0]));

    // Twice: refused. Details and list answer from the server's truth.
    b.send(&C2S::Quest(QuestC2S::Accept { quest: 407 }));
    b.send(&C2S::Quest(QuestC2S::List));
    b.send(&C2S::Quest(QuestC2S::Details { slot: 1 }));
    b.send(&C2S::Quest(QuestC2S::Details { slot: 4 }));
    b.send(&C2S::Quest(QuestC2S::Details { slot: 0 }));
    b.sync();
    assert!(console_has(&b, "Ya tienes esta misión en curso"));
    assert_eq!(quest!(&b).active, vec![407]);
    assert!(b.kicked().is_none() && !b.closed(), "stale or empty requests are not punished");

    // Not done: talking to him only lists the quests again; nothing is handed in.
    let before = quest_msgs(&b);
    talk(&mut b, "Makrom");
    assert!(quest_msgs(&b) > before && completed(&b).is_empty());
    assert_eq!(offer_states(&b)[0], (407, 1), "in progress");
    assert!(quest!(&b).symbols.values().any(|s| *s == 4));

    b.send(&C2S::Quest(QuestC2S::Abandon { slot: 1 }));
    b.sync();
    assert!(quest!(&b).active.is_empty() && console_has(&b, "Has abandonado la misión Una nueva aventura"));
    assert!(quest!(&b).quest_in(1).is_none());
}

/// Quest 2 (Seraphia, NPC 11): workers of level 6, nothing to do, one item as the reward. It is
/// paid once, remembered after a relog, and the others cannot take it.
#[test]
fn worker_quest_is_paid_once_and_survives_a_relog() {
    const SERAPHIA: (u16, i32, i32) = (1, 77, 34);
    let s = server_with("worker", &[Seed::new("obrero", WORKER, 6, SERAPHIA), Seed::new("guerrero", WARRIOR, 6, SERAPHIA), Seed::new("peon", WORKER, 5, SERAPHIA)]);

    // A warrior of the right level: the quest is for the Trabajador class only (the offer
    // list does not filter by class, the acceptance does).
    let mut w = play(&s, "guerrero", "guerrero");
    talk(&mut w, "Seraphia");
    w.send(&C2S::Quest(QuestC2S::Accept { quest: 2 }));
    w.sync();
    assert!(console_has(&w, "no está disponible para tu clase"), "{:?}", w.console().collect::<Vec<_>>());
    assert!(quest!(&w).active.is_empty());
    drop(w);
    // A worker one level short.
    let mut p = play(&s, "peon", "peon");
    talk(&mut p, "Seraphia");
    p.send(&C2S::Quest(QuestC2S::Accept { quest: 2 }));
    p.sync();
    assert!(console_has(&p, "nivel 6"), "{:?}", p.console().collect::<Vec<_>>());
    assert!(quest!(&p).active.is_empty());
    drop(p);
    s.settle();

    let mut b = play(&s, "obrero", "obrero");
    assert!(quest!(&b).done.is_empty());
    talk(&mut b, "Seraphia");
    assert_eq!(offer_states(&b), vec![(2, 0)]);
    b.send(&C2S::Quest(QuestC2S::Accept { quest: 2 }));
    b.sync();
    assert_eq!(quest!(&b).active, vec![2]);
    assert_eq!(obj_count(&b, 1188), 0);
    // Nothing to deliver, so talking to her again completes it.
    talk(&mut b, "Seraphia");
    assert_eq!(completed(&b), vec![2], "{:?}", b.console().collect::<Vec<_>>());
    assert_eq!(obj_count(&b, 1188), 1, "the reward object");
    assert!(quest!(&b).active.is_empty() && quest!(&b).done == vec![2]);

    // Not twice: talking again, or trying to take it again, pays nothing.
    talk(&mut b, "Seraphia");
    assert_eq!(offer_states(&b), vec![(2, 2)], "already done");
    b.send(&C2S::Quest(QuestC2S::Accept { quest: 2 }));
    b.sync();
    assert!(console_has(&b, "Ya has completado esta misión"));
    assert_eq!((completed(&b).len(), obj_count(&b, 1188), quest!(&b).active.len()), (1, 1, 0));

    // A relog: the quest is still done, the reward is still in the bag, no new payment.
    let mut b = relog(&s, b, "obrero", "obrero");
    assert_eq!(quest!(&b).done, vec![2]);
    assert_eq!(obj_count(&b, 1188), 1);
    talk(&mut b, "Seraphia");
    b.send(&C2S::Quest(QuestC2S::Accept { quest: 2 }));
    b.sync();
    assert!(quest!(&b).active.is_empty() && completed(&b).is_empty() && obj_count(&b, 1188) == 1);
}

/// Quest 326 (Costhas, NPC 111): workers up to level 14 hand in 420 + 220 + 110 + 85 + 8 + 1 of
/// six objects. Handing in what is not there (or one short) does nothing; the exact amounts are
/// taken and the extra stays.
#[test]
fn handing_in_objects_you_do_not_have_does_nothing() {
    const COSTHAS: (u16, i32, i32) = (558, 56, 37);
    let need: [(u16, u32); 6] = [(2089, 420), (2101, 220), (2093, 110), (2155, 85), (3321, 8), (2152, 1)];
    let mut bag: Vec<(u8, u16, u32)> = need.iter().enumerate().map(|(i, (o, n))| (i as u8 + 1, *o, *n)).collect();
    bag[0].2 = 425; // five spare
    bag[5] = (6, 2152, 0); // the last object is missing (amount 0 = not saved)
    let mut seed = Seed::new("pescador", WORKER, 5, COSTHAS);
    seed.items = bag;
    let s = server_with("hand_in", &[seed]);
    let mut b = play(&s, "pescador", "pescador");
    assert_eq!(obj_count(&b, 2152), 0);
    talk(&mut b, "Costhas");
    b.send(&C2S::Quest(QuestC2S::Accept { quest: 326 }));
    b.sync();
    assert_eq!(quest!(&b).active, vec![326], "{:?}", b.console().collect::<Vec<_>>());
    let d = quest!(&b).quest_in(1).unwrap();
    assert_eq!(d.have, vec![425, 220, 110, 85, 8, 0], "what the bag holds, as the server counts it");

    // One object missing: it is not handed in, nothing is taken.
    talk(&mut b, "Costhas");
    assert!(completed(&b).is_empty());
    assert_eq!((obj_count(&b, 2089), obj_count(&b, 2101)), (425, 220));
    // And it is not enough to "have" it on the ground or in the vault: only the bag counts.
    b.say("/dar 2152 1");
    b.sync();
    assert_eq!(obj_count(&b, 2152), 1);
    let level = b.level();
    talk(&mut b, "Costhas");
    assert_eq!(completed(&b), vec![326], "{:?}", b.console().collect::<Vec<_>>());
    assert_eq!(obj_count(&b, 2089), 5, "only what was asked for is taken");
    assert_eq!([2101, 2093, 2155, 3321, 2152].map(|o| obj_count(&b, o)), [0; 5]);
    b.wait_until("la experiencia de la mision", |b| b.level() > level);
    assert!(quest!(&b).active.is_empty() && quest!(&b).done == vec![326]);
}

/// A real hunt: quest 407 asks for 10 rats among other creatures. The bot, who has the quest
/// in progress, goes after the rats of map 176 (which also has giant rats, a different
/// creature that does not count) with his sword; the counter follows on the client, and it
/// comes back after a relog.
#[test]
fn hunting_the_creatures_a_quest_asks_for_counts_kills_and_saves_them() {
    const RAT_MAP: (u16, i32, i32) = (176, 31, 38);
    let mut seed = Seed::new("cazador", WARRIOR, 1, RAT_MAP);
    seed.hp = 2000;
    seed.active = vec![(1, 407, vec![])];
    let s = server_with("hunt", &[seed]);
    let mut b = play(&s, "cazador", "cazador");
    assert_eq!(quest!(&b).active, vec![407], "the saved quest came back");
    b.send(&C2S::Quest(QuestC2S::Details { slot: 1 }));
    b.sync();
    assert_eq!(quest!(&b).quest_in(1).unwrap().killed, vec![0, 0, 0, 0]);

    for _ in 0..4000 {
        if quest!(&b).quest_in(1).is_some_and(|d| d.killed[1] >= 3) {
            break;
        }
        assert!(b.hp() > 0, "el cazador no muere: {:?}", b.console().collect::<Vec<_>>());
        if let Some((_, x, y)) = nearest(&b, 1, "Rata") {
            face(&mut b, (x, y));
            let seq = b.game.next_seq();
            b.send(&C2S::Melee { seq });
        } else if let Some((_, x, y)) = nearest(&b, 60, "Rata") {
            step_towards(&mut b, (x, y));
        }
        s.advance(ATTACK_INTERVAL_MS.max(WALK_MS));
        b.sync();
    }
    let killed = quest!(&b).quest_in(1).expect("details").killed.clone();
    assert!(killed[1] >= 3, "three rats counted, got {killed:?}; at {:?} t={:?}, creatures {:?}", b.pos(), s.now(), b.creatures().map(|(_, e)| (e.name.clone(), e.x, e.y)).collect::<Vec<_>>());
    assert_eq!([killed[0], killed[2], killed[3]], [0, 0, 0], "only rats were hunted, nothing else counts");
    let steps = b.count(|m| matches!(m, S2C::Quest(QuestS2C::Progress { slot: 1, index: 1, .. })));
    assert!(steps >= 3, "one progress message per kill");
    assert!(console_has(&b, "/10 Rata"), "{:?}", b.console().collect::<Vec<_>>());

    // Saved with the character.
    let mut b = relog(&s, b, "cazador", "cazador");
    assert_eq!(quest!(&b).active, vec![407]);
    b.send(&C2S::Quest(QuestC2S::Details { slot: 1 }));
    b.sync();
    assert_eq!(quest!(&b).quest_in(1).unwrap().killed, killed, "the counters came back");
}

/// The whole quest over the wire: quest 407 is one snake short of done; the bot kills a real
/// snake of map 37, walks back to Makrom and is paid exactly once (300 experience, 70 gold).
#[test]
fn the_last_kill_and_the_hand_in_pay_once() {
    const MAKROM: (u16, i32, i32) = (37, 73, 86);
    // Level 33: the silver sword asks for it (quest 407 has no top level).
    let mut seed = Seed::new("heroe", WARRIOR, 33, MAKROM);
    seed.hp = 5000;
    seed.sta = 5000;
    seed.weapon = Some(126); // Espada de Plata, 13 to 19
    seed.active = vec![(1, 407, vec![14, 10, 5, 5])];
    let s = server_with("last_kill", &[seed]);
    let mut b = play(&s, "heroe", "heroe");
    assert!(b.game.local.inventory[23].is_some_and(|i| i.equipped), "sword in hand: {:?} {:?}", b.game.local.inventory[23], b.console().collect::<Vec<_>>());

    // Handing in with a snake missing pays nothing.
    talk(&mut b, "Makrom");
    assert!(completed(&b).is_empty() && b.game.local.gold == 0);

    for _ in 0..6000 {
        if console_has(&b, "15/15 Serpiente") {
            break;
        }
        assert!(b.hp() > 0, "el heroe no muere: {:?}", b.console().collect::<Vec<_>>());
        let adjacent = nearest(&b, 1, "Serpiente").or_else(|| nearest(&b, 1, ""));
        if let Some((_, x, y)) = adjacent {
            face(&mut b, (x, y));
            let seq = b.game.next_seq();
            b.send(&C2S::Melee { seq });
        } else if let Some((_, x, y)) = nearest(&b, 80, "Serpiente") {
            step_towards(&mut b, (x, y));
        }
        s.advance(ATTACK_INTERVAL_MS.max(WALK_MS));
        b.sync();
    }
    assert!(console_has(&b, "15/15 Serpiente"), "no llego a matar la serpiente: {:?}", b.console().collect::<Vec<_>>());
    assert!(console_has(&b, "Ya has matado todas las criaturas que la misión Una nueva aventura requería"));

    // Back to Makrom (fighting whatever is in the way) and hand it in.
    let (_, nx, ny) = npc_named(&b, "Makrom");
    for _ in 0..6000 {
        if tile_distance(b.pos(), (nx, ny)) <= 3 {
            break;
        }
        assert!(b.hp() > 0, "el heroe no muere de vuelta");
        if let Some((_, x, y)) = nearest(&b, 1, "") {
            face(&mut b, (x, y));
            let seq = b.game.next_seq();
            b.send(&C2S::Melee { seq });
        } else {
            step_towards(&mut b, (nx, ny));
        }
        s.advance(ATTACK_INTERVAL_MS.max(WALK_MS));
        b.sync();
    }
    let (_, nx, ny) = npc_named(&b, "Makrom");
    assert!(tile_distance(b.pos(), (nx, ny)) <= 4, "de vuelta junto a Makrom");
    talk(&mut b, "Makrom");
    assert_eq!(completed(&b), vec![407], "{:?}", b.console().collect::<Vec<_>>());
    assert_eq!(b.game.local.gold, 70);
    assert!(quest!(&b).active.is_empty() && quest!(&b).done == vec![407]);
    // Again: nothing more.
    talk(&mut b, "Makrom");
    assert_eq!((completed(&b).len(), b.game.local.gold), (1, 70));
}

/// A client that makes up requests: nothing happens, and enough of them is a kick.
#[test]
fn cheating_with_quest_messages_is_refused_and_punished() {
    let s = TestServer::public_at(1, 54, 58);
    let mut b = novice(&s, "Tramposo");
    // Never spoke to an NPC: a late packet, silently ignored.
    for q in [407, 2, 0, 9999] {
        b.send(&C2S::Quest(QuestC2S::Accept { quest: q }));
    }
    b.sync();
    assert!(quest!(&b).active.is_empty() && b.kicked().is_none() && !b.closed());
    // Spoke to Makrom, then asks for quests he never offered (another NPC's, one that does
    // not exist, the free quests of the whole file): refused, and counted as cheating.
    talk(&mut b, "Makrom");
    for q in 1..=60u16 {
        if b.closed() {
            break;
        }
        if q != 407 {
            b.send(&C2S::Quest(QuestC2S::Accept { quest: q }));
            b.sync();
        }
    }
    assert!(quest!(&b).active.is_empty(), "nothing was started: {:?}", quest!(&b).active);
    assert!(b.closed() && b.kicked().is_some(), "a flood of impossible quests ends in a kick");
}

/// The quest NPC has to be near: from far away nothing is offered and nothing can be taken,
/// and a dead character cannot do any of it.
#[test]
fn quests_need_you_near_the_npc() {
    let s = TestServer::public_at(1, 54, 58);
    let mut b = novice(&s, "Lejano");
    talk(&mut b, "Makrom");
    b.send(&C2S::Quest(QuestC2S::Accept { quest: 407 }));
    b.sync();
    assert_eq!(quest!(&b).active, vec![407]);
    // Walk away (a few tiles south) and try again with another quest from the same NPC.
    b.send(&C2S::Quest(QuestC2S::Abandon { slot: 1 }));
    let dir = Dir::Down;
    for _ in 0..8 {
        assert!(b.walk(dir) || b.free_dir(1).is_some());
        s.advance(WALK_MS);
        b.sync();
    }
    let me = b.pos();
    let (_, nx, ny) = npc_named(&b, "Makrom");
    assert!(tile_distance(me, (nx, ny)) > 5, "far enough: {me:?} vs {:?}", (nx, ny));
    b.send(&C2S::Quest(QuestC2S::Accept { quest: 407 }));
    b.sync();
    assert!(quest!(&b).active.is_empty());
    assert!(console_has(&b, "demasiado lejos"), "{:?}", b.console().collect::<Vec<_>>());
}

// ---------------------------------------------------------------------------- hunting helpers
// (the same ones the P1 exit scenario uses)

fn step_towards(bot: &mut Bot, to: (i32, i32)) -> bool {
    let me = bot.pos();
    let map = bot.game.map.as_ref().expect("map");
    let q = Query { from: me, to, vision: 500, max_steps: 20_000, noise: 0.0, closest: true };
    let game = &bot.game;
    let path = seek_path(&q, |x, y, d| walk::can_enter(map, x, y, d) && ((x, y) == to || !game.occupied(x, y)), || 0);
    let Some(next) = path.and_then(|p| p.first().copied()) else { return false };
    let dir = Dir::towards(next.0 - me.0, next.1 - me.1);
    bot.walk(dir)
}

/// Nearest creature within `max` tiles whose name starts with `prefix` ("" = any that is
/// not a quest NPC).
fn nearest(bot: &Bot, max: i32, prefix: &str) -> Option<(u32, i32, i32)> {
    let me = bot.pos();
    bot.creatures()
        .filter(|(_, e)| (prefix.is_empty() || e.name == prefix) && !e.name.contains("<Quest"))
        .filter(|(_, e)| tile_distance(me, (e.x, e.y)) <= max)
        .map(|(id, e)| (tile_distance(me, (e.x, e.y)), id, e.x, e.y))
        .min()
        .map(|(_, id, x, y)| (id, x, y))
}

fn face(bot: &mut Bot, to: (i32, i32)) {
    let me = bot.pos();
    bot.send(&C2S::Turn { dir: Dir::towards(to.0 - me.0, to.1 - me.1) });
}

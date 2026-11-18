//! P1 exit scenario: a warrior and a mage created through accounts hunt the official
//! creatures of map 2 until both level up, the warrior picks up the real loot, both walk
//! through the map exit into Ullathorpe, find the banker with path finding and the loot
//! ends up in the vault. Cheats along the way are refused. Everything is official AO20
//! data; only `ExpMult` / `DropMult` (Configuracion.ini) are raised so it takes a few kills.
use ao_formats::dat::npcs;
use ao_formats::{recursos_dir, AoMap};
use ao_protocol::{C2S, S2C};
use ao_rules::combat::ATTACK_INTERVAL_MS;
use ao_rules::pathfinding::{seek_path, Query};
use ao_rules::{tile_distance, walk, Dir, WALK_MS};
use ao_testkit::{Bot, TestServer};

const MAGE: u8 = 1;
const WARRIOR: u8 = 3;
const DART: u16 = 1;
const GOLD_OBJ: u16 = 12;
/// Map 2 tiles next to its exit to Ullathorpe (14..16, 10).
const HUNT: (u16, i32, i32) = (2, 15, 12);
const CITY: u16 = 1;

fn character(s: &TestServer, name: &str, class: u8) -> Bot {
    let mut c = s.connect();
    assert!(c.create_account(&name.to_lowercase(), "secreta1"));
    assert!(c.create_character(name, 1, class, 1, 1));
    c.play(name);
    c.wait_until("en el mundo", |b| b.game.me().is_some() && b.game.stats.max_hp.get() > 0);
    c
}

/// One step towards `to` (path finding over what the client knows: map and occupants).
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

/// Walks until within `within` tiles of `to` (or onto it with 0), or the map changes.
/// Returns false if it never got there.
fn try_go(s: &TestServer, bot: &mut Bot, to: (i32, i32), within: i32) -> bool {
    let map0 = bot.map();
    for _ in 0..2000 {
        let (x, y) = bot.pos();
        if bot.map() != map0 || (x - to.0).abs() + (y - to.1).abs() <= within {
            return true;
        }
        step_towards(bot, to);
        s.advance(WALK_MS);
        bot.sync();
    }
    false
}

fn go(s: &TestServer, bot: &mut Bot, to: (i32, i32), within: i32) {
    if try_go(s, bot, to, within) {
        return;
    }
    let me = bot.pos();
    let map = bot.game.map.as_ref().expect("map");
    let q = Query { from: me, to, vision: 500, max_steps: 20_000, noise: 0.0, closest: true };
    let path = seek_path(&q, |x, y, d| walk::can_enter(map, x, y, d) && ((x, y) == to || !bot.game.occupied(x, y)), || 0);
    panic!("{} no llega a {to:?} (esta en {me:?}, mapa {}); camino {path:?}", bot.game.me().map_or("?", |e| e.name.as_str()), bot.map());
}

fn nearest_creature(bot: &Bot, max: i32) -> Option<(u32, i32, i32)> {
    let me = bot.pos();
    bot.creatures().filter(|(_, e)| tile_distance(me, (e.x, e.y)) <= max).map(|(id, e)| (tile_distance(me, (e.x, e.y)), id, e.x, e.y)).min().map(|(_, id, x, y)| (id, x, y))
}

fn face(bot: &mut Bot, to: (i32, i32)) {
    let me = bot.pos();
    bot.send(&C2S::Turn { dir: Dir::towards(to.0 - me.0, to.1 - me.1) });
}

#[test]
fn warrior_and_mage_hunt_level_up_and_store_the_loot() {
    // ExpMult so that one bat or snake is a level: Murcielago/Serpiente GiveEXP.
    let dat = npcs::load(&recursos_dir()).unwrap().data;
    let weakest_exp = ["Murciélago", "Serpiente"].iter().filter_map(|n| dat.iter().find(|(_, d)| d.name == *n).map(|(_, d)| d.exp)).min().unwrap();
    let mult = (500 + weakest_exp - 1) / weakest_exp;
    let s = TestServer::with(|st| {
        st.start = Some(HUNT);
        let b = &mut st.data.balance;
        (b.start_map, b.start_x, b.start_y) = HUNT;
        st.config.world.exp_mult = mult as u32;
        st.config.world.drop_mult = 1_000_000;
        // Bats and snakes poison and these newbies carry no antidote: this test is about
        // levelling and storing the loot, not about poison (tested in systems/npc_owner.rs).
        st.config.world.creature_poison = false;
    });
    let mut war = character(&s, "Guerrero", WARRIOR);
    let mut mag = character(&s, "Maga", MAGE);
    // The Ratas Gigantes of map 2 (npcs.dat 584: 500 HP, 15-30 hits) are not newbie prey.
    war.say("/quitarnpc 584");
    war.wait_until("sin ratas gigantes", |b| b.console().any(|t| t.contains("NPC quitados")));
    assert!(mag.game.spells.iter().any(|sp| sp.id == DART), "a mage starts with Dardo Magico");
    let (war_lvl, mag_lvl) = (war.level(), mag.level());

    // ---- hunt: the creatures come for them; the mage casts, the warrior hits.
    let mut kills = 0;
    for _ in 0..4000 {
        // Experience comes per hit (share of life taken), so keep on until something died.
        if war.level() > war_lvl && mag.level() > mag_lvl && kills >= 1 {
            break;
        }
        {
            if let Some((_, x, y)) = nearest_creature(&mag, 7) {
                mag.cast(DART, x, y);
            } else if let Some((_, x, y)) = nearest_creature(&mag, 30) {
                step_towards(&mut mag, (x, y));
            }
        }
        if let Some((_, x, y)) = nearest_creature(&war, 1) {
            face(&mut war, (x, y));
            let seq = war.game.next_seq();
            war.send(&C2S::Melee { seq });
        } else if let Some((_, x, y)) = nearest_creature(&war, 30) {
            step_towards(&mut war, (x, y));
        }
        // Respawns can bring a Rata Gigante back: out again.
        if war.creatures().any(|(_, e)| e.name == "Rata Gigante") {
            war.say("/quitarnpc 584");
        }
        s.advance(ATTACK_INTERVAL_MS.max(WALK_MS));
        war.sync();
        mag.sync();
        assert!(war.hp() > 0 && mag.hp() > 0, "nadie muere cazando: {:?}", war.console().chain(mag.console()).filter(|t| t.contains("pegado") || t.contains("matado")).collect::<Vec<_>>());
        kills = war.console().chain(mag.console()).filter(|t| t.starts_with("Has matado a")).count();
    }
    assert!(war.level() > war_lvl, "el guerrero sube de nivel");
    assert!(mag.level() > mag_lvl, "la maga sube de nivel");
    assert!(kills >= 1, "algo murio");
    assert!(war.game.local.free_skill_points > 0 || mag.game.local.free_skill_points > 0, "subir da puntos de habilidad");

    // ---- loot: every drop falls (DropMult); the warrior picks up what lies on the map.
    let loot: Vec<((i32, i32), (u16, u32))> = war.game.local.ground.iter().map(|(k, v)| (*k, *v)).collect();
    assert!(!loot.is_empty(), "las criaturas sueltan su botin oficial");
    let gold0 = war.game.local.gold;
    for (tile, (obj, _)) in &loot {
        // Respawned Ratas Gigantes would kill the warrior on the way.
        war.say("/quitarnpc 584");
        war.sync();
        // A drop can lie where nobody stands (under a creature that stays): skip it.
        if !try_go(&s, &mut war, *tile, 0) {
            continue;
        }
        assert!(war.hp() > 0, "muere recogiendo: {:?}", war.console().filter(|t| t.contains("pegado") || t.contains("matado")).collect::<Vec<_>>());
        s.advance(300);
        war.send(&C2S::PickUp);
        war.wait_until("recoge", |b| !b.game.local.ground.contains_key(tile) || b.game.local.count_of(*obj) > 0);
    }
    war.sync();
    let item = war.game.local.inventory.iter().flatten().find(|i| loot.iter().any(|(_, (o, _))| *o == i.obj && *o != GOLD_OBJ)).map(|i| (i.obj, i.amount));
    let (item_obj, item_amount) = item.expect("un objeto del botin en el inventario");
    assert!(war.game.local.gold > gold0 || loot.iter().all(|(_, (o, _))| *o != GOLD_OBJ), "el oro va a la billetera");

    // ---- travel: through the exit at the top into Ullathorpe, then to the banker.
    for bot in [&mut war, &mut mag] {
        go(&s, bot, (15, 10), 0);
        bot.wait_until("entra en Ullathorpe", |b| b.map() == CITY);
    }
    let arrival = war.pos();
    let city = AoMap::load(&recursos_dir(), CITY).unwrap();
    let banker = city.npcs.iter().find(|n| dat.get(n.npc as u32).is_some_and(|d| d.npc_type == npcs::NpcType::Banker)).map(|n| (n.x, n.y)).expect("banquero");
    go(&s, &mut war, banker, 4);

    // Cheat: depositing with the vault closed does nothing.
    let slot = war.game.local.inventory.iter().position(|i| i.is_some_and(|i| i.obj == item_obj)).unwrap() as u8 + 1;
    war.send(&C2S::BankDeposit { slot, amount: item_amount });
    war.sync();
    assert_eq!(war.game.local.count_of(item_obj), item_amount, "boveda cerrada: nada se mueve");

    // ---- vault: open it at the banker and store the loot and the gold.
    war.send(&C2S::Interact { tx: banker.0, ty: banker.1 });
    war.wait_until("boveda abierta", |b| b.game.local.bank.is_some());
    // Cheat: more than owned.
    war.send(&C2S::BankDeposit { slot, amount: item_amount + 1000 });
    war.sync();
    war.send(&C2S::BankDeposit { slot, amount: item_amount });
    war.wait_until("objeto en la boveda", |b| b.game.local.bank.as_ref().is_some_and(|k| k.slots.iter().flatten().any(|i| i.obj == item_obj)));
    let stored = war.game.local.bank.as_ref().unwrap().slots.iter().flatten().filter(|i| i.obj == item_obj).map(|i| i.amount).sum::<u32>();
    assert_eq!(stored, item_amount, "se guarda exactamente lo que tenia, ni una unidad mas");
    assert_eq!(war.game.local.count_of(item_obj), 0);
    let gold = war.game.local.gold;
    if gold > 0 {
        war.send(&C2S::BankDepositGold { amount: gold });
        war.wait_until("oro en la boveda", |b| b.game.local.bank.as_ref().is_some_and(|k| k.gold >= gold));
    }
    assert!(war.kicked().is_none());

    // Walking away (back to the city entrance, far beyond 10 tiles) closes it at the next
    // operation.
    go(&s, &mut war, arrival, 1);
    war.send(&C2S::BankDeposit { slot: 1, amount: 1 });
    war.wait_until("boveda cerrada al alejarse", |b| b.log().iter().any(|m| matches!(m, S2C::BankClosed)));

    // ---- persistence: reconnecting finds the level and the vault as they were.
    let level = war.level();
    drop(war);
    s.settle();
    let mut again = s.connect();
    assert!(again.account_login("guerrero", "secreta1"));
    again.play("Guerrero");
    again.wait_until("de vuelta", |b| b.game.me().is_some() && b.level() == level);
    assert_eq!(again.map(), CITY);
    let (bx, by) = banker;
    go(&s, &mut again, banker, 4);
    again.send(&C2S::Interact { tx: bx, ty: by });
    again.wait_until("la boveda sigue llena", |b| b.game.local.bank.as_ref().is_some_and(|k| k.slots.iter().flatten().any(|i| i.obj == item_obj && i.amount == item_amount)));
}

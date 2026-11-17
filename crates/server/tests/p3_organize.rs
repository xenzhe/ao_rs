//! "Ordenar" / "Juntar" of the inventory window against a real server: the swaps that
//! `client_core::p3::items::plan_organize` plans leave the bag exactly as its model of the
//! server says (merging stacks, moving equipment with its object), the server accepts all of
//! them (nobody is kicked) and a second run has nothing left to do.
use ao_formats::dat::obj::ObjType;
use ao_formats::recursos_dir;
use ao_protocol::C2S;
use ao_testkit::{Bot, TestServer};
use client_core::p3::items::{apply_swap, plan_organize, sort_key, SortMode};

fn give(s: &TestServer, bot: &mut Bot, obj: u16, amount: u32) {
    bot.say(&format!("/dar {obj} {amount}"));
    s.advance(50);
    bot.sync();
}

/// Sends the plan the way the window does (all at once) and waits for the server.
fn send_all(s: &TestServer, bot: &mut Bot, plan: &[(u8, u8)]) {
    for (a, b) in plan {
        bot.send(&C2S::SwapSlots { a: *a, b: *b });
    }
    s.advance(50);
    bot.sync();
}

fn objs_of(bot: &Bot) -> Vec<u16> {
    bot.game.local.inventory.iter().flatten().map(|i| i.obj).collect()
}

#[test]
fn organising_the_bag_matches_the_model_and_is_accepted() {
    let s = TestServer::start_at(1, 77, 49);
    let mut bot = s.bot("Ordenado");
    let objs = ao_formats::dat::obj::load(&recursos_dir()).unwrap().data;
    let free = |o: &ao_formats::dat::obj::ObjData| o.forbidden_classes.is_empty() && o.forbidden_races.is_empty() && !o.male_only && !o.female_only && o.royal == 0 && o.chaos == 0 && o.min_level <= 1 && !o.name.is_empty() && o.grh > 0;
    let pick = |ty: ObjType, nth: usize| objs.iter().filter(|(_, o)| o.obj_type == ty && free(o)).nth(nth).map(|(i, _)| i as u16).expect("an object of the kind");
    let weapon = objs.iter().find(|(_, o)| o.obj_type == ObjType::Weapon && free(o) && o.weapon_anim > 0 && o.two_handed == 0).map(|(i, _)| i as u16).unwrap();
    // Scattered on purpose: potions, a weapon, food, two weapons more, a split stack.
    let stuff = [(pick(ObjType::Potions, 0), 5u32), (weapon, 1), (pick(ObjType::UseOnce, 0), 3), (pick(ObjType::Potions, 1), 2), (pick(ObjType::Armor, 0), 1), (pick(ObjType::Drinks, 0), 4)];
    for (o, n) in stuff {
        give(&s, &mut bot, o, n);
    }
    // Two loose stacks of one object (more than a stack cannot be given at once): 10 000 and 4 000, then 7 000 thrown.
    let rope = pick(ObjType::Wood, 0);
    give(&s, &mut bot, rope, 10_000);
    give(&s, &mut bot, rope, 4_000);
    let first = bot.game.local.inventory.iter().position(|i| i.is_some_and(|i| i.obj == rope)).unwrap() as u8 + 1;
    bot.send(&C2S::DropItem { slot: first, amount: 7_000 });
    s.advance(50);
    bot.sync();
    assert_eq!(bot.game.local.count_of(rope), 7_000, "two stacks, 3 000 and 4 000");
    // Equip the weapon, move something far away.
    let wslot = bot.game.local.inventory.iter().position(|i| i.is_some_and(|i| i.obj == weapon)).unwrap() as u8 + 1;
    bot.send(&C2S::EquipItem { slot: wslot });
    s.advance(50);
    bot.sync();
    bot.send(&C2S::SwapSlots { a: 2, b: 20 });
    s.advance(50);
    bot.sync();
    assert!(bot.game.local.inventory.iter().flatten().any(|i| i.equipped));

    let key = |o: u16| sort_key(o, objs.get(o as u32));
    let before = bot.game.local.inventory.clone();
    let plan = plan_organize(&before, 24, SortMode::Type, &key, true);
    assert!(!plan.is_empty());
    let mut model = before.clone();
    for (a, b) in &plan {
        apply_swap(&mut model, *a as usize - 1, *b as usize - 1);
    }
    send_all(&s, &mut bot, &plan);
    assert!(!bot.closed() && bot.kicked().is_none(), "every swap was accepted: {:?}", bot.console().collect::<Vec<_>>());
    assert_eq!(bot.game.local.inventory, model, "the server did what the plan's model said");
    assert_eq!(bot.game.local.count_of(rope), 7_000);
    assert_eq!(objs_of(&bot).iter().filter(|o| **o == rope).count(), 1, "the wood is one stack now");
    assert!(bot.game.local.inventory.iter().flatten().any(|i| i.equipped && i.obj == weapon), "the weapon is still on");
    let packed = objs_of(&bot).len();
    assert!(bot.game.local.inventory[..packed].iter().all(|i| i.is_some()), "packed at the start");

    // Ordered; a second run has nothing to do. Other modes work on the same bag.
    assert!(plan_organize(&bot.game.local.inventory, 24, SortMode::Type, &key, true).is_empty());
    for mode in [SortMode::Name, SortMode::Value] {
        let plan = plan_organize(&bot.game.local.inventory, 24, mode, &key, true);
        let mut model = bot.game.local.inventory.clone();
        for (a, b) in &plan {
            apply_swap(&mut model, *a as usize - 1, *b as usize - 1);
        }
        send_all(&s, &mut bot, &plan);
        assert_eq!(bot.game.local.inventory, model, "{mode:?}");
        assert!(plan_organize(&bot.game.local.inventory, 24, mode, &key, true).is_empty(), "{mode:?} is stable");
    }
    assert!(!bot.closed() && bot.kicked().is_none());
}

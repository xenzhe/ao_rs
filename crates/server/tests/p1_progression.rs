//! P1 progression: official experience, class/race values and skill points, plus what a
//! client can observe on a running server (hunger and thirst over game time, skill traps).
//! The server-side hooks (`init_new_character`, `on_level_up`, regeneration rules) are tested
//! in-crate next to their code, where they can be called directly.
use ao_formats::dat::balance::{self, Class, Race};
use ao_protocol::{C2S, S2C};
use ao_rules::character::{self, StartEquipment};
use ao_rules::progression::{self, add_exp, exp_for_level, LevelState};
use ao_testkit::TestServer;

fn official() -> Option<balance::Balance> {
    let root = ao_formats::paths::recursos_dir();
    root.join("Dat").join(balance::FILE).is_file().then(|| balance::load(&root).expect("Balance.dat").data)
}

#[test]
fn experience_follows_the_official_table() {
    let b = ao_rules::balance::Balance::default();
    // Balance.dat [EXP]: 500, 750, 1125 ...
    assert_eq!((exp_for_level(&b, 1), exp_for_level(&b, 2), exp_for_level(&b, 3)), (500, 750, 1125));
    assert_eq!(exp_for_level(&b, 46), 396_727_662);
    // 500 + 750 = 1250 exactly: level 3 with nothing left over.
    assert_eq!(add_exp(&b, 1, 0, 1250), LevelState { level: 3, exp: 0, levels_gained: 2 });
    // The cap is level 47 and there is nothing beyond it.
    let total: u64 = progression::EXP_TABLE.iter().map(|&e| e as u64).sum();
    let r = add_exp(&b, 1, 0, u32::MAX);
    assert_eq!(r.level, 47);
    assert_eq!(r.exp, 0);
    assert_eq!(r.levels_gained, 46);
    assert!(total < u32::MAX as u64, "the whole curve fits in one u32 grant, so the loop above reached the cap");
}

#[test]
fn official_creation_values_for_two_combinations() {
    let Some(bal) = official() else { return };
    // Human mage: Str 19, Agi 19, Int 18, Con 20, Cha 18; 20 hp; mana round(18 * 8.33) = 150.
    let a = character::initial_attributes(&bal, Race::Human);
    assert_eq!(a, [19, 19, 18, 20, 18]);
    assert_eq!((character::initial_hp(&a), character::initial_mana(&bal, Class::Mage, &a)), (20, 150));
    // Dwarf cleric: Str 21, Agi 18, Int 15, Con 21, Cha 17; 21 hp; mana round(15 * 2.5) = 38 (37.5 -> even).
    let d = character::initial_attributes(&bal, Race::Dwarf);
    assert_eq!(d, [21, 18, 15, 21, 17]);
    assert_eq!((character::initial_hp(&d), character::initial_mana(&bal, Class::Cleric, &d)), (21, 38));
    assert_eq!(character::START_SKILL_POINTS, 10);
}

#[test]
fn starting_items_and_spells_of_the_official_file() {
    let path = ao_formats::paths::recursos_dir().join("Dat").join("EquipamientoInicial.dat");
    let Ok(bytes) = std::fs::read(path) else { return };
    let eq = StartEquipment::parse(&String::from_utf8_lossy(&bytes));
    let (items, spells) = eq.loadout(2, 42);
    // Cleric: 5 common + 9 own items; the last one (3500) is worn; spells 1 and 7.
    assert_eq!(items.len(), 14);
    assert_eq!(items[13], (14, 3500, 1, true));
    assert_eq!(items.iter().filter(|i| i.3).count(), 1);
    assert_eq!(spells, vec![(1, 1), (2, 7)]);
}

#[test]
fn level_gains_by_class() {
    let Some(bal) = official() else { return };
    // Gnome mage (Int 22): level 5 mana = round(22 * 8.33) + 2.65 * 22 * 4 = 183 + 233.2 = 416.2 -> 416.
    assert_eq!(progression::max_mana(&bal, Class::Mage, 22, 5), 416);
    // Cleric (2.5, mult 2) with Int 15 at level 3: 38 + 2 * 15 * 2 = 98.
    assert_eq!(progression::max_mana(&bal, Class::Cleric, 15, 3), 98);
    assert_eq!(progression::max_stamina(&bal, Class::Cleric, 3), 90);
    assert_eq!(progression::hit_range(&bal, Class::Cleric, 3), (5, 6));
    assert_eq!(progression::skill_points_per_level(&bal, Class::Cleric), 5);
}

#[test]
fn hunger_and_thirst_fall_with_game_time() {
    let s = TestServer::start();
    let mut bot = s.bot("Hambriento");
    bot.sync();
    bot.clear_log();
    s.advance_secs(100);
    bot.sync();
    assert_eq!(bot.count(|m| matches!(m, S2C::Vitals { .. })), 0, "nothing changes in the first 160 s");
    s.advance_secs(61);
    bot.wait_until("sed -10", |b| b.count(|m| matches!(m, S2C::Vitals { water: 90, food: 100, .. })) == 1);
    s.advance_secs(20);
    bot.wait_until("hambre -10", |b| b.count(|m| matches!(m, S2C::Vitals { water: 90, food: 90, .. })) == 1);
}

#[test]
fn skill_assignment_traps_end_in_a_kick() {
    let s = TestServer::start();
    let mut cheat = s.bot("Tramposo");
    // A new character owns no free points yet: any assignment is over budget, and so is a
    // skill that does not exist.
    for i in 0..8 {
        cheat.send(&C2S::AssignSkill { skill: if i % 2 == 0 { 3 } else { 99 }, points: 5 });
    }
    cheat.wait_until("expulsado por trampas de skills", |b| b.kicked().is_some());
    let mut honest = s.bot("Honrado");
    honest.send(&C2S::AssignSkill { skill: 3, points: 0 });
    honest.sync();
    assert!(honest.kicked().is_none(), "assigning nothing is harmless");
}

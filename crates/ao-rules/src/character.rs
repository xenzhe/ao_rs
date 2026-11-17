//! Account and character creation rules (official AO20 values), shared by the server
//! (enforcement) and the client (creation screen), plus the starting values of a new
//! character (VB6 `ConnectNewUser`, `RellenarInventario`).
use ao_formats::dat::balance::{Balance as Official, Class, Race};

/// Max characters per account (Declares.bas `MAX_PERSONAJES`).
pub const MAX_CHARACTERS: usize = 10;

pub const RACES: [(u8, &str); 6] = [(1, "Humano"), (2, "Elfo"), (3, "Drow"), (4, "Gnomo"), (5, "Enano"), (6, "Orco")];

pub const CLASSES: [(u8, &str); 12] = [
    (1, "Mago"),
    (2, "Clerigo"),
    (3, "Guerrero"),
    (4, "Asesino"),
    (5, "Bardo"),
    (6, "Druida"),
    (7, "Paladin"),
    (8, "Cazador"),
    (9, "Trabajador"),
    (10, "Pirata"),
    (11, "Ladron"),
    (12, "Bandido"),
];

pub const MALE: u8 = 1;
pub const FEMALE: u8 = 2;

pub fn valid_race(r: u8) -> bool {
    RACES.iter().any(|(id, _)| *id == r)
}

pub fn valid_class(c: u8) -> bool {
    CLASSES.iter().any(|(id, _)| *id == c)
}

pub fn valid_gender(g: u8) -> bool {
    g == MALE || g == FEMALE
}

/// Starting clothed body by race and gender (TCP.bas `DarCuerpo`).
pub fn starting_body(race: u8, gender: u8) -> u16 {
    match (race, gender) {
        (1, _) => 1,
        (2, _) => 2,
        (3, _) => 3,
        (4, _) | (5, _) => 300,
        (6, FEMALE) => 581,
        (6, _) => 582,
        _ => 1,
    }
}

/// Body with no armour equipped (General.bas `SetNakedBody`).
pub fn naked_body(race: u8, gender: u8) -> u16 {
    let base = if gender == FEMALE { 3006 } else { 3000 };
    let offset = match race {
        3 => 1, // Drow
        2 => 2, // Elfo
        4 => 3, // Gnomo
        5 => 4, // Enano
        6 => 5, // Orco
        _ => 0, // Humano
    };
    base + offset
}

// ---------------------------------------------------------------- new characters

/// Attribute indexes, in the order of VB6 `e_Atributos` (Declares.bas) minus one.
/// Careful: Constitution comes *before* Charisma.
pub const ATTR_STRENGTH: usize = 0;
pub const ATTR_AGILITY: usize = 1;
pub const ATTR_INTELLIGENCE: usize = 2;
pub const ATTR_CONSTITUTION: usize = 3;
pub const ATTR_CHARISMA: usize = 4;

/// Free skill points of a brand-new character (`ConnectNewUser`: `Stats.SkillPts = 10`).
pub const START_SKILL_POINTS: u16 = 10;
/// Hunger and thirst of a new character (`MaxHam = MaxAGU = 100`).
pub const START_FOOD_WATER: i32 = 100;

/// Attributes of a new character (`ConnectNewUser`): 18 plus the race modifier.
pub fn initial_attributes(bal: &Official, race: Race) -> [u8; 5] {
    let m = bal.race(race);
    let mut a = [0u8; 5];
    a[ATTR_STRENGTH] = (18 + m.strength).clamp(0, 255) as u8;
    a[ATTR_AGILITY] = (18 + m.agility).clamp(0, 255) as u8;
    a[ATTR_INTELLIGENCE] = (18 + m.intelligence).clamp(0, 255) as u8;
    a[ATTR_CONSTITUTION] = (18 + m.constitution).clamp(0, 255) as u8;
    a[ATTR_CHARISMA] = (18 + m.charisma).clamp(0, 255) as u8;
    a
}

/// Starting hit points: the Constitution value (`MaxHp = UserAtributos(Constitucion)`).
pub fn initial_hp(attributes: &[u8; 5]) -> i32 {
    attributes[ATTR_CONSTITUTION] as i32
}

/// Starting mana: `Inteligencia * ManaInicial` of the class, rounded like VB6.
pub fn initial_mana(bal: &Official, class: Class, attributes: &[u8; 5]) -> i32 {
    crate::progression::vb_round(attributes[ATTR_INTELLIGENCE] as f64 * bal.class(class).initial_mana)
}

/// Highest value of the stamina dice: `RandomNumber(1, Agilidad \ 6)` (at least 1).
pub fn stamina_dice(attributes: &[u8; 5]) -> i32 {
    (attributes[ATTR_AGILITY] as i32 / 6).max(1)
}

/// Starting stamina for a dice roll in `1..=stamina_dice`: `MaxSta = 20 * MiInt`, where a
/// roll of 1 counts as 2.
pub fn initial_stamina(roll: i32) -> i32 {
    let roll = if roll <= 1 { 2 } else { roll };
    20 * roll
}

/// One starting item of `EquipamientoInicial.dat`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StartItem {
    pub obj: u16,
    pub amount: u32,
    /// Third field of the line: 1 = worn from the start (`AsignarItemInicial`).
    pub equipped: bool,
}

/// `[COMMON]` or `[CLASSn]` section of `EquipamientoInicial.dat`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct StartSet {
    pub items: Vec<StartItem>,
    pub spells: Vec<u16>,
}

/// Starting equipment (`CargarEquipamientoInicial` / `LoadEquipSet`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct StartEquipment {
    pub common: StartSet,
    /// Index 0 = class 1.
    pub classes: Vec<StartSet>,
}

impl StartEquipment {
    /// Parses the file contents (any text encoding: only ASCII is read).
    pub fn parse(text: &str) -> StartEquipment {
        use std::collections::HashMap;
        let mut sections: HashMap<String, HashMap<String, String>> = HashMap::new();
        let mut current = String::new();
        for line in text.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with(';') || line.starts_with('\'') {
                continue;
            }
            if let Some(name) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
                current = name.trim().to_ascii_uppercase();
            } else if let Some((k, v)) = line.split_once('=') {
                sections.entry(current.clone()).or_default().insert(k.trim().to_ascii_uppercase(), v.trim().to_string());
            }
        }
        let load = |name: &str| -> StartSet {
            let Some(s) = sections.get(name) else { return StartSet::default() };
            let num = |key: &str| s.get(key).and_then(|v| v.parse::<usize>().ok()).unwrap_or(0);
            let mut set = StartSet::default();
            for i in 1..=num("NUMOBJS") {
                // "Obj,Amount,Equip"; a malformed line is skipped (the VB logs and moves on).
                let Some(raw) = s.get(&format!("OBJ{i}")) else { continue };
                let parts: Vec<&str> = raw.split(',').map(str::trim).collect();
                if let [o, a, e] = parts[..] {
                    if let (Ok(obj), Ok(amount), Ok(equip)) = (o.parse(), a.parse(), e.parse::<u8>()) {
                        set.items.push(StartItem { obj, amount, equipped: equip == 1 });
                    }
                }
            }
            for i in 1..=num("NUMSPELLS") {
                set.spells.push(s.get(&format!("SPELL{i}")).and_then(|v| v.parse().ok()).unwrap_or(0));
            }
            set
        };
        StartEquipment { common: load("COMMON"), classes: (1..=12).map(|c| load(&format!("CLASS{c}"))).collect() }
    }

    /// Items and spells of a new character of `class` (`RellenarInventario`): the common
    /// set first, then the class set, filling inventory slots 1, 2, 3... A worn item always
    /// has amount 1. Spell slots are assigned from slot 1 by each set in turn, so a later
    /// set overwrites an earlier one exactly like the VB does.
    /// Returns `(inventory: (slot, obj, amount, equipped), spells: (slot, spell id))`.
    #[allow(clippy::type_complexity)]
    pub fn loadout(&self, class: u8, max_slots: usize) -> (Vec<(u8, u16, u32, bool)>, Vec<(u8, u16)>) {
        let class_set = self.classes.get((class as usize).wrapping_sub(1));
        let mut items = Vec::new();
        for it in self.common.items.iter().chain(class_set.into_iter().flat_map(|s| s.items.iter())) {
            if items.len() >= max_slots {
                break;
            }
            let slot = items.len() as u8 + 1;
            items.push((slot, it.obj, if it.equipped { 1 } else { it.amount }, it.equipped));
        }
        let mut spells: Vec<u16> = Vec::new();
        for set in std::iter::once(&self.common).chain(class_set) {
            for (i, s) in set.spells.iter().enumerate() {
                if spells.len() <= i {
                    spells.resize(i + 1, 0);
                }
                spells[i] = *s;
            }
        }
        let spells = spells.into_iter().enumerate().filter(|(_, s)| *s != 0).map(|(i, s)| (i as u8 + 1, s)).collect();
        (items, spells)
    }
}

/// Account user names: 3-20 letters, digits or underscores.
pub fn valid_account_name(user: &str) -> bool {
    (3..=20).contains(&user.chars().count()) && user.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// Passwords: 6-64 characters, no control characters.
pub fn valid_password(pass: &str) -> bool {
    (6..=64).contains(&pass.chars().count()) && !pass.chars().any(|c| c.is_control())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn official_bodies() {
        assert_eq!(starting_body(1, MALE), 1);
        assert_eq!(starting_body(6, MALE), 582);
        assert_eq!(starting_body(6, FEMALE), 581);
        assert_eq!(naked_body(1, MALE), 3000);
        assert_eq!(naked_body(3, FEMALE), 3007);
        assert_eq!(naked_body(6, FEMALE), 3011);
    }

    fn official() -> Option<Official> {
        let root = ao_formats::paths::recursos_dir();
        root.join("Dat").join("Balance.dat").is_file().then(|| ao_formats::dat::balance::load(&root).expect("Balance.dat").data)
    }

    fn equipment() -> Option<StartEquipment> {
        let path = ao_formats::paths::recursos_dir().join("Dat").join("EquipamientoInicial.dat");
        std::fs::read(path).ok().map(|b| StartEquipment::parse(&String::from_utf8_lossy(&b)))
    }

    #[test]
    fn initial_values_by_race_and_class() {
        let Some(bal) = official() else { return };
        // Attributes = 18 + race modifier, in e_Atributos order (Str, Agi, Int, Con, Cha).
        let human = initial_attributes(&bal, Race::Human);
        assert_eq!(human, [19, 19, 18, 20, 18]);
        assert_eq!(initial_attributes(&bal, Race::Gnome), [16, 21, 22, 18, 20]);
        assert_eq!(initial_attributes(&bal, Race::Orc), [21, 19, 16, 20, 17]);
        assert_eq!(initial_attributes(&bal, Race::Drow), [20, 19, 19, 19, 17]);
        assert_eq!(initial_attributes(&bal, Race::Dwarf), [21, 18, 15, 21, 17]);
        assert_eq!(initial_attributes(&bal, Race::Elf), [18, 20, 20, 19, 19]);

        // Hit points = Constitution.
        assert_eq!(initial_hp(&human), 20);
        assert_eq!(initial_hp(&initial_attributes(&bal, Race::Gnome)), 18);
        // Mana = Int * ManaInicial, rounded: human mage 18*8.33 = 149.94 -> 150.
        assert_eq!(initial_mana(&bal, Class::Mage, &human), 150);
        assert_eq!(initial_mana(&bal, Class::Mage, &initial_attributes(&bal, Race::Gnome)), 183, "22*8.33 = 183.26");
        assert_eq!(initial_mana(&bal, Class::Mage, &initial_attributes(&bal, Race::Dwarf)), 125, "15*8.33 = 124.95");
        assert_eq!(initial_mana(&bal, Class::Druid, &initial_attributes(&bal, Race::Elf)), 50, "20*2.5");
        assert_eq!(initial_mana(&bal, Class::Warrior, &human), 0);
        // Stamina: dice 1..=Agi/6 (3 for Agi 18-23), a 1 counts as 2.
        assert_eq!(stamina_dice(&human), 3);
        assert_eq!((initial_stamina(1), initial_stamina(2), initial_stamina(3)), (40, 40, 60));
    }

    #[test]
    fn starting_equipment_of_the_official_file() {
        let Some(eq) = equipment() else { return };
        // Mage: 5 common items (slots 1-5), then its 5 (slots 6-10); the robe (slot 10) is worn.
        let (items, spells) = eq.loadout(1, 42);
        assert_eq!(items.len(), 10);
        assert_eq!(items[0], (1, 4335, 500, false));
        assert_eq!(items[5], (6, 4336, 850, false));
        assert_eq!(items[9], (10, 3502, 1, true));
        assert_eq!(spells, vec![(1, 1), (2, 7), (3, 352)]);
        // Warrior: 15 items, no spells; the worn item is the last one.
        let (items, spells) = eq.loadout(3, 42);
        assert_eq!(items.len(), 15);
        assert_eq!(items[14], (15, 3500, 1, true));
        assert!(spells.is_empty());
        // Assassin and bandit share the same two spells.
        assert_eq!(eq.loadout(4, 42).1, vec![(1, 291), (2, 12)]);
        assert_eq!(eq.loadout(12, 42).1, vec![(1, 291), (2, 12)]);
        // Never more items than slots.
        assert_eq!(eq.loadout(3, 7).0.len(), 7);
    }

    #[test]
    fn equipment_parser_skips_garbage() {
        let eq = StartEquipment::parse("[COMMON]\nNumObjs=3\nObj1=10,5,0\nObj2=oops\nObj3=11,1,1\n[CLASS1]\nNumObjs=1\nObj1=12,2,0\nNumSpells=1\nSpell1=9\n");
        let (items, spells) = eq.loadout(1, 42);
        assert_eq!(items, vec![(1, 10, 5, false), (2, 11, 1, true), (3, 12, 2, false)]);
        assert_eq!(spells, vec![(1, 9)]);
        assert_eq!(eq.loadout(99, 42).0.len(), 2, "unknown class: only the common set");
    }

    #[test]
    fn validation() {
        assert!(valid_race(6) && !valid_race(7) && !valid_race(0));
        assert!(valid_class(12) && !valid_class(13));
        assert!(valid_account_name("xen_zhe") && !valid_account_name("ab") && !valid_account_name("con espacio"));
        assert!(valid_password("secreta1") && !valid_password("corta"));
    }
}

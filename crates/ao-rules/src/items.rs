//! Item rules from the official server (InvUsuario.bas, WorldActions.bas `CanUseObject`):
//! who can use/equip an object, which equipment piece it is, the armour body it gives,
//! what falls on death. Pure functions over obj.dat data.
use ao_formats::dat::balance::{Class, Race, Skill};
use ao_formats::dat::obj::{ObjData, ObjType};

/// `MaxInventoryObjs` (Configuracion.ini): most units per inventory slot or floor tile.
pub const MAX_STACK: u32 = 10_000;
/// `iORO` (Declares.bas): the gold coin object.
pub const GOLD_OBJ: u16 = 12;
/// `LimiteNewbie` (Declares.bas): newbies are level 1..=12.
pub const NEWBIE_MAX_LEVEL: u32 = 12;
/// HandleDrop: at most 100.000 coins per drop request.
pub const MAX_GOLD_DROP: u32 = 100_000;
/// `OroPorNivelBilletera` (Configuracion.ini): gold per level kept on death.
pub const WALLET_GOLD_PER_LEVEL: u32 = 1_000;
/// `MaxHam` / `MaxAGU`: hunger and thirst cap.
pub const MAX_FOOD: i32 = 100;
/// Tilelibre: rings searched around a point for a free floor tile.
pub const FREE_TILE_RINGS: i32 = 15;

/// Equipment piece an object fills (one of each, t_Inventario `Equipped*Slot`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Role {
    Weapon,
    Armor,
    Shield,
    Helmet,
    Ammo,
    Tool,
    Ship,
    Saddle,
    Ring,
    Amulet,
    Backpack,
}

/// Which piece an object type equips as (EquiparInvItem's `Select Case`). Rings and
/// magical instruments share the ring slot like in the VB.
pub fn role_of(t: ObjType) -> Option<Role> {
    Some(match t {
        ObjType::Weapon => Role::Weapon,
        ObjType::Armor => Role::Armor,
        ObjType::Shield => Role::Shield,
        ObjType::Helmet => Role::Helmet,
        ObjType::Arrows => Role::Ammo,
        ObjType::WorkingTools => Role::Tool,
        ObjType::Ships => Role::Ship,
        ObjType::Saddles => Role::Saddle,
        ObjType::RingAccessory | ObjType::MagicalInstrument => Role::Ring,
        ObjType::Amulets => Role::Amulet,
        ObjType::Backpack => Role::Backpack,
        _ => return None,
    })
}

/// What `CanUseObject` needs to know about a character.
#[derive(Clone, Copy, Debug)]
pub struct User<'a> {
    pub race: u8,
    pub class: u8,
    pub gender: u8,
    pub level: u32,
    /// Index 0 = skill 1.
    pub skills: &'a [u8],
}

/// Why an object cannot be used (CanUseObject's codes, first match wins).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Denied {
    MinLevel(i32),
    MaxLevel(i32),
    Faction,
    Class,
    Gender,
    Race,
    Skill(Skill, i32),
}

impl Denied {
    /// The official message text (locale 1926, 1982, 416, 265, 267, 266, NEED_SKILL_POINTS).
    pub fn message(self) -> String {
        match self {
            Denied::MinLevel(l) => format!("Necesitas ser nivel {l} para usar este item."),
            Denied::MaxLevel(l) => format!("Este objeto no puede ser utilizado por personajes de nivel {l} o superior."),
            Denied::Faction => "Tu faccion no te permite utilizarlo.".into(),
            Denied::Class => "Tu clase no puede usar este objeto.".into(),
            Denied::Gender => "Tu sexo no puede usar este objeto.".into(),
            Denied::Race => "Tu raza no puede usar este objeto.".into(),
            Denied::Skill(s, l) => format!("Necesitas {l} puntos en {s:?} para usar este item."),
        }
    }
}

/// WorldActions.bas `CanUseObject` (GM bypass not modelled).
pub fn can_use(o: &ObjData, u: &User) -> Result<(), Denied> {
    if (u.level as i32) < o.min_level {
        return Err(Denied::MinLevel(o.min_level));
    }
    if o.max_level > 0 && u.level as i32 > o.max_level {
        return Err(Denied::MaxLevel(o.max_level));
    }
    // `Not FaccionPuedeUsarItem And JerarquiaPuedeUsarItem`: without factions nobody is
    // armada/caos and nobody has rewards, so faction items with rank 0 are refused.
    if (o.royal == 1 || o.chaos == 1) && o.rank <= 0 {
        return Err(Denied::Faction);
    }
    if !class_can_use(o, u.class) {
        return Err(Denied::Class);
    }
    if !gender_can_use(o, u.gender) {
        return Err(Denied::Gender);
    }
    if !race_can_use(o, u.race, u.gender) {
        return Err(Denied::Race);
    }
    if let Some(req) = o.skill_requirement {
        let have = u.skills.get((req.skill.id() as usize).wrapping_sub(1)).copied().unwrap_or(0) as i32;
        if have < req.level {
            return Err(Denied::Skill(req.skill, req.level));
        }
    }
    Ok(())
}

/// `ClasePuedeUsarItem`.
pub fn class_can_use(o: &ObjData, class: u8) -> bool {
    match Class::from_id(class as i32) {
        Some(c) => !o.forbidden_classes.contains(&c),
        None => true,
    }
}

/// `SexoPuedeUsarItem` (1 = male, 2 = female).
pub fn gender_can_use(o: &ObjData, gender: u8) -> bool {
    if o.female_only {
        gender != 1
    } else if o.male_only {
        gender != 2
    } else {
        true
    }
}

/// `RazaPuedeUsarItem`: forbidden races, per-race flags, and armour must have a body.
pub fn race_can_use(o: &ObjData, race: u8, gender: u8) -> bool {
    let Some(r) = Race::from_id(race as i32) else { return true };
    if o.forbidden_races.contains(&r) {
        return false;
    }
    let ok = if o.race_flags.iter().sum::<i32>() == 0 { true } else { o.race_flags[r.id() as usize - 1] > 0 };
    if ok && o.obj_type == ObjType::Armor {
        return clothing_for(o, race, gender) != 0;
    }
    ok
}

/// `ObtenerRopaje`: the body an armour (or backpack/saddle) gives to this race and gender.
pub fn clothing_for(o: &ObjData, race: u8, gender: u8) -> i32 {
    let Some(r) = Race::from_id(race as i32) else { return o.clothing };
    if o.obj_type == ObjType::Saddles {
        let short = matches!(r, Race::Gnome | Race::Dwarf);
        let v = if short { o.clothing_short } else { o.clothing_tall };
        if v > 0 {
            return v;
        }
    }
    let i = r.id() as usize - 1;
    if gender == 2 && o.clothing_female[i] > 0 {
        return o.clothing_female[i];
    }
    if o.clothing_male[i] > 0 {
        return o.clothing_male[i];
    }
    o.clothing
}

/// `ItemSeCae`: whether an object falls to the floor when its owner dies.
pub fn falls_on_death(o: &ObjData) -> bool {
    (o.royal != 1 || o.never_falls == 0)
        && (o.chaos != 1 || o.never_falls == 0)
        && !matches!(o.obj_type, ObjType::Keys | ObjType::Ships | ObjType::Saddles)
        && o.never_falls == 0
        && o.non_droppable != 1
        && o.destroys != 1
        && o.non_transferable != 1
}

/// `EsNewbie`.
pub fn is_newbie(level: u32) -> bool {
    level <= NEWBIE_MAX_LEVEL
}

/// `IsConsumableFreeZone`, the parts that do not depend on triggers or tiers: castles
/// (maps 758-772) and capture the flag (275-277) do not consume potions.
pub fn consumable_free_map(map: u16) -> bool {
    (758..=772).contains(&map) || (275..=277).contains(&map)
}

/// `e_PotionType` (Declares.bas).
pub mod potion {
    pub const AGILITY: i32 = 1;
    pub const STRENGTH: i32 = 2;
    pub const HP: i32 = 3;
    pub const MP: i32 = 4;
    pub const HEALS_POISON: i32 = 5;
    pub const HEALS_PARALYSIS: i32 = 6;
    pub const STAMINA: i32 = 7;
    pub const HEALS_ALL: i32 = 13;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn user(level: u32) -> User<'static> {
        User { race: 1, class: 1, gender: 1, level, skills: &[0; 24] }
    }

    #[test]
    fn requirements_in_official_order() {
        let mut o = ObjData { min_level: 10, ..ObjData::default() };
        assert_eq!(can_use(&o, &user(5)), Err(Denied::MinLevel(10)));
        assert!(can_use(&o, &user(10)).is_ok());
        o.forbidden_classes = vec![Class::from_id(1).unwrap()];
        assert_eq!(can_use(&o, &user(10)), Err(Denied::Class));
        o.forbidden_classes.clear();
        o.female_only = true;
        assert_eq!(can_use(&o, &user(10)), Err(Denied::Gender));
        o.female_only = false;
        o.race_flags[1] = 1; // elves only
        assert_eq!(can_use(&o, &user(10)), Err(Denied::Race));
    }

    #[test]
    fn armour_body_by_race_and_gender() {
        let mut o = ObjData { obj_type: ObjType::Armor, clothing: 5, ..ObjData::default() };
        o.clothing_male[5] = 70; // orc
        o.clothing_female[5] = 71;
        assert_eq!(clothing_for(&o, 6, 1), 70);
        assert_eq!(clothing_for(&o, 6, 2), 71);
        assert_eq!(clothing_for(&o, 1, 1), 5);
        let bare = ObjData { obj_type: ObjType::Armor, ..ObjData::default() };
        assert!(!race_can_use(&bare, 1, 1), "armour with no body cannot be worn");
    }

    #[test]
    fn what_falls_on_death() {
        assert!(falls_on_death(&ObjData::default()));
        assert!(!falls_on_death(&ObjData { never_falls: 1, ..ObjData::default() }));
        assert!(!falls_on_death(&ObjData { obj_type: ObjType::Ships, ..ObjData::default() }));
        assert!(!falls_on_death(&ObjData { non_droppable: 1, ..ObjData::default() }));
    }
}

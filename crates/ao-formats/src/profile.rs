//! Game numbers that belong to a resource set, not to the engine: naked and dead bodies,
//! which floor graphics are water... AO20 values by default; `init/profile.ini` (written by
//! tools/tsao for Tierras Sagradas) overrides them:
//! ```ini
//! [Bodies]
//! Naked1-1=21        ; gender-race (gender 1 male, 2 female)
//! DeadBody=8
//! DeadHead=500
//! [Water]
//! Ranges=1505-1520   ; layer-1 GRH ranges
//! EmptyLayer2=1      ; and only if layer 2 is empty (0.11.5 `HayAgua`)
//! ```
use crate::ini::Ini;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Profile {
    /// `(gender, race)` -> body with no armour. Missing pairs use the AO20 formula.
    pub naked: HashMap<(u8, u8), u16>,
    pub dead_body: u16,
    pub dead_head: u16,
    pub water: Vec<(u32, u32)>,
    pub water_needs_empty_layer2: bool,
    /// `[Meditate] FxByLevel=15:4,30:5`: below level 15 FX 4, below 30 FX 5... (empty =
    /// the AO20 table of the engine).
    pub meditate_fx: Vec<(u32, u16)>,
    /// `[Combat] Rules=tsao`: Tierras Sagradas hit, evasion and spell formulas plus its PvP
    /// balance (`Dat/pvp_balance.dat`).
    pub tsao_combat: bool,
    /// `[Walk] Bounds=minx,maxx,miny,maxy`: walkable border (None = the engine's).
    pub walk_bounds: Option<(i32, i32, i32, i32)>,
    /// `[Sounds]`: the engine's fixed sound ids for this resource set (None = engine's).
    pub sounds: HashMap<String, u16>,
    /// `[Fx]`: the engine's fixed FX ids for this resource set (`levelup`, `ritual`).
    pub fx: HashMap<String, u16>,
    /// `[Combat]` creature extras of TSAO (`NpcCriticalPct`...); all off by default.
    pub npc_combat: NpcCombatRules,
    /// `[Pets]`: tamed and summoned creatures.
    pub pets: PetRules,
}

/// `[Combat]` keys for fights against creatures (TSAO UserDañoNpc / HechizoPropNPC).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NpcCombatRules {
    /// `NpcCriticalPct` / `NpcCriticalMult`: chance and multiplier of a critical blow or
    /// spell on a creature (0 = none).
    pub critical_pct: i32,
    pub critical_mult: i32,
    /// `SpellVsNpcPct`: spell damage on creatures in % (100 = unchanged).
    pub spell_vs_npc_pct: i32,
    /// `MeleeVsPetPct` / `SpellVsPetPct`: damage on someone's pet in %.
    pub melee_vs_pet_pct: i32,
    pub spell_vs_pet_pct: i32,
    /// `NpcFlatBonus=PALADIN:45`: flat damage added to blows on creatures by class.
    pub flat_bonus: Vec<(String, i32)>,
    /// `NpcParalysisSecs`: how long a paralysed creature stays still (None = the spell's).
    pub paralysis_secs: Option<u32>,
}

impl Default for NpcCombatRules {
    fn default() -> Self {
        NpcCombatRules { critical_pct: 0, critical_mult: 1, spell_vs_npc_pct: 100, melee_vs_pet_pct: 100, spell_vs_pet_pct: 100, flat_bonus: Vec::new(), paralysis_secs: None }
    }
}

/// `[Pets]`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PetRules {
    /// `Max`: tamed + summoned creatures per player (TSAO MAXMASCOTAS 3).
    pub max: usize,
    /// `TrainerMax`: creatures a trainer keeps out at once (MAXMASCOTASENTRENADOR 7).
    pub trainer_max: usize,
    /// `SummonSecs`: life of a summoned creature.
    pub summon_secs: u32,
    /// `TameMod=DRUIDA:6,...,*:10`: the class divisor of the taming skill (ModDomar).
    pub tame_mod: Vec<(String, i32)>,
}

impl Default for PetRules {
    fn default() -> Self {
        PetRules { max: 3, trainer_max: 7, summon_secs: 120, tame_mod: Vec::new() }
    }
}

impl PetRules {
    /// `ModDomar` of a class (uppercase, no accents); 10 when nothing says otherwise.
    pub fn tame_mod_for(&self, class: &str) -> i32 {
        let c = crate::dat::obj::class_key(class);
        self.tame_mod.iter().find(|(k, _)| *k == c).or_else(|| self.tame_mod.iter().find(|(k, _)| k == "*")).map_or(10, |(_, n)| *n).max(1)
    }
}

impl NpcCombatRules {
    pub fn flat_bonus_for(&self, class: &str) -> i32 {
        let c = crate::dat::obj::class_key(class);
        self.flat_bonus.iter().find(|(k, _)| *k == c).map_or(0, |(_, n)| *n)
    }
}

fn class_list(v: &str) -> Vec<(String, i32)> {
    v.split(',').filter_map(|p| p.split_once(':')).filter_map(|(c, n)| Some((crate::dat::obj::class_key(c.trim()), n.trim().parse().ok()?))).collect()
}

/// AO20 `HayAgua` layer-1 ranges.
const AO20_WATER: [(u32, u32); 9] = [(1505, 1520), (124, 139), (24223, 24238), (24303, 24318), (468, 483), (44668, 44683), (24143, 24158), (12628, 12643), (2948, 2963)];

impl Default for Profile {
    fn default() -> Self {
        Profile {
            naked: HashMap::new(),
            dead_body: 829,
            dead_head: 0,
            water: AO20_WATER.to_vec(),
            water_needs_empty_layer2: false,
            meditate_fx: Vec::new(),
            tsao_combat: false,
            walk_bounds: None,
            sounds: HashMap::new(),
            fx: HashMap::new(),
            npc_combat: NpcCombatRules::default(),
            pets: PetRules::default(),
        }
    }
}

fn ranges(v: &str) -> Vec<(u32, u32)> {
    v.split(',').filter_map(|p| p.split_once('-')).filter_map(|(a, b)| Some((a.trim().parse().ok()?, b.trim().parse().ok()?))).collect()
}

impl Profile {
    pub fn parse(ini: &Ini) -> Profile {
        let mut p = Profile::default();
        if let Some(b) = ini.section("bodies") {
            for (k, v) in b {
                if let Some((g, r)) = k.strip_prefix("naked").and_then(|s| s.split_once('-')) {
                    if let (Ok(g), Ok(r), Ok(body)) = (g.parse(), r.parse(), v.parse()) {
                        p.naked.insert((g, r), body);
                    }
                }
            }
            if let Some(v) = b.get("deadbody").and_then(|v| v.parse().ok()) {
                p.dead_body = v;
            }
            if let Some(v) = b.get("deadhead").and_then(|v| v.parse().ok()) {
                p.dead_head = v;
            }
        }
        if let Some(w) = ini.section("water") {
            if let Some(v) = w.get("ranges") {
                p.water = ranges(v);
            }
            p.water_needs_empty_layer2 = w.get("emptylayer2").is_some_and(|v| v.trim() == "1");
        }
        if let Some(v) = ini.section("walk").and_then(|w| w.get("bounds")) {
            let n: Vec<i32> = v.split(',').filter_map(|s| s.trim().parse().ok()).collect();
            if let [a, b, c, d] = n[..] {
                p.walk_bounds = Some((a, b, c, d));
            }
        }
        if let Some(s) = ini.section("sounds") {
            p.sounds = s.iter().filter_map(|(k, v)| Some((k.clone(), v.trim().parse().ok()?))).collect();
        }
        if let Some(s) = ini.section("fx") {
            p.fx = s.iter().filter_map(|(k, v)| Some((k.clone(), v.trim().parse().ok()?))).collect();
        }
        p.tsao_combat = ini.section("combat").and_then(|c| c.get("rules")).is_some_and(|v| v.trim().eq_ignore_ascii_case("tsao"));
        if let Some(c) = ini.section("combat") {
            let int = |k: &str| c.get(k).and_then(|v| v.trim().parse::<i32>().ok());
            let r = &mut p.npc_combat;
            r.critical_pct = int("npccriticalpct").unwrap_or(r.critical_pct);
            r.critical_mult = int("npccriticalmult").unwrap_or(r.critical_mult);
            r.spell_vs_npc_pct = int("spellvsnpcpct").unwrap_or(r.spell_vs_npc_pct);
            r.melee_vs_pet_pct = int("meleevspetpct").unwrap_or(r.melee_vs_pet_pct);
            r.spell_vs_pet_pct = int("spellvspetpct").unwrap_or(r.spell_vs_pet_pct);
            r.paralysis_secs = int("npcparalysissecs").map(|v| v.max(0) as u32);
            if let Some(v) = c.get("npcflatbonus") {
                r.flat_bonus = class_list(v);
            }
        }
        if let Some(c) = ini.section("pets") {
            let int = |k: &str| c.get(k).and_then(|v| v.trim().parse::<i64>().ok());
            let r = &mut p.pets;
            r.max = int("max").map_or(r.max, |v| v.max(0) as usize);
            r.trainer_max = int("trainermax").map_or(r.trainer_max, |v| v.max(0) as usize);
            r.summon_secs = int("summonsecs").map_or(r.summon_secs, |v| v.max(1) as u32);
            if let Some(v) = c.get("tamemod") {
                r.tame_mod = class_list(v);
            }
        }
        if let Some(v) = ini.section("meditate").and_then(|m| m.get("fxbylevel")) {
            p.meditate_fx = v.split(',').filter_map(|p| p.split_once(':')).filter_map(|(l, f)| Some((l.trim().parse().ok()?, f.trim().parse().ok()?))).collect();
        }
        p
    }

    /// Meditation FX for `level` from the profile (`None` = use the engine's table).
    pub fn meditate_fx(&self, level: u32) -> Option<u16> {
        self.meditate_fx.iter().find(|(below, _)| level < *below).or(self.meditate_fx.last()).map(|(_, fx)| *fx)
    }

    pub fn load(recursos: &Path) -> Profile {
        Self::parse(&Ini::load(&recursos.join("init").join("profile.ini")))
    }

    /// Body with no armour (AO20 `SetNakedBody` when the profile does not say).
    pub fn naked_body(&self, race: u8, gender: u8) -> u16 {
        if let Some(b) = self.naked.get(&(gender, race)) {
            return *b;
        }
        let base = if gender == 2 { 3006 } else { 3000 };
        base + match race {
            3 => 1,
            2 => 2,
            4 => 3,
            5 => 4,
            6 => 5,
            _ => 0,
        }
    }

    /// Is a tile with these floor (layer 1) and overlay (layer 2) graphics water?
    pub fn is_water(&self, layer1: u32, layer2: u32) -> bool {
        (!self.water_needs_empty_layer2 || layer2 == 0) && self.water.iter().any(|(a, b)| (*a..=*b).contains(&layer1))
    }
}

/// Loaded once per `Recursos` folder.
pub fn for_recursos(recursos: &Path) -> &'static Profile {
    static CACHE: OnceLock<Mutex<HashMap<PathBuf, &'static Profile>>> = OnceLock::new();
    let mut m = CACHE.get_or_init(Default::default).lock().unwrap();
    m.entry(recursos.to_path_buf()).or_insert_with(|| Box::leak(Box::new(Profile::load(recursos))))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn overrides() {
        let p = Profile::parse(&Ini::parse(b"[Bodies]\nNaked1-1=21\nNaked2-1=39\nDeadBody=8\nDeadHead=500\n[Water]\nRanges=1505-1520\nEmptyLayer2=1\n"));
        assert_eq!((p.naked_body(1, 1), p.naked_body(1, 2), p.naked_body(2, 1)), (21, 39, 3002));
        assert_eq!((p.dead_body, p.dead_head), (8, 500));
        assert!(p.is_water(1510, 0) && !p.is_water(1510, 7) && !p.is_water(130, 0));
        let d = Profile::default();
        assert!(d.is_water(130, 7) && d.dead_body == 829);
    }

    #[test]
    fn pets_and_creature_combat() {
        let p = Profile::parse(&Ini::parse(b"[Combat]\nRules=tsao\nNpcCriticalPct=40\nNpcCriticalMult=2\nNpcFlatBonus=PALADIN:45\nNpcParalysisSecs=50\n[Pets]\nMax=3\nTrainerMax=7\nSummonSecs=120\nTameMod=DRUIDA:6,CLERIGO:7,*:10\n"));
        assert_eq!((p.npc_combat.critical_pct, p.npc_combat.critical_mult, p.npc_combat.paralysis_secs), (40, 2, Some(50)));
        assert_eq!((p.npc_combat.flat_bonus_for("Paladín"), p.npc_combat.flat_bonus_for("Mago")), (45, 0));
        assert_eq!((p.pets.tame_mod_for("Druida"), p.pets.tame_mod_for("Clérigo"), p.pets.tame_mod_for("Guerrero")), (6, 7, 10));
        let d = Profile::default();
        assert_eq!((d.npc_combat.critical_pct, d.npc_combat.spell_vs_npc_pct, d.pets.max), (0, 100, 3));
    }
}

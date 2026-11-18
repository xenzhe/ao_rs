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

#[derive(Clone, Debug, PartialEq)]
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
    /// `[Creation] Races=1,2` / `Classes=1,3`: what a new character may be (empty = all).
    pub creation_races: Vec<u8>,
    pub creation_classes: Vec<u8>,
    /// `[Creation] Hp=21` and `ManaN=base,div` (class id N): starting life and mana
    /// `base + Inteligencia / (2 * div)` (TSAO ConnectNewUser). None = AO20 formulas.
    pub start_hp: Option<i32>,
    pub start_mana: HashMap<u8, (i32, i32)>,
    /// `[Scene]`: where the login / character screens are staged (None = AO20 map 1).
    pub scene: Option<SceneCfg>,
}

/// `[Scene] Map=28 Fire=40,19 TownPath=32,48;45,48 Folk=40,47;46,49 BonfireGrh=123`.
#[derive(Clone, Debug, PartialEq)]
pub struct SceneCfg {
    pub map: u16,
    pub fire: (i32, i32),
    pub town_path: Vec<(f32, f32)>,
    pub folk: Vec<(i32, i32)>,
    pub bonfire_grh: u32,
}

impl Profile {
    pub fn may_create(&self, race: u8, class: u8) -> bool {
        (self.creation_races.is_empty() || self.creation_races.contains(&race)) && (self.creation_classes.is_empty() || self.creation_classes.contains(&class))
    }

    /// Starting mana of a class with this intelligence, if the profile sets life/mana.
    pub fn start_mana_for(&self, class: u8, intelligence: u8) -> i32 {
        match self.start_mana.get(&class) {
            Some((base, div)) if *div > 0 => base + intelligence as i32 / (2 * div),
            Some((base, _)) => *base,
            None => 0,
        }
    }
}

/// AO20 `HayAgua` layer-1 ranges.
const AO20_WATER: [(u32, u32); 9] = [(1505, 1520), (124, 139), (24223, 24238), (24303, 24318), (468, 483), (44668, 44683), (24143, 24158), (12628, 12643), (2948, 2963)];

impl Default for Profile {
    fn default() -> Self {
        Profile { naked: HashMap::new(), dead_body: 829, dead_head: 0, water: AO20_WATER.to_vec(), water_needs_empty_layer2: false, meditate_fx: Vec::new(), tsao_combat: false, walk_bounds: None, sounds: HashMap::new(), fx: HashMap::new(), creation_races: Vec::new(), creation_classes: Vec::new(), start_hp: None, start_mana: HashMap::new(), scene: None }
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
        if let Some(c) = ini.section("creation") {
            let ids = |k: &str| c.get(k).map(|v| v.split(',').filter_map(|s| s.trim().parse().ok()).collect()).unwrap_or_default();
            p.creation_races = ids("races");
            p.creation_classes = ids("classes");
            p.start_hp = c.get("hp").and_then(|v| v.trim().parse().ok());
            for (k, v) in c {
                if let Some(n) = k.strip_prefix("mana").and_then(|n| n.parse::<u8>().ok()) {
                    let f: Vec<i32> = v.split(',').filter_map(|s| s.trim().parse().ok()).collect();
                    p.start_mana.insert(n, (f.first().copied().unwrap_or(0), f.get(1).copied().unwrap_or(0)));
                }
            }
        }
        if let Some(s) = ini.section("scene") {
            let pair = |v: &str| -> Option<(i32, i32)> {
                let (a, b) = v.split_once(',')?;
                Some((a.trim().parse().ok()?, b.trim().parse().ok()?))
            };
            let list = |k: &str| s.get(k).map(|v| v.split(';').filter_map(pair).collect::<Vec<_>>()).unwrap_or_default();
            if let (Some(map), Some(fire)) = (s.get("map").and_then(|v| v.trim().parse().ok()), s.get("fire").and_then(|v| pair(v))) {
                p.scene = Some(SceneCfg {
                    map,
                    fire,
                    town_path: list("townpath").into_iter().map(|(x, y)| (x as f32, y as f32)).collect(),
                    folk: list("folk"),
                    bonfire_grh: s.get("bonfiregrh").and_then(|v| v.trim().parse().ok()).unwrap_or(0),
                });
            }
        }
        p.tsao_combat = ini.section("combat").and_then(|c| c.get("rules")).is_some_and(|v| v.trim().eq_ignore_ascii_case("tsao"));
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
}

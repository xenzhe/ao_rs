//! Hit points per level from a table (TSAO `CheckUserLevel`): instead of AO20's biased
//! random gain, each class and race has targets so every character of that pair ends close
//! to the same life (`VidaMinima..VidaMaxima` at level 50). Loaded from `levels.toml` of the
//! data folder (tools/tsao/levels.py extracts it); without it the AO20 formula applies.
use serde::Deserialize;
use std::collections::HashMap;

#[derive(Deserialize, Clone, Debug, PartialEq)]
#[allow(non_snake_case)]
pub struct RaceLife {
    pub VidaMinima: i32,
    pub VidaMaxima: i32,
    /// Levels 2-20: `RandomVeinteMax` while below the curve to `VidaVeinte`, else `Min`.
    pub RandomVeinteMin: i32,
    pub RandomVeinteMax: i32,
    pub VidaVeinte: i32,
    /// Levels 21-29: `RandomNumber(DefineRandomMin, DefineRandomMax)`.
    pub DefineRandomMin: i32,
    pub DefineRandomMax: i32,
    /// Level 30 and up.
    pub RandomFinal: i32,
}

#[derive(Deserialize, Clone, Debug, PartialEq)]
pub struct ClassLevels {
    /// Hit gain up to level 35 and after (`AumentoHIT`).
    pub hit: [i32; 2],
    /// Mana per point of intelligence each level (`AumentoMANA`).
    pub mana_per_int: f32,
    /// Stamina each level (`AumentoSTA`).
    pub sta: i32,
    pub race: HashMap<String, RaceLife>,
}

/// New characters (TSAO `ConnectNewUser`): fixed life and mana `base + Inteligencia /
/// (2 * div)` per class (the VB rolled `RandomNumber(1, INT) / div`; its average is used).
#[derive(Deserialize, Clone, Debug, Default, PartialEq)]
pub struct StartVitals {
    pub hp: i32,
    /// class -> `[base, div]` (div 0 = no intelligence part). Missing classes start at 0.
    #[serde(default)]
    pub mana: HashMap<String, [i32; 2]>,
}

#[derive(Deserialize, Clone, Debug, Default, PartialEq)]
pub struct LevelTable {
    pub class: HashMap<String, ClassLevels>,
    pub start: Option<StartVitals>,
}

impl StartVitals {
    pub fn mana_for(&self, class: &str, intelligence: u8) -> i32 {
        let n = fold(class);
        match self.mana.iter().find(|(k, _)| fold(k) == n).map(|(_, v)| *v) {
            Some([base, div]) if div > 0 => base + intelligence as i32 / (2 * div),
            Some([base, _]) => base,
            None => 0,
        }
    }
}

impl LevelTable {
    pub fn parse(text: &str) -> Result<LevelTable, String> {
        toml::from_str(text).map_err(|e| e.to_string())
    }

    /// Class by its Spanish name, accents and case ignored ("Paladín" = "Paladin").
    pub fn class(&self, name: &str) -> Option<&ClassLevels> {
        let n = fold(name);
        self.class.iter().find(|(k, _)| fold(k) == n).map(|(_, v)| v)
    }
}

fn fold(s: &str) -> String {
    s.chars()
        .map(|c| match c {
            'á' | 'Á' => 'a',
            'é' | 'É' => 'e',
            'í' | 'Í' => 'i',
            'ó' | 'Ó' => 'o',
            'ú' | 'Ú' => 'u',
            c => c.to_ascii_lowercase(),
        })
        .collect()
}

impl ClassLevels {
    pub fn race(&self, name: &str) -> Option<&RaceLife> {
        let n = fold(name);
        self.race.iter().find(|(k, _)| fold(k) == n).map(|(_, v)| v)
    }

    pub fn hit_gain(&self, level: u32) -> i32 {
        if level > 35 { self.hit[1] } else { self.hit[0] }
    }

    pub fn mana_gain(&self, intelligence: u8) -> i32 {
        (self.mana_per_int * intelligence as f32).round() as i32
    }
}

impl RaceLife {
    /// Life gained on reaching `level` with `max_hp` before it. `roll(lo, hi)` is
    /// `RandomNumber` (inclusive).
    pub fn hp_gain(&self, level: u32, max_hp: i32, roll: &mut impl FnMut(i32, i32) -> i32) -> i32 {
        if level <= 20 {
            if max_hp + (21 - level as i32) * self.RandomVeinteMin < self.VidaVeinte {
                self.RandomVeinteMax
            } else {
                self.RandomVeinteMin
            }
        } else if level < 30 {
            roll(self.DefineRandomMin, self.DefineRandomMax)
        } else {
            self.RandomFinal
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const T: &str = r#"
[class."Guerrero"]
hit = [3, 2]
mana_per_int = 0.0
sta = 15
race."Humano" = { VidaMinima = 555, VidaMaxima = 565, RandomVeinteMin = 10, RandomVeinteMax = 11, VidaVeinte = 225, DefineRandomMin = 11, DefineRandomMax = 12, RandomFinal = 11 }
"#;

    #[test]
    fn warrior_reaches_the_target_band() {
        let t = LevelTable::parse(T).unwrap();
        let c = t.class("guerrero").unwrap();
        let r = c.race("HUMANO").unwrap();
        let mut hp = 20;
        let mut mid = |lo: i32, hi: i32| (lo + hi) / 2;
        for level in 2..=50 {
            hp += r.hp_gain(level, hp, &mut mid);
        }
        // Level 20 lands near VidaVeinte; level 50 inside the class band.
        assert!((540..=580).contains(&hp), "{hp}");
        assert_eq!((c.hit_gain(35), c.hit_gain(36)), (3, 2));
        assert!(t.class("Paladín").is_none());
    }
}

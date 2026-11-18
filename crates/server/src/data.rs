//! Game data loaded from `data/*.toml`, with the repository's files embedded as defaults
//! so the server (and its tests) always start, whatever the working directory.
use ao_rules::balance::Balance;
use ao_rules::creatures::CreatureDef;
use ao_rules::spells::SpellDef;
use serde::Deserialize;
use std::collections::HashSet;
use std::path::Path;

const DEFAULT_SPELLS: &str = include_str!("../../../data/spells.toml");
const DEFAULT_CREATURES: &str = include_str!("../../../data/creatures.toml");
const DEFAULT_BALANCE: &str = include_str!("../../../data/balance.toml");

#[derive(Clone, Debug)]
pub struct GameData {
    pub spells: Vec<SpellDef>,
    pub creatures: Vec<CreatureDef>,
    pub balance: Balance,
    pub levels: Option<ao_rules::level_table::LevelTable>,
}

#[derive(Deserialize)]
struct SpellFile {
    spell: Vec<SpellDef>,
}

#[derive(Deserialize)]
struct CreatureFile {
    creature: Vec<CreatureDef>,
}

fn read_or(dir: &Path, file: &str, default: &str) -> String {
    std::fs::read_to_string(dir.join(file)).unwrap_or_else(|_| default.to_string())
}

impl GameData {
    pub fn load(dir: &Path) -> Result<GameData, String> {
        let spells: SpellFile = toml::from_str(&read_or(dir, "spells.toml", DEFAULT_SPELLS)).map_err(|e| format!("spells.toml: {e}"))?;
        let creatures: CreatureFile = toml::from_str(&read_or(dir, "creatures.toml", DEFAULT_CREATURES)).map_err(|e| format!("creatures.toml: {e}"))?;
        let balance: Balance = toml::from_str(&read_or(dir, "balance.toml", DEFAULT_BALANCE)).map_err(|e| format!("balance.toml: {e}"))?;
        // Optional per-class/race life table (TSAO); absent = AO20 formula.
        let levels = match std::fs::read_to_string(dir.join("levels.toml")) {
            Ok(t) => Some(ao_rules::level_table::LevelTable::parse(&t).map_err(|e| format!("levels.toml: {e}"))?),
            Err(_) => None,
        };
        let data = GameData { spells: spells.spell, creatures: creatures.creature, balance, levels };
        data.validate()?;
        Ok(data)
    }

    /// The embedded defaults (used by tests).
    pub fn defaults() -> GameData {
        GameData::load(Path::new("<embedded>")).expect("embedded data is valid")
    }

    fn validate(&self) -> Result<(), String> {
        let mut ids = HashSet::new();
        for s in &self.spells {
            if !ids.insert(s.id) {
                return Err(format!("spells.toml: id {} repetido", s.id));
            }
            if s.mana < 0 || s.range < 1 || s.power < 0 {
                return Err(format!("spells.toml: valores invalidos en '{}'", s.name));
            }
        }
        let mut ids = HashSet::new();
        for c in &self.creatures {
            if !ids.insert(c.id) {
                return Err(format!("creatures.toml: id {} repetido", c.id));
            }
            if c.hp <= 0 || c.damage[0] > c.damage[1] || c.spawn_weight == 0 {
                return Err(format!("creatures.toml: valores invalidos en '{}'", c.name));
            }
        }
        let b = &self.balance;
        if b.player_bodies.is_empty() || b.player_heads.is_empty() || b.exp_factor == 0 {
            return Err("balance.toml: faltan cuerpos/cabezas o exp_factor = 0".into());
        }
        Ok(())
    }

    pub fn spell(&self, id: u16) -> Option<&SpellDef> {
        self.spells.iter().find(|s| s.id == id)
    }

    pub fn creature(&self, id: u16) -> Option<&CreatureDef> {
        self.creatures.iter().find(|c| c.id == id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn repository_data_is_valid() {
        let d = GameData::defaults();
        assert!(!d.spells.is_empty() && !d.creatures.is_empty());
    }

    #[test]
    fn duplicate_ids_are_rejected() {
        let mut d = GameData::defaults();
        d.spells.push(d.spells[0].clone());
        assert!(d.validate().is_err());
    }
}

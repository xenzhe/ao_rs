//! Tierras Sagradas combat (0.11.5 lineage, SistemaCombate.bas + modHechizos.bas +
//! modBalance.bas). The server uses these instead of the AO20 formulas when the resource
//! profile says `[Combat] Rules=tsao`. Pure: randomness comes in as already rolled values.
use std::collections::HashMap;

fn vb(x: f64) -> i64 {
    crate::combat::vb_round(x)
}

/// `PoderAtaqueArma` / `PoderAtaqueProyectil` / `PoderAtaqueWresterling`: the weapon skill
/// counts alone below 31, then agility joins once, twice and three times.
pub fn attack_power(skill: u8, agility: u8, level: u32, class_mod: f64) -> i64 {
    let (s, a) = (skill as f64, agility as f64);
    let base = match skill {
        0..=30 => s,
        31..=60 => s + a,
        61..=90 => s + 2.0 * a,
        _ => s + 3.0 * a,
    };
    let temp = vb(base * class_mod);
    vb(temp as f64 + 2.5 * (level as f64 - 12.0).max(0.0))
}

/// `PoderEvasion`: `(Tácticas + Tácticas / 33 * Agilidad) * ModificadorEvasion`.
pub fn evasion_power(tactics: u8, agility: u8, level: u32, class_mod: f64) -> i64 {
    let t = tactics as f64;
    let temp = vb((t + t / 33.0 * agility as f64) * class_mod);
    vb(temp as f64 + 2.5 * (level as f64 - 12.0).max(0.0))
}

/// `PoderEvasionEscudo` with a shield: `(Agilidad + MaxDef + 30) * ModEscudo / 2`.
pub fn shield_evasion(agility: u8, shield_max_def: i32, class_mod: f64) -> i64 {
    vb((agility as f64 + shield_max_def as f64 + 30.0) * class_mod / 2.0)
}

/// `ProbExito` of every TSAO hit roll: 10..90.
pub fn hit_chance(attack: i64, evasion: i64) -> i64 {
    vb((50.0 + (attack - evasion) as f64 * 0.4).clamp(10.0, 90.0))
}

/// Spell damage before staves and defences: `Daño + Porcentaje(Daño, 3 * 70)` (the VB
/// once scaled by level; TSAO froze it at 70, so every spell hits for 3.1x its roll).
pub fn spell_base(roll: i32) -> i64 {
    roll as i64 + roll as i64 * 210 / 100
}

/// `StaffAffected` spells: a mage hits for `(70 + StaffDamageBonus)%` with a weapon and
/// 70% without; a druid for 73% without a staff bonus, `(65 + bonus)%` with one. Other
/// classes are not changed.
pub fn staff_scaled(damage: i64, mage: bool, druid: bool, weapon_staff_bonus: Option<i32>) -> i64 {
    if mage {
        match weapon_staff_bonus {
            Some(b) => damage * (b as i64 + 70) / 100,
            None => vb(damage as f64 * 0.7),
        }
    } else if druid {
        match weapon_staff_bonus.unwrap_or(0) {
            0 => damage * 73 / 100,
            b => damage * (b as i64 + 65) / 100,
        }
    } else {
        damage
    }
}

/// TSAO `Balance.dat` player-versus-player layer (modBalance.bas): percents by class for
/// physical / ranged / magic attack and physical / magic defence, plus a flat bonus by
/// attacker class against victim class (`[PALADIN] AFGuerrero=28`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PvpBalance {
    pub physical_attack: HashMap<String, f64>,
    pub magic_attack: HashMap<String, f64>,
    pub physical_defense: HashMap<String, f64>,
    pub magic_defense: HashMap<String, f64>,
    pub ranged_attack: HashMap<String, f64>,
    /// `(attacker, victim)` -> flat physical / magic damage.
    pub physical_vs: HashMap<(String, String), i64>,
    pub magic_vs: HashMap<(String, String), i64>,
}

fn key(class: &str) -> String {
    class
        .chars()
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

impl PvpBalance {
    /// The TSAO `Balance.dat` text.
    pub fn parse(text: &str) -> PvpBalance {
        let mut b = PvpBalance::default();
        let mut section = String::new();
        for line in text.lines() {
            let line = line.split('\'').next().unwrap_or("").trim();
            if let Some(s) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
                section = key(s);
                continue;
            }
            let Some((k, v)) = line.split_once('=') else { continue };
            let (k, v) = (key(k.trim()), v.trim().replace(',', "."));
            let Ok(n) = v.parse::<f64>() else { continue };
            let table = match section.as_str() {
                "ataquefisico" => Some(&mut b.physical_attack),
                "ataquemagico" => Some(&mut b.magic_attack),
                "defensafisica" => Some(&mut b.physical_defense),
                "defensamagica" => Some(&mut b.magic_defense),
                "ataqueproyectil" => Some(&mut b.ranged_attack),
                _ => None,
            };
            if let Some(t) = table {
                t.insert(k, n);
            } else if let Some(victim) = k.strip_prefix("af") {
                b.physical_vs.insert((section.clone(), victim.to_string()), n as i64);
            } else if let Some(victim) = k.strip_prefix("am") {
                b.magic_vs.insert((section.clone(), victim.to_string()), n as i64);
            }
        }
        b
    }

    fn pct(t: &HashMap<String, f64>, class: &str) -> f64 {
        t.get(&key(class)).copied().unwrap_or(0.0)
    }

    /// `Round(d + d * p / 100)`.
    fn plus(d: i64, p: f64) -> i64 {
        vb(d as f64 + d as f64 * p / 100.0)
    }

    /// Physical hit of `attacker` on `victim` (class names), after armour.
    pub fn physical(&self, damage: i64, attacker: &str, victim: &str, ranged: bool) -> i64 {
        let mut d = Self::plus(damage, Self::pct(&self.physical_attack, attacker));
        if ranged {
            d = Self::plus(d, Self::pct(&self.ranged_attack, attacker));
        }
        d = Self::plus(d, -Self::pct(&self.physical_defense, victim));
        d + self.physical_vs.get(&(key(attacker), key(victim))).copied().unwrap_or(0)
    }

    /// Spell damage of `attacker` on `victim`, before magic defence of items.
    pub fn magic(&self, damage: i64, attacker: &str, victim: &str) -> i64 {
        let d = Self::plus(damage, Self::pct(&self.magic_attack, attacker));
        let d = Self::plus(d, -Self::pct(&self.magic_defense, victim));
        d + self.magic_vs.get(&(key(attacker), key(victim))).copied().unwrap_or(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formulas_by_hand() {
        // Skill 100, AGI 20, level 40, warrior 0.93: (100 + 60) * 0.93 = 148.8 -> 149; + 70.
        assert_eq!(attack_power(100, 20, 40, 0.93), 219);
        assert_eq!(attack_power(20, 20, 1, 0.93), 19);
        // Tactics 100, AGI 20: (100 + 100/33*20) * 0.82 = 131.7 -> 132.
        assert_eq!(evasion_power(100, 20, 1, 0.82), 132);
        assert_eq!(shield_evasion(20, 10, 0.92), 28);
        assert_eq!((hit_chance(0, 1000), hit_chance(1000, 0), hit_chance(100, 100)), (10, 90, 50));
        assert_eq!(spell_base(10), 31);
        assert_eq!(staff_scaled(100, true, false, None), 70);
        assert_eq!(staff_scaled(100, true, false, Some(20)), 90);
        assert_eq!(staff_scaled(100, false, true, None), 73);
        assert_eq!(staff_scaled(100, false, false, None), 100);
    }

    #[test]
    fn pvp_layer() {
        let b = PvpBalance::parse("[AtaqueFisico]\nGuerrero=-15\nPaladin=25\n[DefensaFisica]\nClerigo=-3\n[PALADIN]\nAFGuerrero=28\nAMMago=-10\n[AtaqueMagico]\nDruida=16\n");
        // 100 * 1.25 = 125, victim guerrero no defence, +28 flat.
        assert_eq!(b.physical(100, "Paladín", "Guerrero", false), 153);
        assert_eq!(b.physical(100, "Guerrero", "Clérigo", false), 88);
        assert_eq!(b.magic(100, "Druida", "Mago"), 116);
        assert_eq!(b.magic(100, "Paladin", "Mago"), 90);
    }
}

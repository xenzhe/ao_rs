//! `Cofres.dat` (TSAO): what a chest object (`TipoCofre=N`) gives when used.
//! `Random=1`: one entry, picked with weight `prob` (the VB re-rolled until one passed its
//! probability: the same distribution, without the loop). `Random=0`: every entry.
//! `SwapN=Class1|Class2:from-to-amount` (Numic, written by tools/tsao from the cases the VB
//! had in code) changes an entry for some classes.
use std::collections::HashMap;
use std::path::Path;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ChestEntry {
    pub obj: u16,
    pub amount: u32,
    pub prob: u32,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ChestSwap {
    /// Spanish class names, accent-free lower case.
    pub classes: Vec<String>,
    pub from: u16,
    pub to: u16,
    /// 0 = keep the entry's amount.
    pub amount: u32,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Chest {
    pub random: bool,
    pub entries: Vec<ChestEntry>,
    pub swaps: Vec<ChestSwap>,
}

pub fn fold(s: &str) -> String {
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

pub fn parse(text: &str) -> HashMap<u32, Chest> {
    let mut out: HashMap<u32, Chest> = HashMap::new();
    let mut cur: Option<u32> = None;
    for line in text.lines() {
        let line = line.split('\'').next().unwrap_or("").trim();
        if let Some(s) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
            cur = s.to_ascii_uppercase().strip_prefix("COFRE").and_then(|n| n.parse().ok());
            continue;
        }
        let (Some(c), Some((k, v))) = (cur, line.split_once('=')) else { continue };
        let (k, v) = (k.trim().to_ascii_uppercase(), v.trim());
        let chest = out.entry(c).or_default();
        if k == "RANDOM" {
            chest.random = v == "1";
        } else if k.starts_with("OBJ") {
            let f: Vec<u32> = v.split('-').filter_map(|p| p.trim().parse().ok()).collect();
            if let [obj, amount, prob, ..] = f[..] {
                chest.entries.push(ChestEntry { obj: obj as u16, amount, prob });
            }
        } else if k.starts_with("SWAP") {
            if let Some((cls, rest)) = v.split_once(':') {
                let f: Vec<u32> = rest.split('-').filter_map(|p| p.trim().parse().ok()).collect();
                if f.len() >= 2 {
                    chest.swaps.push(ChestSwap { classes: cls.split('|').map(fold).collect(), from: f[0] as u16, to: f[1] as u16, amount: f.get(2).copied().unwrap_or(0) });
                }
            }
        }
    }
    out
}

pub fn load(recursos: &Path) -> HashMap<u32, Chest> {
    let bytes = std::fs::read(recursos.join("Dat").join("Cofres.dat")).unwrap_or_default();
    parse(&bytes.iter().map(|&b| b as char).collect::<String>())
}

impl Chest {
    /// What opening it gives a character of `class` (Spanish name). `pick(total)` returns
    /// 1..=total (only used for random chests).
    pub fn open(&self, class: &str, pick: &mut impl FnMut(u32) -> u32) -> Vec<(u16, u32)> {
        let class = fold(class);
        let swap = |obj: u16, amount: u32| {
            self.swaps.iter().find(|s| s.from == obj && s.classes.contains(&class)).map_or((obj, amount), |s| (s.to, if s.amount > 0 { s.amount } else { amount }))
        };
        if self.random {
            let total: u32 = self.entries.iter().map(|e| e.prob).sum();
            if total == 0 {
                return Vec::new();
            }
            let mut r = pick(total);
            for e in &self.entries {
                if r <= e.prob {
                    return vec![swap(e.obj, e.amount)];
                }
                r -= e.prob;
            }
            Vec::new()
        } else {
            self.entries.iter().map(|e| swap(e.obj, e.amount)).collect()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn random_and_full_chests_with_swaps() {
        let c = parse("[INIT]\nNumCofres=2\n[COFRE1]\nRandom=1\nOBJ1=10-1-5\nOBJ2=11-2-95\n[COFRE3]\nRandom=0\nOBJ1=206-1-100\nOBJ2=207-50-100\nSwap1=Guerrero|Cazador:207-551-100\n");
        let r = &c[&1];
        assert_eq!(r.open("Mago", &mut |_| 3), vec![(10, 1)]);
        assert_eq!(r.open("Mago", &mut |_| 6), vec![(11, 2)]);
        let f = &c[&3];
        assert_eq!(f.open("Mago", &mut |_| 1), vec![(206, 1), (207, 50)]);
        assert_eq!(f.open("Guerrero", &mut |_| 1), vec![(206, 1), (551, 100)]);
    }
}

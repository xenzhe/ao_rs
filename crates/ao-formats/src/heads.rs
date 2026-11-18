//! Which heads a character may wear, by race and gender: at creation and at the surgeon.
//!
//! `init/head_ranges.ini` (written by tools/tsao for Tierras Sagradas) overrides the AO20
//! ranges of `ValidarCabeza` (TCP.bas):
//! ```ini
//! [1-1]          ; gender-race (gender 1 = male, 2 = female; race ids of Balance.dat)
//! Ranges=1-30    ; comma-separated a-b ranges
//! ```
use crate::ini::Ini;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

pub const MALE: u8 = 1;
pub const FEMALE: u8 = 2;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HeadRanges {
    by: HashMap<(u8, u8), Vec<(u16, u16)>>,
}

impl HeadRanges {
    /// AO20 `ValidarCabeza`.
    pub fn ao20() -> HeadRanges {
        type Row = ((u8, u8), &'static [(u16, u16)]);
        let t: &[Row] = &[
            ((MALE, 1), &[(1, 41), (778, 791)]),
            ((MALE, 2), &[(101, 132), (531, 545)]),
            ((MALE, 3), &[(200, 229), (792, 810)]),
            ((MALE, 4), &[(400, 429)]),
            ((MALE, 5), &[(300, 344)]),
            ((MALE, 6), &[(500, 529)]),
            ((FEMALE, 1), &[(50, 80), (187, 190), (230, 246)]),
            ((FEMALE, 2), &[(150, 179), (758, 777)]),
            ((FEMALE, 3), &[(250, 279)]),
            ((FEMALE, 4), &[(450, 479)]),
            ((FEMALE, 5), &[(350, 379)]),
            ((FEMALE, 6), &[(550, 579)]),
        ];
        HeadRanges { by: t.iter().map(|(k, v)| (*k, v.to_vec())).collect() }
    }

    pub fn parse(ini: &Ini) -> Option<HeadRanges> {
        let mut by = HashMap::new();
        for (k, sec) in ini.sections() {
            let Some((g, r)) = k.split_once('-') else { continue };
            let (Ok(g), Ok(r)) = (g.trim().parse::<u8>(), r.trim().parse::<u8>()) else { continue };
            let ranges: Vec<(u16, u16)> = sec
                .get("ranges")
                .map(|v| v.split(',').filter_map(|p| p.split_once('-')).filter_map(|(a, b)| Some((a.trim().parse().ok()?, b.trim().parse().ok()?))).collect())
                .unwrap_or_default();
            by.insert((g, r), ranges);
        }
        (!by.is_empty()).then_some(HeadRanges { by })
    }

    /// `recursos/init/head_ranges.ini`, else the AO20 ranges.
    pub fn load(recursos: &Path) -> HeadRanges {
        Self::parse(&Ini::load(&recursos.join("init").join("head_ranges.ini"))).unwrap_or_else(Self::ao20)
    }

    pub fn ranges(&self, race: u8, gender: u8) -> &[(u16, u16)] {
        self.by.get(&(gender, race)).map_or(&[], |v| v.as_slice())
    }

    pub fn allows(&self, race: u8, gender: u8, head: u16) -> bool {
        self.ranges(race, gender).iter().any(|&(a, b)| (a..=b).contains(&head))
    }

    pub fn heads(&self, race: u8, gender: u8) -> impl Iterator<Item = u16> + '_ {
        self.ranges(race, gender).iter().flat_map(|&(a, b)| a..=b)
    }
}

/// Loaded once per `Recursos` folder.
pub fn for_recursos(recursos: &Path) -> &'static HeadRanges {
    static CACHE: OnceLock<Mutex<HashMap<PathBuf, &'static HeadRanges>>> = OnceLock::new();
    let mut m = CACHE.get_or_init(Default::default).lock().unwrap();
    m.entry(recursos.to_path_buf()).or_insert_with(|| Box::leak(Box::new(HeadRanges::load(recursos))))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_overrides_and_defaults() {
        let ini = Ini::parse(b"[1-1]\nRanges=1-30\n[2-1]\nRanges=70-76, 80-81\n");
        let h = HeadRanges::parse(&ini).unwrap();
        assert!(h.allows(1, MALE, 30) && !h.allows(1, MALE, 31));
        assert!(h.allows(1, FEMALE, 81) && !h.allows(1, FEMALE, 77));
        assert!(!h.allows(6, MALE, 500), "races the file leaves out have no heads");
        assert!(HeadRanges::ao20().allows(6, MALE, 500));
    }
}

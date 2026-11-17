//! Crafting data: what each trade can build, and the objects placed on a map (anvils).
//! OWNER: Opus B (docs/P4_CONTRATO.md).
//!
//! - Trade lists, mirroring the `FileIO.bas` loaders: `LoadArmasHerreria`
//!   (`ArmasHerrero.dat`, `[INIT] NumArmas`, `[ArmaN] Index`), `LoadArmadurasHerreria`
//!   (`ArmadurasHerrero.dat`, `NumArmaduras`, `[ArmaduraN]`), `LoadBlackSmithElementalRunes`
//!   (`BlackSmithElementalRunes.dat`, `NumRunas`, `[RunaN]`), and the carpenter, alchemist
//!   and tailor loaders (`ObjCarpintero.dat` / `ObjAlquimista.dat` / `ObjSastre.dat`,
//!   `NumObjs`, `[ObjN] Index`). Like VB they loop `1..=Num` and keep the order of the file.
//! - Map objects: the `.csm` objects block (`t_DatosObjs`: x, y, ObjIndex, ObjAmmount as
//!   16-bit integers, `CargarMapaFormatoCSM`), which `csm::AoMap` skips. Only needed to know
//!   where the anvils are (`HandleAnvilAction`).
use super::{Loaded, RawIni, Table, Warning};
use crate::csm::{map_path, on_map};
use std::path::Path;

/// The six official lists.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CraftLists {
    /// `ArmasHerrero` (weapons and arrows of the blacksmith).
    pub smith_weapons: Vec<u16>,
    /// `ArmadurasHerrero` (armor, helmets, shields, amulets, rings).
    pub smith_armors: Vec<u16>,
    /// `BlackSmithElementalRunes`.
    pub smith_runes: Vec<u16>,
    /// `ObjCarpintero`.
    pub carpenter: Vec<u16>,
    /// `ObjAlquimista`.
    pub alchemy: Vec<u16>,
    /// `ObjSastre`.
    pub tailor: Vec<u16>,
}

fn list(recursos: &Path, file: &'static str, count_key: &str, prefix: &str, warnings: &mut Vec<Warning>) -> Vec<u16> {
    let Ok(ini) = RawIni::load(recursos, file, warnings) else {
        warnings.push(Warning { file, section: String::new(), key: String::new(), value: String::new(), message: "file missing: empty list".into() });
        return Vec::new();
    };
    let n = ini.sec(file, "INIT", warnings).count(count_key, 10_000);
    let mut out = Vec::with_capacity(n);
    for i in 1..=n {
        let name = format!("{prefix}{i}");
        let v = ini.sec(file, &name, warnings).int("Index");
        if v > 0 && v <= u16::MAX as i32 {
            out.push(v as u16);
        } else {
            // VB keeps a 0 in the array: it matches nothing, so skipping it is equivalent.
            warnings.push(Warning { file, section: name, key: "Index".into(), value: v.to_string(), message: "no object index; skipped".into() });
        }
    }
    out
}

/// Reads the six lists from `Recursos/Dat`. A missing file is an empty list (and a warning).
pub fn load(recursos: &Path) -> Loaded<CraftLists> {
    let mut w = Vec::new();
    let data = CraftLists {
        smith_weapons: list(recursos, "ArmasHerrero.dat", "NumArmas", "Arma", &mut w),
        smith_armors: list(recursos, "ArmadurasHerrero.dat", "NumArmaduras", "Armadura", &mut w),
        smith_runes: list(recursos, "BlackSmithElementalRunes.dat", "NumRunas", "Runa", &mut w),
        carpenter: list(recursos, "ObjCarpintero.dat", "NumObjs", "Obj", &mut w),
        alchemy: list(recursos, "ObjAlquimista.dat", "NumObjs", "Obj", &mut w),
        tailor: list(recursos, "ObjSastre.dat", "NumObjs", "Obj", &mut w),
    };
    Loaded { data, warnings: w }
}

/// One object lying on a map in the `.csm` file.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MapObject {
    pub x: i32,
    pub y: i32,
    pub obj: u16,
    pub amount: u16,
}

struct R<'a> {
    d: &'a [u8],
    p: usize,
}

impl R<'_> {
    fn take(&mut self, n: usize) -> Option<&[u8]> {
        let s = self.d.get(self.p..self.p.checked_add(n)?)?;
        self.p += n;
        Some(s)
    }
    fn i16(&mut self) -> Option<i16> {
        Some(i16::from_le_bytes(self.take(2)?.try_into().ok()?))
    }
    fn i32(&mut self) -> Option<i32> {
        Some(i32::from_le_bytes(self.take(4)?.try_into().ok()?))
    }
    fn count(&mut self) -> Option<usize> {
        let n = self.i32()?;
        (0..=10_000).contains(&n).then_some(n as usize)
    }
    fn string(&mut self) -> Option<()> {
        let n = self.i16()? as u16 as usize;
        self.take(n).map(|_| ())
    }
}

/// Same signature as `csm::FIVE_LAYER_SIGNATURE` (the 5-layer header).
const FIVE_LAYERS: i32 = 0x324C_3557;

/// The objects block of a `.csm` file (empty when the file is malformed).
pub fn parse_map_objects(data: &[u8]) -> Vec<MapObject> {
    fn go(data: &[u8]) -> Option<Vec<MapObject>> {
        let mut r = R { d: data, p: 0 };
        let first = r.i32()?;
        let mut layers = [0usize; 5];
        let blocked = if first == FIVE_LAYERS {
            let b = r.count()?;
            for l in layers.iter_mut() {
                *l = r.count()?;
            }
            b
        } else {
            if !(0..=10_000).contains(&first) {
                return None;
            }
            for l in [0usize, 1, 3, 4] {
                layers[l] = r.count()?;
            }
            first as usize
        };
        let (triggers, lights, particles, _npcs, objs) = (r.count()?, r.count()?, r.count()?, r.count()?, r.count()?);
        let _exits = r.count()?;
        // tMapSize + tMapDat (same order as `AoMap::parse`).
        r.take(8)?;
        r.string()?;
        r.take(1)?;
        r.string()?;
        r.take(8)?;
        r.take(1)?;
        r.string()?;
        r.string()?;
        r.string()?;
        r.take(16)?;
        r.string()?;
        r.take(3)?;
        // blocked (x, y, u8), layers (x, y, i32), triggers (x, y, i16), particles (x, y,
        // i32), lights (x, y, rgba, range), then the objects.
        r.take(blocked * 5)?;
        for n in layers {
            r.take(n * 8)?;
        }
        r.take(triggers * 6)?;
        r.take(particles * 8)?;
        r.take(lights * 9)?;
        let mut out = Vec::with_capacity(objs);
        for _ in 0..objs {
            let (x, y, obj, amount) = (r.i16()? as i32, r.i16()? as i32, r.i16()?, r.i16()?);
            if on_map(x, y) && obj > 0 {
                out.push(MapObject { x, y, obj: obj as u16, amount: amount.max(0) as u16 });
            }
        }
        Some(out)
    }
    go(data).unwrap_or_default()
}

/// The objects of map `number` (empty if the file is missing or malformed).
pub fn map_objects(recursos: &Path, number: u16) -> Vec<MapObject> {
    std::fs::read(map_path(recursos, number)).map(|d| parse_map_objects(&d)).unwrap_or_default()
}

/// Convenience for callers holding the object table: keeps only objects of a given type.
pub fn objects_of_type(objs: &[MapObject], table: &Table<super::obj::ObjData>, t: super::obj::ObjType) -> Vec<MapObject> {
    objs.iter().copied().filter(|o| table.get(o.obj as u32).is_some_and(|d| d.obj_type == t)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::paths::recursos_dir;

    #[test]
    fn official_lists_load() {
        let root = recursos_dir();
        if !root.join("Dat").join("ObjCarpintero.dat").exists() {
            return;
        }
        let l = load(&root);
        let d = &l.data;
        assert_eq!(d.smith_weapons.len(), 33, "ArmasHerrero NumArmas=33");
        assert_eq!(d.smith_weapons[0], 3980);
        assert_eq!(d.carpenter.len(), 31);
        assert_eq!(d.carpenter[0], 163, "Cuchara de madera");
        assert_eq!(d.alchemy.len(), 7);
        assert_eq!(d.tailor[0], 3578);
        assert_eq!(d.smith_runes, vec![5170, 5171, 5172, 5173]);
        assert!(!d.smith_armors.is_empty());
    }

    #[test]
    fn ullathorpe_has_anvils() {
        let root = recursos_dir();
        let Ok(objs) = crate::dat::obj::load(&root) else { return };
        let on_map = map_objects(&root, 1);
        assert!(!on_map.is_empty(), "map 1 carries objects");
        let anvils = objects_of_type(&on_map, &objs.data, crate::dat::obj::ObjType::Anvil);
        assert!(anvils.iter().any(|a| (a.x, a.y, a.obj) == (34, 65, 384)), "the anvil of Ullathorpe: {anvils:?}");
    }

    #[test]
    fn garbage_is_empty() {
        assert!(parse_map_objects(&[1, 2, 3]).is_empty());
    }
}

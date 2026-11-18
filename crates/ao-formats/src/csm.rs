//! Argentum Online 2.0 map format (.csm), ported from the official VB6 loaders
//! (client Recursos.bas `CargarMapa`, server FileIO.bas). Data only: the walking rules
//! that use it live in `ao-rules`.
//!
//! The file is a packed little-endian dump of VB6 UDTs: a header with section counts,
//! map size, map info (VB6 strings = u16 length + bytes), then each section in order:
//! blocked, layers 1..5, triggers, particles, lights, objects, NPCs, tile exits.
use std::path::{Path, PathBuf};

pub const MAP_SIZE: i32 = 100;
pub const LAYERS: usize = 5;

pub const BLOCK_NORTH: u8 = 0x01;
pub const BLOCK_EAST: u8 = 0x02;
pub const BLOCK_SOUTH: u8 = 0x04;
pub const BLOCK_WEST: u8 = 0x08;
pub const BLOCK_ALL: u8 = 0x0F;
pub const FLAG_WATER: u8 = 0x20;

pub const TRIGGER_UNDER_ROOF: u16 = 1;
pub const TRIGGER_SAFE_ZONE: u16 = 4;
pub const TRIGGER_SWIM_UNDER_ROOF: u16 = 16;
pub const FIRST_ROOF_TRIGGER: u16 = 60;

const FIVE_LAYER_SIGNATURE: i32 = 0x324C_3557;

#[derive(Clone, Copy, Debug)]
pub struct Light {
    pub x: i32,
    pub y: i32,
    /// RGBA, stored by VB6 as B,G,R,A.
    pub rgba: [u8; 4],
    pub range: u8,
}

#[derive(Clone, Copy, Debug)]
pub struct TileExit {
    pub x: i32,
    pub y: i32,
    pub map: u16,
    pub dest_x: i32,
    pub dest_y: i32,
}

#[derive(Clone, Copy, Debug)]
pub struct MapNpc {
    pub x: i32,
    pub y: i32,
    pub npc: u16,
}

pub struct AoMap {
    pub number: u16,
    pub name: String,
    pub safe: bool,
    pub zone: String,
    pub terrain: String,
    pub rain: bool,
    pub snow: bool,
    pub fog: bool,
    /// Per tile (index `tile(x, y)`): blocked side bits + FLAG_WATER.
    pub blocked: Vec<u8>,
    /// Per layer, per tile GRH index (0 = empty).
    pub layers: [Vec<u32>; LAYERS],
    pub triggers: Vec<u16>,
    pub particles: Vec<(i32, i32, i32)>,
    pub lights: Vec<Light>,
    pub npcs: Vec<MapNpc>,
    pub exits: Vec<TileExit>,
    exit_at: Vec<i32>,
    /// Bytes left after the last section (ignored, like the VB6 loaders do).
    pub trailing: usize,
    /// Trigger values that mark the inside of a roofed building on this map. The public
    /// AO20 maps use per-building ids (20-59) that the old client constant (>= 60) misses;
    /// a trigger counts when most of its tiles are under a layer-5 roof.
    pub roof_triggers: Vec<u16>,
    /// Walkable border of its resource set (`init/profile.ini [Walk]`; None = AO20).
    pub walk_bounds: Option<(i32, i32, i32, i32)>,
}

/// Triggers whose tiles are mostly (≥ 60 %, at least 4) under a layer-5 roof.
fn roof_triggers_of(triggers: &[u16], roofs: &[u32]) -> Vec<u16> {
    let mut stats: std::collections::BTreeMap<u16, (usize, usize)> = std::collections::BTreeMap::new();
    for (t, r) in triggers.iter().zip(roofs) {
        if *t != 0 {
            let e = stats.entry(*t).or_default();
            e.0 += 1;
            if *r > 0 {
                e.1 += 1;
            }
        }
    }
    stats.into_iter().filter(|(_, (n, roofed))| *roofed >= 4 && *roofed * 10 >= *n * 6).map(|(t, _)| t).collect()
}

#[derive(Debug)]
pub enum LoadError {
    Io(std::io::Error),
    Truncated(&'static str),
    Invalid(&'static str),
    TrailingBytes(usize),
}

impl From<std::io::Error> for LoadError {
    fn from(e: std::io::Error) -> Self {
        LoadError::Io(e)
    }
}

struct Reader<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    fn take(&mut self, n: usize, what: &'static str) -> Result<&'a [u8], LoadError> {
        let end = self.pos.checked_add(n).ok_or(LoadError::Truncated(what))?;
        let s = self.data.get(self.pos..end).ok_or(LoadError::Truncated(what))?;
        self.pos = end;
        Ok(s)
    }
    fn u8(&mut self, w: &'static str) -> Result<u8, LoadError> {
        Ok(self.take(1, w)?[0])
    }
    fn i16(&mut self, w: &'static str) -> Result<i16, LoadError> {
        Ok(i16::from_le_bytes(self.take(2, w)?.try_into().unwrap()))
    }
    fn i32(&mut self, w: &'static str) -> Result<i32, LoadError> {
        Ok(i32::from_le_bytes(self.take(4, w)?.try_into().unwrap()))
    }
    /// VB6 variable-length string inside a UDT: u16 length + Windows-1252 bytes.
    fn string(&mut self, w: &'static str) -> Result<String, LoadError> {
        let len = self.i16(w)? as u16 as usize;
        Ok(self.take(len, w)?.iter().map(|&b| cp1252(b)).collect())
    }
    fn count(&mut self, w: &'static str) -> Result<usize, LoadError> {
        let n = self.i32(w)?;
        if !(0..=10_000).contains(&n) {
            return Err(LoadError::Invalid(w));
        }
        Ok(n as usize)
    }
}

/// Windows-1252 to char for the Spanish characters used in map names.
fn cp1252(b: u8) -> char {
    match b {
        0x80 => '€',
        0x91 | 0x92 => '\'',
        0x93 | 0x94 => '"',
        0x96 | 0x97 => '-',
        _ => b as char, // 0xA0..=0xFF matches Latin-1
    }
}

pub fn tile(x: i32, y: i32) -> usize {
    ((y - 1) * MAP_SIZE + (x - 1)) as usize
}

pub fn on_map(x: i32, y: i32) -> bool {
    (1..=MAP_SIZE).contains(&x) && (1..=MAP_SIZE).contains(&y)
}

/// Layer-1 GRH ranges the official server treats as water (General.bas `HayAgua`).
pub fn is_water_grh(g: u32) -> bool {
    const RANGES: [(u32, u32); 9] = [
        (1505, 1520),
        (124, 139),
        (24223, 24238),
        (24303, 24318),
        (468, 483),
        (44668, 44683),
        (24143, 24158),
        (12628, 12643),
        (2948, 2963),
    ];
    RANGES.iter().any(|(a, b)| (*a..=*b).contains(&g))
}

pub fn is_roof_trigger(t: u16) -> bool {
    t >= FIRST_ROOF_TRIGGER || t == TRIGGER_UNDER_ROOF || t == TRIGGER_SAFE_ZONE || t == TRIGGER_SWIM_UNDER_ROOF
}

pub fn map_path(root: &Path, number: u16) -> PathBuf {
    let lower = root.join("Mapas").join(format!("mapa{number}.csm"));
    if lower.exists() {
        lower
    } else {
        root.join("Mapas").join(format!("Mapa{number}.csm"))
    }
}

impl AoMap {
    pub fn load(root: &Path, number: u16) -> Result<AoMap, LoadError> {
        let data = std::fs::read(map_path(root, number))?;
        let mut m = Self::parse(number, &data)?;
        // Which floors are water depends on the resource set (init/profile.ini).
        let p = crate::profile::for_recursos(root);
        m.walk_bounds = p.walk_bounds;
        for i in 0..m.blocked.len() {
            m.blocked[i] &= !FLAG_WATER;
            if p.is_water(m.layers[0][i], m.layers[1][i]) {
                m.blocked[i] |= FLAG_WATER;
            }
        }
        Ok(m)
    }

    pub fn parse(number: u16, data: &[u8]) -> Result<AoMap, LoadError> {
        let mut r = Reader { data, pos: 0 };
        let first = r.i32("header")?;
        let mut layer_counts = [0usize; LAYERS];
        let blocked_count;
        if first == FIVE_LAYER_SIGNATURE {
            blocked_count = r.count("blocked count")?;
            for c in layer_counts.iter_mut() {
                *c = r.count("layer count")?;
            }
        } else {
            // Legacy 4-layer file: file layers 1,2,3,4 map to 1,2,4,5 (empty layer 3).
            if !(0..=10_000).contains(&first) {
                return Err(LoadError::Invalid("blocked count"));
            }
            blocked_count = first as usize;
            for l in [0usize, 1, 3, 4] {
                layer_counts[l] = r.count("layer count")?;
            }
        }
        let triggers_n = r.count("triggers")?;
        let lights_n = r.count("lights")?;
        let particles_n = r.count("particles")?;
        let npcs_n = r.count("npcs")?;
        let objs_n = r.count("objects")?;
        let exits_n = r.count("exits")?;

        // tMapSize (unused) + tMapDat.
        r.take(8, "map size")?;
        let name = r.string("name")?;
        let _backup = r.u8("backup")?;
        let _restrict = r.string("restrict")?;
        let _music_hi = r.i32("music")?;
        let _music_lo = r.i32("music")?;
        let safe = r.u8("safe")? != 0;
        let zone = r.string("zone")?;
        let terrain = r.string("terrain")?;
        let _ambient = r.string("ambient")?;
        r.take(16, "light/letter/extra")?;
        let _extra3 = r.string("extra3")?;
        let rain = r.u8("rain")? != 0;
        let snow = r.u8("snow")? != 0;
        let fog = r.u8("fog")? != 0;

        let n = (MAP_SIZE * MAP_SIZE) as usize;
        let mut map = AoMap {
            number,
            name,
            safe,
            zone,
            terrain,
            rain,
            snow,
            fog,
            blocked: vec![0; n],
            layers: Default::default(),
            triggers: vec![0; n],
            particles: Vec::new(),
            lights: Vec::new(),
            npcs: Vec::new(),
            exits: Vec::new(),
            exit_at: vec![-1; n],
            trailing: 0,
            roof_triggers: Vec::new(),
            walk_bounds: None,
        };
        for l in map.layers.iter_mut() {
            *l = vec![0; n];
        }

        let pos = |r: &mut Reader, w: &'static str| -> Result<(i32, i32), LoadError> {
            let (x, y) = (r.i16(w)? as i32, r.i16(w)? as i32);
            if on_map(x, y) { Ok((x, y)) } else { Err(LoadError::Invalid(w)) }
        };

        for _ in 0..blocked_count {
            let (x, y) = pos(&mut r, "blocked")?;
            map.blocked[tile(x, y)] = r.u8("blocked")?;
        }
        for (l, &count) in layer_counts.iter().enumerate() {
            for _ in 0..count {
                let (x, y) = pos(&mut r, "layer")?;
                map.layers[l][tile(x, y)] = r.i32("layer")?.max(0) as u32;
            }
        }
        for _ in 0..triggers_n {
            let (x, y) = pos(&mut r, "trigger")?;
            map.triggers[tile(x, y)] = r.i16("trigger")? as u16;
        }
        for _ in 0..particles_n {
            let (x, y) = pos(&mut r, "particle")?;
            map.particles.push((x, y, r.i32("particle")?));
        }
        for _ in 0..lights_n {
            let (x, y) = pos(&mut r, "light")?;
            let c = r.take(4, "light")?;
            let range = r.u8("light")?;
            map.lights.push(Light { x, y, rgba: [c[2], c[1], c[0], c[3]], range });
        }
        for _ in 0..objs_n {
            pos(&mut r, "object")?;
            r.take(4, "object")?;
        }
        for _ in 0..npcs_n {
            let (x, y) = pos(&mut r, "npc")?;
            map.npcs.push(MapNpc { x, y, npc: r.i16("npc")? as u16 });
        }
        for _ in 0..exits_n {
            let (x, y) = pos(&mut r, "exit")?;
            let (m, dx, dy) = (r.i16("exit")?, r.i16("exit")? as i32, r.i16("exit")? as i32);
            if m > 0 {
                map.exit_at[tile(x, y)] = map.exits.len() as i32;
                map.exits.push(TileExit { x, y, map: m as u16, dest_x: dx, dest_y: dy });
            }
        }
        // The official loaders close the file after the exits and never check for EOF, so
        // extra bytes at the end are tolerated (several maps carry them).
        map.trailing = data.len() - r.pos;

        for i in 0..n {
            if is_water_grh(map.layers[0][i]) {
                map.blocked[i] |= FLAG_WATER;
            }
        }
        map.roof_triggers = roof_triggers_of(&map.triggers, &map.layers[4]);
        Ok(map)
    }

    /// Is `t` a roof trigger on this map (the official ones, or one whose area is mostly
    /// under layer-5 roofs)?
    pub fn is_roof(&self, t: u16) -> bool {
        t != 0 && (is_roof_trigger(t) || self.roof_triggers.contains(&t))
    }

    pub fn layer(&self, l: usize, x: i32, y: i32) -> u32 {
        if on_map(x, y) { self.layers[l][tile(x, y)] } else { 0 }
    }

    pub fn trigger(&self, x: i32, y: i32) -> u16 {
        if on_map(x, y) { self.triggers[tile(x, y)] } else { 0 }
    }

    pub fn blocked(&self, x: i32, y: i32) -> u8 {
        if on_map(x, y) { self.blocked[tile(x, y)] } else { BLOCK_ALL }
    }

    pub fn exit(&self, x: i32, y: i32) -> Option<TileExit> {
        if !on_map(x, y) {
            return None;
        }
        let i = self.exit_at[tile(x, y)];
        (i >= 0).then(|| self.exits[i as usize])
    }

    /// Roof id that covers (x, y): the tile's own roof trigger or a neighbour's (NearRoof).
    pub fn near_roof(&self, x: i32, y: i32) -> u16 {
        for ly in y - 1..=y + 1 {
            for lx in x - 1..=x + 1 {
                let t = self.trigger(lx, ly);
                if self.is_roof(t) {
                    return t;
                }
            }
        }
        0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::paths::recursos_dir;

    #[test]
    fn every_official_map_parses_exactly() {
        let root = recursos_dir();
        let dir = root.join("Mapas");
        let Ok(entries) = std::fs::read_dir(&dir) else {
            eprintln!("AO resources not found at {dir:?}, skipping");
            return;
        };
        let (mut ok, mut failed) = (0, Vec::new());
        for e in entries.flatten() {
            let name = e.file_name().to_string_lossy().to_lowercase();
            let Some(num) = name.strip_prefix("mapa").and_then(|s| s.strip_suffix(".csm")).and_then(|s| s.parse::<u16>().ok()) else { continue };
            let data = std::fs::read(e.path()).unwrap();
            if data.is_empty() {
                continue; // the official repo ships a few empty placeholders (mapa844.csm)
            }
            match AoMap::parse(num, &data) {
                Ok(m) => {
                    // Sanity of the decoded data: exits point at real maps and tiles.
                    let bad_exit = m.exits.iter().find(|e| e.map > 9999 || !on_map(e.dest_x, e.dest_y));
                    if let Some(e) = bad_exit {
                        failed.push(format!("{name}: exit {e:?}"));
                    } else {
                        ok += 1;
                    }
                }
                Err(err) => failed.push(format!("{name}: {err:?}")),
            }
        }
        println!("{ok} maps ok, {} failed", failed.len());
        assert!(failed.is_empty(), "{failed:#?}");
        assert!(ok > 800);
    }

    #[test]
    fn ullathorpe_header() {
        let root = recursos_dir();
        let Ok(m) = AoMap::load(&root, 1) else { return };
        assert_eq!(m.name, "Ciudad de Ullathorpe");
        assert!(m.safe);
        assert!(!m.exits.is_empty());
        assert!(m.layers[0].iter().all(|g| *g > 0), "layer 1 covers every tile");
    }
}

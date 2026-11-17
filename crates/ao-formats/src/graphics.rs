//! AO20 graphics index: GRH table (graficos.ini), bodies built from templates
//! (moldes.ini + cuerpos.dat), heads (cabezas.ini) and the tree list (TreeGrh.dat).
//! Pure data, mirroring the official client's Recursos.bas loaders; rendering lives in
//! the client.
use crate::ini::{num, Ini};
use std::collections::{HashMap, HashSet};
use std::path::Path;

pub const TILE_PX: f32 = 32.0;

#[derive(Clone, Debug)]
pub struct Grh {
    /// Frame GRH indices (a single-frame GRH lists itself).
    pub frames: Vec<u32>,
    pub file: u32,
    pub sx: f32,
    pub sy: f32,
    pub w: f32,
    pub h: f32,
    /// Total animation time in ms (0 = static).
    pub speed: f32,
}

#[derive(Clone, Default, Debug)]
pub struct Body {
    /// Walk animation GRH per AO heading (index 1..=4: N, E, S, W).
    pub walk: [u32; 5],
    pub head_offset: (f32, f32),
    pub body_offset: (f32, f32),
}

#[derive(Default)]
pub struct GraphicsDb {
    grh: Vec<Option<Grh>>,
    pub bodies: HashMap<u16, Body>,
    /// Head GRH per AO heading (index 1..=4).
    pub heads: HashMap<u16, [u32; 5]>,
    pub trees: HashSet<u32>,
}

impl GraphicsDb {
    pub fn load(recursos: &Path) -> GraphicsDb {
        let init = recursos.join("init");
        let mut db = GraphicsDb::default();
        db.load_grh(&init.join("graficos.ini"));
        db.load_bodies(&init);
        db.load_heads(&init.join("cabezas.ini"));
        if let Some(t) = Ini::load(&init.join("TreeGrh.dat")).section("trees") {
            db.trees = t.values().filter_map(|v| v.parse().ok()).collect();
        }
        db
    }

    /// `GrhN=1-file-sx-sy-w-h` (static) or `GrhN=frames-f1-...-fn-speed` (animated).
    fn load_grh(&mut self, path: &Path) {
        let text: String = std::fs::read(path).map(|b| b.iter().map(|&c| c as char).collect()).unwrap_or_default();
        let mut animated = Vec::new();
        for line in text.lines() {
            let Some(rest) = line.strip_prefix("Grh") else { continue };
            let Some((idx, vals)) = rest.split_once('=') else { continue };
            let Ok(idx) = idx.trim().parse::<usize>() else { continue };
            let v: Vec<f32> = vals.trim().split('-').filter_map(|p| p.trim().replace(',', ".").parse().ok()).collect();
            if v.is_empty() {
                continue;
            }
            if self.grh.len() <= idx {
                self.grh.resize(idx + 1, None);
            }
            let n = v[0] as usize;
            if n == 1 && v.len() >= 6 {
                self.grh[idx] = Some(Grh { frames: vec![idx as u32], file: v[1] as u32, sx: v[2], sy: v[3], w: v[4], h: v[5], speed: 0.0 });
            } else if n > 1 && v.len() >= n + 2 {
                animated.push((idx, v[1..=n].iter().map(|f| *f as u32).collect::<Vec<_>>(), v[n + 1]));
            }
        }
        // Animated entries take their size/texture from the first frame.
        for (idx, frames, speed) in animated {
            if let Some(Some(f0)) = frames.first().and_then(|f| self.grh.get(*f as usize)).cloned() {
                self.grh[idx] = Some(Grh { frames, speed, ..f0 });
            }
        }
    }

    fn push_grh(&mut self, mut g: Grh) -> u32 {
        let id = self.grh.len() as u32;
        if g.frames.is_empty() {
            g.frames = vec![id];
        }
        self.grh.push(Some(g));
        id
    }

    /// Bodies with `Std=N` are generated from template N of moldes.ini: one row of frames
    /// per direction (S, N, W, E) starting at (X, Y) on texture `FileNum`.
    fn load_bodies(&mut self, init: &Path) {
        let moldes = Ini::load(&init.join("moldes.ini"));
        let cuerpos = Ini::load(&init.join("cuerpos.dat"));
        const DIR_TO_HEADING: [usize; 4] = [3, 1, 4, 2];
        let count = num(cuerpos.section("init"), "numbodies") as u16;
        for i in 1..=count {
            let Some(b) = cuerpos.section(&format!("body{i}")) else { continue };
            let b = Some(b);
            let mut body = Body {
                head_offset: (num(b, "headoffsetx"), num(b, "headoffsety")),
                body_offset: (num(b, "bodyoffsetx"), num(b, "bodyoffsety")),
                ..Default::default()
            };
            let std = num(b, "std") as u32;
            if std == 0 {
                for (h, slot) in body.walk.iter_mut().enumerate().skip(1) {
                    *slot = num(b, &format!("walk{h}")) as u32;
                }
            } else if let Some(m) = moldes.section(&format!("molde{std}")) {
                let m = Some(m);
                let file = num(b, "filenum") as u32;
                let speed = match num(b, "speed") {
                    s if s > 0.0 => s,
                    _ => 1.0,
                };
                let frame_ms = 1.0 / speed / 0.018;
                let (x0, mut y) = (num(m, "x"), num(m, "y"));
                let (w, h) = (num(m, "width"), num(m, "height"));
                for (j, heading) in DIR_TO_HEADING.iter().enumerate() {
                    let n = num(m, &format!("dir{}", j + 1)) as usize;
                    let frames: Vec<u32> = (0..n).map(|k| self.push_grh(Grh { frames: vec![], file, sx: x0 + k as f32 * w, sy: y, w, h, speed: 0.0 })).collect();
                    if !frames.is_empty() {
                        body.walk[*heading] = self.push_grh(Grh { frames, file, sx: x0, sy: y, w, h, speed: n as f32 * frame_ms });
                    }
                    y += h;
                }
            }
            self.bodies.insert(i, body);
        }
    }

    fn load_heads(&mut self, path: &Path) {
        let ini = Ini::load(path);
        for (k, v) in ini.sections() {
            let Some(n) = k.strip_prefix("head").and_then(|s| s.parse::<u16>().ok()) else { continue };
            let mut h = [0u32; 5];
            for (i, slot) in h.iter_mut().enumerate().skip(1) {
                *slot = num(Some(v), &format!("head{i}")) as u32;
            }
            self.heads.insert(n, h);
        }
    }

    pub fn grh(&self, idx: u32) -> Option<&Grh> {
        self.grh.get(idx as usize).and_then(|g| g.as_ref())
    }

    /// Frame to show at time `t_ms` (animated GRHs loop forever, like map layers in AO).
    pub fn frame(&self, idx: u32, t_ms: f32) -> Option<&Grh> {
        let g = self.grh(idx)?;
        if g.frames.len() <= 1 || g.speed <= 0.0 {
            return self.grh(*g.frames.first().unwrap_or(&idx)).or(Some(g));
        }
        let per = g.speed / g.frames.len() as f32;
        let f = ((t_ms / per) as usize) % g.frames.len();
        self.grh(g.frames[f])
    }

    /// Every texture file a GRH needs, including all animation frames.
    pub fn files_of(&self, idx: u32, out: &mut HashSet<u32>) {
        if let Some(g) = self.grh(idx) {
            out.insert(g.file);
            for f in &g.frames {
                if let Some(ff) = self.grh(*f) {
                    out.insert(ff.file);
                }
            }
        }
    }

    pub fn grh_count(&self) -> usize {
        self.grh.iter().filter(|g| g.is_some()).count()
    }
}

/// Top-left where a GRH frame is drawn for tile position (x, y). With `center`, sprites
/// wider/taller than a tile are anchored to the bottom-center of the tile, exactly like
/// `Draw_Grh` in the official client.
pub fn anchor(g: &Grh, x: f32, y: f32, center: bool) -> (f32, f32) {
    let (mut x, mut y) = (x, y);
    if center {
        if g.w != TILE_PX {
            x = x - (g.w / 2.0).floor() + TILE_PX / 2.0;
        }
        if g.h != TILE_PX {
            y = y - g.h + TILE_PX;
        }
    }
    (x, y)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::paths::recursos_dir;

    #[test]
    fn official_tables_load() {
        let root = recursos_dir();
        if !root.join("init").is_dir() {
            eprintln!("AO resources not found, skipping");
            return;
        }
        let db = GraphicsDb::load(&root);
        assert!(db.grh_count() > 50_000);
        // Body 1 comes from a template: 4 directions with animated walk cycles.
        let body = &db.bodies[&1];
        for h in 1..=4 {
            let g = db.grh(body.walk[h]).expect("walk grh");
            assert!(g.frames.len() >= 5 && g.speed > 0.0);
        }
        // Heads are 27x64 sprites.
        let head = db.grh(db.heads[&1][3]).unwrap();
        assert_eq!((head.w, head.h), (27.0, 64.0));
        assert!(!db.trees.is_empty());
    }

    #[test]
    fn anchor_matches_draw_grh() {
        let g = Grh { frames: vec![1], file: 1, sx: 0.0, sy: 0.0, w: 27.0, h: 64.0, speed: 0.0 };
        assert_eq!(anchor(&g, 100.0, 100.0, true), (103.0, 68.0));
        assert_eq!(anchor(&g, 100.0, 100.0, false), (100.0, 100.0));
    }
}

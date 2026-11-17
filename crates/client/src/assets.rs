//! GPU side of the AO graphics: lazy texture cache over `Recursos/Graficos` and GRH
//! drawing. The data itself (GRH table, bodies, heads) comes from `ao_formats::graphics`.
use ao_formats::csm::AoMap;
use ao_formats::graphics::{anchor, GraphicsDb};
use macroquad::prelude::*;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

pub struct Assets {
    pub root: PathBuf,
    pub db: GraphicsDb,
    textures: HashMap<u32, Option<Texture2D>>,
}

impl Assets {
    pub fn load(root: PathBuf) -> Assets {
        let db = GraphicsDb::load(&root);
        Assets { root, db, textures: HashMap::new() }
    }

    pub fn texture(&mut self, file: u32) -> Option<Texture2D> {
        if let Some(t) = self.textures.get(&file) {
            return t.clone();
        }
        let dir = self.root.join("Graficos");
        let tex = ["png", "PNG"].iter().find_map(|ext| std::fs::read(dir.join(format!("{file}.{ext}"))).ok()).and_then(|bytes| {
            let img = Image::from_file_with_format(&bytes, None).ok()?;
            let t = Texture2D::from_image(&img);
            t.set_filter(FilterMode::Nearest);
            Some(t)
        });
        self.textures.insert(file, tex.clone());
        tex
    }

    /// Loads every texture a map uses, so walking around never stalls on disk reads.
    pub fn preload_map(&mut self, map: &AoMap) -> usize {
        let mut files = HashSet::new();
        let grhs: HashSet<u32> = map.layers.iter().flat_map(|l| l.iter().copied().filter(|g| *g > 0)).collect();
        for g in grhs {
            self.db.files_of(g, &mut files);
        }
        let n = files.len();
        for f in files {
            self.texture(f);
        }
        n
    }

    /// Draws GRH `idx` at (x, y); with `center` it is anchored like the official `Draw_Grh`.
    pub fn draw(&mut self, idx: u32, x: f32, y: f32, center: bool, t_ms: f32, tint: Color) {
        let Some(g) = self.db.frame(idx, t_ms).cloned() else { return };
        let (x, y) = anchor(&g, x, y, center);
        let Some(tex) = self.texture(g.file) else { return };
        draw_texture_ex(&tex, x.round(), y.round(), tint, DrawTextureParams { source: Some(Rect::new(g.sx, g.sy, g.w, g.h)), ..Default::default() });
    }
}

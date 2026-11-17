//! Headless AO20 screenshots: a CPU compositor (no window, no GPU) that draws a map with
//! its layers and characters to an `RgbaImage`, plus golden-image comparison helpers.
//!
//! Layer order and sprite anchoring replicate `client/src/render/world.rs` (which follows
//! the official `TileEngine_RenderScreen.bas`): floor, layers 2-3, then row by row
//! characters and layer 4 (trees with shadow), and finally layer-5 roofs (opaque here).
//! No lights, particles, weather or fades.
mod diff;
mod font;

pub use diff::{assert_similar, compare, compare_with, Diff};

use ao_formats::csm::AoMap;
use ao_formats::graphics::{anchor, GraphicsDb, TILE_PX};
use image::{Rgba, RgbaImage};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

const TILE: i32 = TILE_PX as i32;

/// A character to draw. `heading`: 1 = north, 2 = east, 3 = south, 4 = west.
#[derive(Clone, Debug)]
pub struct SnapEntity {
    pub x: i32,
    pub y: i32,
    pub body: u16,
    pub head: u16,
    pub heading: u8,
    pub name: Option<String>,
}

impl SnapEntity {
    /// Parses `body,head,heading@x,y` (CLI syntax).
    pub fn parse(s: &str) -> Result<SnapEntity, String> {
        let bad = || format!("invalid entity '{s}', expected body,head,heading@x,y");
        let (look, pos) = s.split_once('@').ok_or_else(bad)?;
        let l: Vec<&str> = look.split(',').collect();
        let p: Vec<&str> = pos.split(',').collect();
        if l.len() != 3 || p.len() != 2 {
            return Err(bad());
        }
        let n = |v: &str| v.trim().parse::<i32>().map_err(|_| bad());
        let heading = n(l[2])?;
        if !(1..=4).contains(&heading) {
            return Err(bad());
        }
        Ok(SnapEntity { body: n(l[0])? as u16, head: n(l[1])? as u16, heading: heading as u8, x: n(p[0])?, y: n(p[1])?, name: None })
    }
}

/// Where the official resources live: `$AO_RECURSOS`, `ao_formats::recursos_dir()`, or an
/// `ao20/Recursos` folder next to any ancestor of this crate (works from git worktrees).
pub fn find_recursos() -> Option<PathBuf> {
    let ok = |p: &Path| p.join("Mapas").is_dir() && p.join("init").is_dir();
    if let Ok(p) = std::env::var("AO_RECURSOS") {
        let p = PathBuf::from(p);
        return ok(&p).then_some(p);
    }
    let p = ao_formats::recursos_dir();
    if ok(&p) {
        return Some(p);
    }
    Path::new(env!("CARGO_MANIFEST_DIR")).ancestors().map(|a| a.join("ao20").join("Recursos")).find(|p| ok(p))
}

/// CPU renderer with a decoded-texture cache; keep one around to make repeated captures fast.
pub struct Snapshot {
    pub root: PathBuf,
    pub db: GraphicsDb,
    textures: HashMap<u32, Option<Arc<RgbaImage>>>,
    /// Integer upscale applied to the output (nearest neighbour).
    pub scale: u32,
}

impl Snapshot {
    pub fn new(recursos: impl Into<PathBuf>) -> Snapshot {
        let root = recursos.into();
        let db = GraphicsDb::load(&root);
        Snapshot { root, db, textures: HashMap::new(), scale: 1 }
    }

    pub fn with_scale(mut self, scale: u32) -> Snapshot {
        self.scale = scale.max(1);
        self
    }

    fn texture(&mut self, file: u32) -> Option<Arc<RgbaImage>> {
        if let Some(t) = self.textures.get(&file) {
            return t.clone();
        }
        let dir = self.root.join("Graficos");
        let tex = ["png", "PNG"]
            .iter()
            .find_map(|ext| image::open(dir.join(format!("{file}.{ext}"))).ok())
            .map(|i| Arc::new(i.to_rgba8()));
        self.textures.insert(file, tex.clone());
        tex
    }

    /// Draws GRH `idx` with its top-left tile corner at (x, y) canvas pixels, anchored like `Draw_Grh`.
    #[allow(clippy::too_many_arguments)]
    fn draw(&mut self, canvas: &mut RgbaImage, idx: u32, x: i32, y: i32, center: bool, t_ms: f32, tint: [u8; 4]) {
        let Some(g) = self.db.frame(idx, t_ms).cloned() else { return };
        let (ax, ay) = anchor(&g, x as f32, y as f32, center);
        let Some(tex) = self.texture(g.file) else { return };
        blit(canvas, &tex, (g.sx as i32, g.sy as i32, g.w as i32, g.h as i32), ax.round() as i32, ay.round() as i32, tint);
    }

    /// Renders `size_tiles` (width, height) tiles centred on tile `center` (1-based).
    pub fn render(&mut self, map: &AoMap, center: (i32, i32), size_tiles: (i32, i32), entities: &[SnapEntity], t_ms: f32) -> RgbaImage {
        let (tw, th) = size_tiles;
        let (w, h) = (tw * TILE, th * TILE);
        let mut canvas = RgbaImage::from_pixel(w as u32, h as u32, Rgba([0, 0, 0, 255]));
        // World pixel of the canvas' top-left corner.
        let ox = (center.0 - 1) * TILE + TILE / 2 - w / 2;
        let oy = (center.1 - 1) * TILE + TILE / 2 - h / 2;
        let (tx0, ty0) = (ox.div_euclid(TILE) + 1, oy.div_euclid(TILE) + 1);
        let (tx1, ty1) = (tx0 + tw, ty0 + th);
        // Big sprites are anchored to their bottom tile: draw a margin around the view.
        let (bx0, bx1, by0, by1) = (tx0 - 6, tx1 + 6, ty0 - 1, ty1 + 10);
        let px = |x: i32| (x - 1) * TILE - ox;
        let py = |y: i32| (y - 1) * TILE - oy;
        const WHITE: [u8; 4] = [255; 4];

        // 1. Floor (not centred).
        for y in ty0 - 1..=ty1 {
            for x in tx0 - 1..=tx1 {
                let g = map.layer(0, x, y);
                if g > 0 {
                    self.draw(&mut canvas, g, px(x), py(y), false, t_ms, WHITE);
                }
            }
        }
        // 2. Walkable overlays (layers 2 and 3), each layer fully before the next.
        for l in [1usize, 2] {
            for y in by0..=by1 {
                for x in bx0..=bx1 {
                    let g = map.layer(l, x, y);
                    if g > 0 {
                        self.draw(&mut canvas, g, px(x), py(y), true, t_ms, WHITE);
                    }
                }
            }
        }
        // 3. Row by row: characters, then layer 4 (trees get a drop shadow).
        let mut by_row: HashMap<i32, Vec<&SnapEntity>> = HashMap::new();
        for e in entities {
            by_row.entry(e.y).or_default().push(e);
        }
        for y in by0..=by1 {
            if let Some(row) = by_row.get_mut(&y) {
                row.sort_by_key(|e| e.x);
                for e in row.iter() {
                    self.draw_character(&mut canvas, e, px(e.x), py(e.y));
                }
            }
            for x in bx0..=bx1 {
                let g = map.layer(3, x, y);
                if g == 0 {
                    continue;
                }
                if self.db.trees.contains(&g) {
                    self.draw(&mut canvas, g, px(x) + 10, py(y) + 4, true, t_ms, [0, 0, 0, (0.22f32 * 255.0) as u8]);
                }
                self.draw(&mut canvas, g, px(x), py(y), true, t_ms, WHITE);
            }
        }
        // 4. Roofs, always opaque.
        for y in by0..=by1 {
            for x in bx0..=bx1 {
                let g = map.layer(4, x, y);
                if g > 0 {
                    self.draw(&mut canvas, g, px(x), py(y), true, t_ms, WHITE);
                }
            }
        }
        // 5. Names on top of everything.
        for e in entities {
            if let Some(name) = &e.name {
                draw_name(&mut canvas, name, px(e.x) + TILE / 2, py(e.y) + TILE + 2);
            }
        }
        if self.scale > 1 {
            let s = self.scale;
            return image::imageops::resize(&canvas, w as u32 * s, h as u32 * s, image::imageops::FilterType::Nearest);
        }
        canvas
    }

    /// Body + head standing still at the tile whose top-left is (tlx, tly), as the client's `draw_character`.
    fn draw_character(&mut self, canvas: &mut RgbaImage, e: &SnapEntity, tlx: i32, tly: i32) {
        let heading = (e.heading as usize).clamp(1, 4);
        let Some(body) = self.db.bodies.get(&e.body).cloned() else { return };
        fill_ellipse(canvas, tlx + 16, tly + 29, 11, 4, [0, 0, 0, (0.3f32 * 255.0) as u8]);
        let bx = tlx as f32 + body.body_offset.0;
        let by = tly as f32 + body.body_offset.1;
        self.draw(canvas, body.walk[heading], bx.round() as i32, by.round() as i32, true, 0.0, [255; 4]);
        if let Some(head) = self.db.heads.get(&e.head).map(|h| h[heading]) {
            if head > 0 {
                let hx = bx + body.head_offset.0;
                let hy = by + body.head_offset.1 - 1.0;
                self.draw(canvas, head, hx.round() as i32, hy.round() as i32, true, 0.0, [255; 4]);
            }
        }
    }
}

/// Alpha-blends `src_rect` (x, y, w, h) of `tex` onto `canvas` at (dx, dy), multiplied by `tint`.
fn blit(canvas: &mut RgbaImage, tex: &RgbaImage, src_rect: (i32, i32, i32, i32), dx: i32, dy: i32, tint: [u8; 4]) {
    let (sx, sy, sw, sh) = src_rect;
    let (cw, ch) = (canvas.width() as i32, canvas.height() as i32);
    let (tw, th) = (tex.width() as i32, tex.height() as i32);
    let x_from = 0.max(-dx).max(-sx);
    let x_to = sw.min(cw - dx).min(tw - sx);
    let y_from = 0.max(-dy).max(-sy);
    let y_to = sh.min(ch - dy).min(th - sy);
    if x_from >= x_to || y_from >= y_to {
        return;
    }
    let plain = tint == [255; 4];
    let src = tex.as_raw();
    let dst_stride = cw as usize * 4;
    let dst = canvas.as_mut();
    for y in y_from..y_to {
        let mut si = (((sy + y) * tw + sx + x_from) * 4) as usize;
        let mut di = ((dy + y) as usize) * dst_stride + ((dx + x_from) as usize) * 4;
        for _ in x_from..x_to {
            let sa = src[si + 3] as u32;
            if sa != 0 {
                if plain && sa == 255 {
                    dst[di..di + 3].copy_from_slice(&src[si..si + 3]);
                } else {
                    let a = if plain { sa } else { sa * tint[3] as u32 / 255 };
                    if a != 0 {
                        for c in 0..3 {
                            let s = if plain { src[si + c] as u32 } else { src[si + c] as u32 * tint[c] as u32 / 255 };
                            let d = dst[di + c] as u32;
                            dst[di + c] = ((s * a + d * (255 - a) + 127) / 255) as u8;
                        }
                    }
                }
            }
            si += 4;
            di += 4;
        }
    }
}

fn blend_px(canvas: &mut RgbaImage, x: i32, y: i32, c: [u8; 4]) {
    if x < 0 || y < 0 || x >= canvas.width() as i32 || y >= canvas.height() as i32 {
        return;
    }
    let p = canvas.get_pixel_mut(x as u32, y as u32);
    let a = c[3] as u32;
    for i in 0..3 {
        p[i] = ((c[i] as u32 * a + p[i] as u32 * (255 - a) + 127) / 255) as u8;
    }
}

fn fill_ellipse(canvas: &mut RgbaImage, cx: i32, cy: i32, rx: i32, ry: i32, c: [u8; 4]) {
    for y in -ry..=ry {
        for x in -rx..=rx {
            let (fx, fy) = (x as f32 / rx as f32, y as f32 / ry as f32);
            if fx * fx + fy * fy <= 1.0 {
                blend_px(canvas, cx + x, cy + y, c);
            }
        }
    }
}

/// White name with a black outline, centred on `cx`, top at `y`.
fn draw_name(canvas: &mut RgbaImage, name: &str, cx: i32, y: i32) {
    const S: i32 = 2;
    let n = name.chars().count() as i32;
    let width = n * font::advance(S) - S;
    let x0 = cx - width / 2;
    for pass in 0..2 {
        for (i, ch) in name.chars().enumerate() {
            let g = font::glyph(ch);
            let gx = x0 + i as i32 * font::advance(S);
            for (row, bits) in g.iter().enumerate() {
                for col in 0..font::GLYPH_W {
                    if bits >> (font::GLYPH_W - 1 - col) & 1 == 0 {
                        continue;
                    }
                    let (px, py) = (gx + col * S, y + row as i32 * S);
                    if pass == 0 {
                        for oy in -1..=S {
                            for ox in -1..=S {
                                blend_px(canvas, px + ox, py + oy, [0, 0, 0, 255]);
                            }
                        }
                    } else {
                        for oy in 0..S {
                            for ox in 0..S {
                                blend_px(canvas, px + ox, py + oy, [255, 255, 255, 255]);
                            }
                        }
                    }
                }
            }
        }
    }
    let _ = font::GLYPH_H;
}

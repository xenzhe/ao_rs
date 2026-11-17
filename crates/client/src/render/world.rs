//! Map and characters, in the official AO20 order (TileEngine_RenderScreen.bas):
//! floor, walkable overlays (layers 2-3), then row by row characters and layer 4 (trees
//! fade when you walk behind them), and finally layer-5 roofs that fade while under them.
//!
//! Continuous world: tiles beyond the current map's border come from the loaded neighbour
//! maps (`client_core::p3::world::ContinuousWorld`), so the edge of the map shows the land
//! that continues instead of black. Characters are only drawn inside the official view.
use super::layout::TILE;
use crate::assets::Assets;
use crate::game::{Ent, Game};
use ao_formats::csm::{tile, MAP_SIZE};
use ao_formats::AoMap;
use ao_rules::{VIEW_TILES_H, VIEW_TILES_W};
use client_core::p3::world::ContinuousWorld;
use macroquad::prelude::*;
use std::collections::HashMap;

const TREE_MIN_ALPHA: f32 = 130.0 / 255.0;
const TREE_FADE_SECS: f32 = 0.15;

/// Fades that persist between frames.
#[derive(Default)]
pub struct WorldFades {
    trees: HashMap<usize, f32>,
    roofs: HashMap<u16, f32>,
}

impl WorldFades {
    pub fn clear(&mut self) {
        self.trees.clear();
        self.roofs.clear();
    }
}

/// The current map plus its neighbours, looked up per tile in current-map coordinates.
struct Tiles<'a> {
    map: &'a AoMap,
    cw: &'a ContinuousWorld,
}

impl Tiles<'_> {
    fn layer(&self, l: usize, x: i32, y: i32) -> u32 {
        if (1..=MAP_SIZE).contains(&x) && (1..=MAP_SIZE).contains(&y) {
            return self.map.layer(l, x, y);
        }
        match self.cw.resolve(x, y) {
            Some((m, nx, ny)) => self.cw.map(m).map(|m| m.layer(l, nx, ny)).unwrap_or(0),
            None => 0,
        }
    }
}

/// `view` is the area to fill, in world-target pixels relative to `origin` (the official
/// view is (0, 0, RT_W, RT_H); widescreen adds negative/extra room around it).
#[allow(clippy::too_many_arguments)]
pub fn draw_world(game: &Game, assets: &mut Assets, fades: &mut WorldFades, cw: &ContinuousWorld, origin: Vec2, view: Rect, t_ms: f32, dt: f32) {
    let Some(map) = &game.map else { return };
    let tiles = Tiles { map, cw };
    // Official view, in tiles: characters only inside it (+1 for those walking in).
    let (ox0, oy0) = ((origin.x / TILE).floor() as i32 + 1, (origin.y / TILE).floor() as i32 + 1);
    let (ox1, oy1) = (ox0 + VIEW_TILES_W, oy0 + VIEW_TILES_H);
    // Whole window, in tiles.
    let (tx0, ty0) = (((origin.x + view.x) / TILE).floor() as i32 + 1, ((origin.y + view.y) / TILE).floor() as i32 + 1);
    let (tx1, ty1) = (((origin.x + view.right()) / TILE).ceil() as i32 + 1, ((origin.y + view.bottom()) / TILE).ceil() as i32 + 1);
    // Big sprites are anchored to their bottom tile: draw a margin around the view.
    let (bx0, bx1, by0, by1) = (tx0 - 6, tx1 + 6, ty0 - 1, ty1 + 10);
    let px = |x: i32| (x - 1) as f32 * TILE - origin.x;
    let py = |y: i32| (y - 1) as f32 * TILE - origin.y;
    let own = |x: i32, y: i32| (1..=MAP_SIZE).contains(&x) && (1..=MAP_SIZE).contains(&y);

    // 1. Floor.
    for y in ty0 - 1..=ty1 {
        for x in tx0 - 1..=tx1 {
            let g = tiles.layer(0, x, y);
            if g > 0 {
                assets.draw(g, px(x), py(y), false, t_ms, WHITE);
            }
        }
    }

    // 2. Walkable overlays (shores, bridges), each layer fully before the next.
    for l in [1usize, 2] {
        for y in by0..=by1 {
            for x in bx0..=bx1 {
                let g = tiles.layer(l, x, y);
                if g > 0 {
                    assets.draw(g, px(x), py(y), true, t_ms, WHITE);
                }
            }
        }
    }

    // 3. Row by row: characters, then layer 4.
    let (me_x, me_y) = game.me().map(|e| (e.x, e.y)).unwrap_or((0, 0));
    let mut by_row: HashMap<i32, Vec<&Ent>> = HashMap::new();
    for e in game.ents.values() {
        if e.x >= ox0 - 1 && e.x <= ox1 && e.y >= oy0 - 1 && e.y <= oy1 + 1 {
            by_row.entry(e.y).or_default().push(e);
        }
    }
    for y in by0..=by1 {
        if let Some(row) = by_row.get_mut(&y) {
            row.sort_by_key(|e| e.x);
            for e in row.iter() {
                super::characters::draw_character(assets, e, origin, t_ms);
            }
        }
        for x in bx0..=bx1 {
            let g = tiles.layer(3, x, y);
            if g == 0 {
                continue;
            }
            let mut alpha = 1.0;
            if assets.db.trees.contains(&g) {
                if own(x, y) {
                    let behind = (me_x - x).abs() <= 3 && (y - me_y) > 0 && (y - me_y) < 12;
                    let a = fades.trees.entry(tile(x, y)).or_insert(1.0);
                    let target = if behind { TREE_MIN_ALPHA } else { 1.0 };
                    *a += (target - *a) * (dt / TREE_FADE_SECS).min(1.0);
                    alpha = *a;
                }
                // Soft drop shadow, like Draw_Sombra.
                assets.draw(g, px(x) + 10.0, py(y) + 4.0, true, t_ms, Color::new(0.0, 0.0, 0.0, 0.22 * alpha));
            }
            assets.draw(g, px(x), py(y), true, t_ms, Color::new(1.0, 1.0, 1.0, alpha));
        }
    }

    // 4. Roofs: fade out the one we are standing under (neighbour roofs stay solid).
    let my_trigger = map.trigger(me_x, me_y);
    let mut updated = std::collections::HashSet::new();
    for y in by0..=by1 {
        for x in bx0..=bx1 {
            let g = tiles.layer(4, x, y);
            if g == 0 {
                continue;
            }
            let roof = if own(x, y) { map.near_roof(x, y) } else { 0 };
            let mut alpha = 1.0;
            if roof != 0 {
                let a = fades.roofs.entry(roof).or_insert(1.0);
                if updated.insert(roof) {
                    let target = if map.is_roof(my_trigger) && my_trigger == roof { 0.0 } else { 1.0 };
                    *a += (target - *a) * (dt / 0.25).min(1.0);
                }
                alpha = *a;
            }
            assets.draw(g, px(x), py(y), true, t_ms, Color::new(1.0, 1.0, 1.0, alpha));
        }
    }
}

/// Fog outside the official view: a soft haze that grows over two tiles past its border and
/// keeps deepening towards the window's edge. `view` is the whole window, `inner` the
/// official view, both in target pixels.
pub fn draw_fog(view: Rect, inner: Rect) {
    if view.w <= inner.w + 1.0 && view.h <= inner.h + 1.0 {
        return;
    }
    let haze = |a: f32| Color::new(0.05, 0.06, 0.08, a);
    let soft = 2.0 * TILE;
    let mid = Rect::new(inner.x - soft, inner.y - soft, inner.w + 2.0 * soft, inner.h + 2.0 * soft);
    ring(inner, haze(0.0), mid, haze(0.42));
    let outer = Rect::new(view.x.min(mid.x), view.y.min(mid.y), view.right().max(mid.right()) - view.x.min(mid.x), view.bottom().max(mid.bottom()) - view.y.min(mid.y));
    ring(mid, haze(0.42), outer, haze(0.72));
}

/// Gradient frame between two nested rectangles (4 trapezoids, vertex colours).
fn ring(a: Rect, ca: Color, b: Rect, cb: Color) {
    let ai = [vec2(a.x, a.y), vec2(a.right(), a.y), vec2(a.right(), a.bottom()), vec2(a.x, a.bottom())];
    let bo = [vec2(b.x, b.y), vec2(b.right(), b.y), vec2(b.right(), b.bottom()), vec2(b.x, b.bottom())];
    let mut vertices = Vec::with_capacity(8);
    for p in ai {
        vertices.push(Vertex::new(p.x, p.y, 0.0, 0.0, 0.0, ca));
    }
    for p in bo {
        vertices.push(Vertex::new(p.x, p.y, 0.0, 0.0, 0.0, cb));
    }
    let mut indices = Vec::with_capacity(24);
    for i in 0..4u16 {
        let j = (i + 1) % 4;
        indices.extend_from_slice(&[i, j, 4 + i, j, 4 + j, 4 + i]);
    }
    draw_mesh(&Mesh { vertices, indices, texture: None });
}

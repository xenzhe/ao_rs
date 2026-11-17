//! Map and characters, in the official AO20 order (TileEngine_RenderScreen.bas):
//! floor, walkable overlays (layers 2-3), then row by row characters and layer 4 (trees
//! fade when you walk behind them), and finally layer-5 roofs that fade while under them.
use super::layout::TILE;
use crate::assets::Assets;
use crate::game::{Ent, Game};
use ao_formats::csm::{is_roof_trigger, tile};
use ao_rules::{VIEW_TILES_H, VIEW_TILES_W};
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

pub fn draw_world(game: &Game, assets: &mut Assets, fades: &mut WorldFades, origin: Vec2, t_ms: f32, dt: f32) {
    let Some(map) = &game.map else { return };
    let (tx0, ty0) = ((origin.x / TILE).floor() as i32 + 1, (origin.y / TILE).floor() as i32 + 1);
    let (tx1, ty1) = (tx0 + VIEW_TILES_W, ty0 + VIEW_TILES_H);
    // Big sprites are anchored to their bottom tile: draw a margin around the view.
    let (bx0, bx1, by0, by1) = (tx0 - 6, tx1 + 6, ty0 - 1, ty1 + 10);
    let px = |x: i32| (x - 1) as f32 * TILE - origin.x;
    let py = |y: i32| (y - 1) as f32 * TILE - origin.y;

    // 1. Floor.
    for y in ty0 - 1..=ty1 {
        for x in tx0 - 1..=tx1 {
            let g = map.layer(0, x, y);
            if g > 0 {
                assets.draw(g, px(x), py(y), false, t_ms, WHITE);
            }
        }
    }

    // 2. Walkable overlays (shores, bridges), each layer fully before the next.
    for l in [1usize, 2] {
        for y in by0..=by1 {
            for x in bx0..=bx1 {
                let g = map.layer(l, x, y);
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
        by_row.entry(e.y).or_default().push(e);
    }
    for y in by0..=by1 {
        if let Some(row) = by_row.get_mut(&y) {
            row.sort_by_key(|e| e.x);
            for e in row.iter() {
                super::characters::draw_character(assets, e, origin, t_ms);
            }
        }
        for x in bx0..=bx1 {
            let g = map.layer(3, x, y);
            if g == 0 {
                continue;
            }
            let mut alpha = 1.0;
            if assets.db.trees.contains(&g) {
                let behind = (me_x - x).abs() <= 3 && (y - me_y) > 0 && (y - me_y) < 12;
                let a = fades.trees.entry(tile(x, y)).or_insert(1.0);
                let target = if behind { TREE_MIN_ALPHA } else { 1.0 };
                *a += (target - *a) * (dt / TREE_FADE_SECS).min(1.0);
                alpha = *a;
                // Soft drop shadow, like Draw_Sombra.
                assets.draw(g, px(x) + 10.0, py(y) + 4.0, true, t_ms, Color::new(0.0, 0.0, 0.0, 0.22 * alpha));
            }
            assets.draw(g, px(x), py(y), true, t_ms, Color::new(1.0, 1.0, 1.0, alpha));
        }
    }

    // 4. Roofs: fade out the one we are standing under.
    let my_trigger = map.trigger(me_x, me_y);
    let mut updated = std::collections::HashSet::new();
    for y in by0..=by1 {
        for x in bx0..=bx1 {
            let g = map.layer(4, x, y);
            if g == 0 {
                continue;
            }
            let roof = map.near_roof(x, y);
            let mut alpha = 1.0;
            if roof != 0 {
                let a = fades.roofs.entry(roof).or_insert(1.0);
                if updated.insert(roof) {
                    let target = if is_roof_trigger(my_trigger) && my_trigger == roof { 0.0 } else { 1.0 };
                    *a += (target - *a) * (dt / 0.25).min(1.0);
                }
                alpha = *a;
            }
            assets.draw(g, px(x), py(y), true, t_ms, Color::new(1.0, 1.0, 1.0, alpha));
        }
    }
}

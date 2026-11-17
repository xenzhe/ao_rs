//! DX9-style light map: the ambient colour of the hour, plus additive lights (the
//! player's torch, the map's own lights from the .csm, spell effects). The result is
//! multiplied over the world.
use super::fx::Fx;
use super::layout::{RT_H, RT_W, TILE};
use crate::game::{tile_center, v, Game};
use macroquad::prelude::*;

/// 24 in-game hours every 4 real minutes.
pub const DAY_SECONDS: f32 = 240.0;

pub fn hour(offset: f32, now: f64) -> f32 {
    (offset + now as f32 / DAY_SECONDS * 24.0).rem_euclid(24.0)
}

pub fn ambient(hour: f32, dungeon: bool) -> Color {
    if dungeon {
        return Color::new(0.32, 0.30, 0.36, 1.0);
    }
    let night = Color::new(0.30, 0.34, 0.56, 1.0);
    let dusk = Color::new(0.95, 0.66, 0.50, 1.0);
    let day = WHITE;
    let mix = |a: Color, b: Color, t: f32| Color::new(a.r + (b.r - a.r) * t, a.g + (b.g - a.g) * t, a.b + (b.b - a.b) * t, 1.0);
    match hour {
        h if h < 5.0 => night,
        h if h < 6.5 => mix(night, dusk, (h - 5.0) / 1.5),
        h if h < 8.0 => mix(dusk, day, (h - 6.5) / 1.5),
        h if h < 18.0 => day,
        h if h < 19.5 => mix(day, dusk, (h - 18.0) / 1.5),
        h if h < 21.0 => mix(dusk, night, (h - 19.5) / 1.5),
        _ => night,
    }
}

/// Fills the current render target (the light map) for this frame.
pub fn build_light_map(game: &Game, fx: &Fx, origin: Vec2, amb: Color, t_ms: f32) {
    clear_background(amb);
    let night = (1.0 - amb.g).clamp(0.0, 1.0);
    gl_use_material(&fx.additive);
    if night > 0.02 {
        if let Some(me) = game.me() {
            let flick = 1.0 + (t_ms * 0.009).sin() * 0.03 + (t_ms * 0.023).sin() * 0.02;
            fx.light(v(me.world_pos()) - origin - vec2(0.0, 20.0), 140.0 * flick, Color::new(1.0, 0.75, 0.45, 0.85 * night));
        }
        if let Some(map) = &game.map {
            for l in &map.lights {
                // Official ranges: < 100 square lights, >= 100 round lights of (range - 99).
                let tiles = if l.range < 100 { l.range as f32 } else { (l.range - 99) as f32 };
                let p = v(tile_center(l.x, l.y)) - origin;
                if p.x < -400.0 || p.y < -400.0 || p.x > RT_W + 400.0 || p.y > RT_H + 400.0 {
                    continue;
                }
                let c = l.rgba;
                let flick = 1.0 + ((t_ms * 0.007) + l.x as f32).sin() * 0.04;
                fx.light(p, (tiles * TILE + 24.0) * flick, Color::new(c[0] as f32 / 255.0, c[1] as f32 / 255.0, c[2] as f32 / 255.0, night));
            }
        }
    }
    for (p, r, c) in fx.lights() {
        fx.light(p - origin, r, Color::new(c.r, c.g, c.b, c.a * night.max(0.25)));
    }
    gl_use_default_material();
}

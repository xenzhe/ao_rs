//! Screen layout for any resolution: AO-style console on top, world in the middle scaled
//! to fit (optionally pixel-perfect), side panel on the right, chat bar at the bottom.
use ao_rules::{VIEW_TILES_H, VIEW_TILES_W};
use macroquad::prelude::*;

pub const TILE: f32 = 32.0;
/// Size of the world render target: the official 23x18 tile window.
pub const RT_W: f32 = VIEW_TILES_W as f32 * TILE;
pub const RT_H: f32 = VIEW_TILES_H as f32 * TILE;

pub struct Layout {
    /// UI scale factor (1.0 at 800 px of height).
    pub u: f32,
    pub panel: Rect,
    pub console: Rect,
    pub bottom: Rect,
    /// Where the world render target is drawn on screen.
    pub world: Rect,
    /// World pixels -> screen pixels.
    pub scale: f32,
}

impl Layout {
    pub fn compute(pixel_perfect: bool) -> Layout {
        let (sw, sh) = (screen_width(), screen_height());
        let u = (sh / 800.0).clamp(0.85, 2.5);
        let m = 6.0 * u;
        let panel_w = 232.0 * u;
        let console_h = 96.0 * u;
        let bottom_h = 58.0 * u;
        let left_w = sw - panel_w - 3.0 * m;
        let panel = Rect::new(sw - panel_w - m, m, panel_w, sh - 2.0 * m);
        let console = Rect::new(m, m, left_w, console_h);
        let bottom = Rect::new(m, sh - m - bottom_h, left_w, bottom_h);
        let area = Rect::new(m, console.bottom() + m, left_w, bottom.y - console.bottom() - 2.0 * m);
        let mut scale = (area.w / RT_W).min(area.h / RT_H).max(0.25);
        if pixel_perfect && scale >= 1.0 {
            scale = scale.floor();
        }
        let (w, h) = (RT_W * scale, RT_H * scale);
        let world = Rect::new((area.x + (area.w - w) / 2.0).round(), (area.y + (area.h - h) / 2.0).round(), w, h);
        Layout { u, panel, console, bottom, world, scale }
    }

    /// Screen position -> world pixel, if inside the world view.
    pub fn screen_to_world(&self, p: Vec2, origin: Vec2) -> Option<Vec2> {
        self.world.contains(p).then(|| (p - vec2(self.world.x, self.world.y)) / self.scale + origin)
    }

    pub fn world_to_screen(&self, w: Vec2, origin: Vec2) -> Vec2 {
        vec2(self.world.x, self.world.y) + (w - origin) * self.scale
    }
}

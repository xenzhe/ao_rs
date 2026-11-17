//! Screen layout for any resolution: AO-style console on top, world in the middle scaled
//! to fit (optionally pixel-perfect), side panel on the right, chat bar at the bottom.
//!
//! Widescreen: the official 23x18 view (`view`) keeps the size it always had; the world
//! window (`world`) grows around it to fill the free space, showing more of the map (the
//! client has it) but fogged and without characters, since the server only sends those
//! inside the official range. No PvP advantage, just a wider stage.
use ao_rules::{VIEW_TILES_H, VIEW_TILES_W};
use macroquad::prelude::*;

pub const TILE: f32 = 32.0;
/// Size of the official 23x18 tile window, in world pixels.
pub const RT_W: f32 = VIEW_TILES_W as f32 * TILE;
pub const RT_H: f32 = VIEW_TILES_H as f32 * TILE;
/// Most extra tiles drawn on each side of the official view.
const MAX_EXTRA_X: f32 = 16.0;
const MAX_EXTRA_Y: f32 = 10.0;

pub struct Layout {
    /// UI scale factor (1.0 at 800 px of height).
    pub u: f32,
    pub panel: Rect,
    pub console: Rect,
    pub bottom: Rect,
    /// The whole world window on screen (official view plus the fogged surroundings).
    pub world: Rect,
    /// The official 23x18 view on screen, inside `world`; the camera origin is its top-left.
    pub view: Rect,
    /// World pixels -> screen pixels.
    pub scale: f32,
}

impl Layout {
    pub fn compute(pixel_perfect: bool) -> Layout {
        Layout::for_screen(screen_width(), screen_height(), pixel_perfect)
    }

    pub fn for_screen(sw: f32, sh: f32, pixel_perfect: bool) -> Layout {
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
        let view = Rect::new((area.x + (area.w - w) / 2.0).round(), (area.y + (area.h - h) / 2.0).round(), w, h);
        // Whole world pixels around the view, so the target maps 1:1 onto the screen.
        let ex = (((area.w - w) / 2.0 / scale).floor()).clamp(0.0, MAX_EXTRA_X * TILE);
        let ey = (((area.h - h) / 2.0 / scale).floor()).clamp(0.0, MAX_EXTRA_Y * TILE);
        let world = Rect::new(view.x - ex * scale, view.y - ey * scale, w + 2.0 * ex * scale, h + 2.0 * ey * scale);
        Layout { u, panel, console, bottom, world, view, scale }
    }

    /// Size of the world render target in world pixels (whole `world` window).
    pub fn rt_size(&self) -> Vec2 {
        vec2((self.world.w / self.scale).round(), (self.world.h / self.scale).round())
    }

    /// Top-left of the world window relative to the camera origin, in world pixels (<= 0).
    pub fn rt_offset(&self) -> Vec2 {
        vec2(((self.world.x - self.view.x) / self.scale).round(), ((self.world.y - self.view.y) / self.scale).round())
    }

    /// Screen position -> world pixel, only inside the official view (the fogged surroundings
    /// are scenery: nothing there can be targeted).
    pub fn screen_to_world(&self, p: Vec2, origin: Vec2) -> Option<Vec2> {
        self.view.contains(p).then(|| (p - vec2(self.view.x, self.view.y)) / self.scale + origin)
    }

    pub fn world_to_screen(&self, w: Vec2, origin: Vec2) -> Vec2 {
        vec2(self.view.x, self.view.y) + (w - origin) * self.scale
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_resolution_keeps_the_official_view_inside_the_world() {
        for (sw, sh) in [(1280.0, 720.0), (1280.0, 800.0), (1920.0, 1080.0), (2560.0, 1080.0), (2560.0, 1440.0), (3440.0, 1440.0), (1024.0, 768.0)] {
            for pp in [false, true] {
                let l = Layout::for_screen(sw, sh, pp);
                assert!(l.world.x <= l.view.x && l.world.right() >= l.view.right() - 0.01, "{sw}x{sh}");
                assert!(l.world.x >= 0.0 && l.world.right() <= l.panel.x, "{sw}x{sh} world overlaps the panel");
                assert!(l.world.y >= l.console.bottom() && l.world.bottom() <= l.bottom.y, "{sw}x{sh}");
                let rt = l.rt_size();
                assert!((rt.x * l.scale - l.world.w).abs() < 1.0 && rt.x >= RT_W, "{sw}x{sh}");
                if pp && l.scale >= 1.0 {
                    assert_eq!(l.scale, l.scale.floor());
                }
                let o = vec2(320.0, 640.0);
                let p = l.world_to_screen(o + vec2(100.0, 50.0), o);
                assert_eq!(l.screen_to_world(p, o).map(|w| w.round()), Some(o + vec2(100.0, 50.0)));
            }
        }
        // Ultra-wide shows more world than 23 tiles.
        let l = Layout::for_screen(2560.0, 1080.0, false);
        assert!(l.rt_size().x > RT_W + 8.0 * TILE);
    }
}

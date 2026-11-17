//! Screen layout for any resolution. The world fills the whole screen (`stage`); the HUD
//! floats over it as separate pieces:
//!
//! - `console`: the chat, top-left, over the world (semi-transparent while unused).
//! - `panel`: the minimap column, top-right (map, zone, clock, weather).
//! - `bottom`: the deck band at the bottom (portrait, life/mana orbs, action bar, window
//!   icons). The official view never goes under it.
//! - `world`: the free world area, i.e. everything above the deck. Windows use it to place
//!   banners (`world.y`, `world.bottom()`) and to tell "dropped on the ground".
//! - `view`: the official 23x18 view (no PvP advantage), centred above the deck. Around it
//!   the map is drawn fogged and without characters (the server only sends those in range).
use ao_rules::{VIEW_TILES_H, VIEW_TILES_W};
use macroquad::prelude::*;

pub const TILE: f32 = 32.0;
/// Size of the official 23x18 tile window, in world pixels.
pub const RT_W: f32 = VIEW_TILES_W as f32 * TILE;
pub const RT_H: f32 = VIEW_TILES_H as f32 * TILE;
/// Most extra tiles drawn on each side of the official view (beyond that, plain backdrop).
const MAX_EXTRA_X: f32 = 30.0;
const MAX_EXTRA_Y: f32 = 18.0;
/// Height of the deck band in UI units.
pub const DECK_H: f32 = 104.0;

pub struct Layout {
    /// UI scale factor (1.0 at 800 px of height).
    pub u: f32,
    /// Minimap column (top-right).
    pub panel: Rect,
    /// Chat, collapsed (top-left).
    pub console: Rect,
    /// Deck band at the bottom (full width).
    pub bottom: Rect,
    /// Free world area above the deck (full width).
    pub world: Rect,
    /// The official 23x18 view on screen; the camera origin is its top-left.
    pub view: Rect,
    /// What the world render target actually covers on screen (the whole screen or more).
    pub stage: Rect,
    /// World pixels -> screen pixels.
    pub scale: f32,
}

impl Layout {
    pub fn compute(pixel_perfect: bool) -> Layout {
        Layout::for_screen(screen_width(), screen_height(), pixel_perfect)
    }

    pub fn for_screen(sw: f32, sh: f32, pixel_perfect: bool) -> Layout {
        let u = (sh / 800.0).clamp(0.85, 2.5);
        let m = 8.0 * u;
        let deck_h = DECK_H * u;
        let bottom = Rect::new(0.0, sh - deck_h, sw, deck_h);
        let world = Rect::new(0.0, 0.0, sw, bottom.y);
        // The view sits in the free area, with a small margin all round.
        let area = Rect::new(m, m, sw - 2.0 * m, bottom.y - 2.0 * m);
        let mut scale = (area.w / RT_W).min(area.h / RT_H).max(0.25);
        if pixel_perfect && scale >= 1.0 {
            scale = scale.floor();
        }
        let (w, h) = (RT_W * scale, RT_H * scale);
        let view = Rect::new((area.x + (area.w - w) / 2.0).round(), (area.y + (area.h - h) / 2.0).round(), w, h);
        // Whole world pixels around the view, enough to cover the screen (1:1 target).
        let ex = (view.x.max(sw - view.right()) / scale).ceil().clamp(0.0, MAX_EXTRA_X * TILE);
        let ey = (view.y.max(sh - view.bottom()) / scale).ceil().clamp(0.0, MAX_EXTRA_Y * TILE);
        let stage = Rect::new(view.x - ex * scale, view.y - ey * scale, w + 2.0 * ex * scale, h + 2.0 * ey * scale);
        // Chat: top-left, as wide as reads well; minimap column: top-right.
        // (It leaves the top centre free for the target plaque.)
        let console = Rect::new(m, m, (560.0 * u).min(view.center().x - 135.0 * u - m), 112.0 * u);
        let side = (196.0 * u).min((sw - view.right() - 2.0 * m).max(150.0 * u));
        let panel = Rect::new(sw - side - m, m, side, side + 74.0 * u);
        Layout { u, panel, console, bottom, world, view, stage, scale }
    }

    /// Size of the world render target in world pixels (whole `stage`).
    pub fn rt_size(&self) -> Vec2 {
        vec2((self.stage.w / self.scale).round(), (self.stage.h / self.scale).round())
    }

    /// Top-left of the stage relative to the camera origin, in world pixels (<= 0).
    pub fn rt_offset(&self) -> Vec2 {
        vec2(((self.stage.x - self.view.x) / self.scale).round(), ((self.stage.y - self.view.y) / self.scale).round())
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
    fn every_resolution_keeps_the_official_view_clear_of_the_deck() {
        for (sw, sh) in [(1280.0, 720.0), (1280.0, 800.0), (1920.0, 1080.0), (2560.0, 1080.0), (2560.0, 1440.0), (3440.0, 1440.0), (1024.0, 768.0)] {
            for pp in [false, true] {
                let l = Layout::for_screen(sw, sh, pp);
                assert!(l.stage.x <= l.view.x && l.stage.right() >= l.view.right() - 0.01, "{sw}x{sh}");
                assert!(l.view.bottom() <= l.bottom.y && l.view.y >= 0.0, "{sw}x{sh} view under the deck");
                assert!(l.world.contains(l.view.point()) && l.world.bottom() <= l.bottom.y + 0.01, "{sw}x{sh}");
                assert!(l.console.bottom() < l.bottom.y && l.panel.right() <= sw, "{sw}x{sh}");
                // The stage covers the screen unless the extra tiles run out (very wide).
                if sw <= 2560.0 {
                    assert!(l.stage.x <= 0.5 && l.stage.right() >= sw - 0.5 && l.stage.y <= 0.5 && l.stage.bottom() >= sh - 0.5, "{sw}x{sh} stage");
                }
                let rt = l.rt_size();
                assert!((rt.x * l.scale - l.stage.w).abs() < 1.0 && rt.x >= RT_W, "{sw}x{sh}");
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

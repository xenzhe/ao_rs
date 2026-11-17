//! Reusable UI pieces in the AO look (dark wood panels, gold borders). Sizes scale by `u`.
use macroquad::prelude::*;

pub const GOLD: Color = Color::new(0.62, 0.48, 0.24, 1.0);
pub const GOLD_TEXT: Color = Color::new(0.85, 0.65, 0.3, 1.0);

pub fn panel(r: Rect, u: f32) {
    draw_rectangle(r.x, r.y, r.w, r.h, Color::new(0.09, 0.07, 0.05, 0.96));
    draw_rectangle_lines(r.x, r.y, r.w, r.h, 2.0 * u, GOLD);
    draw_rectangle_lines(r.x + 3.0 * u, r.y + 3.0 * u, r.w - 6.0 * u, r.h - 6.0 * u, 1.0, Color::new(0.30, 0.22, 0.12, 1.0));
}

/// Text with a 1 px shadow, pixel-aligned.
pub fn text(s: &str, x: f32, y: f32, size: f32, c: Color) {
    draw_text(s, x.round() + 1.0, y.round() + 1.0, size, Color::new(0.0, 0.0, 0.0, c.a * 0.8));
    draw_text(s, x.round(), y.round(), size, c);
}

pub fn text_centered(s: &str, cx: f32, y: f32, size: f32, c: Color) {
    let d = measure_text(s, None, size as u16, 1.0);
    text(s, cx - d.width / 2.0, y, size, c);
}

#[allow(clippy::too_many_arguments)]
pub fn bar(r: Rect, v: f32, max: f32, c: Color, label: &str, u: f32) {
    draw_rectangle(r.x, r.y, r.w, r.h, Color::new(0.0, 0.0, 0.0, 0.7));
    let f = if max > 0.0 { (v / max).clamp(0.0, 1.0) } else { 0.0 };
    draw_rectangle(r.x, r.y, r.w * f, r.h, c);
    draw_rectangle(r.x, r.y, r.w * f, r.h * 0.35, Color::new(1.0, 1.0, 1.0, 0.18));
    draw_rectangle_lines(r.x, r.y, r.w, r.h, 1.0, GOLD);
    if !label.is_empty() {
        let size = 16.0 * u;
        let d = measure_text(label, None, size as u16, 1.0);
        text(label, r.x + (r.w - d.width) / 2.0, r.y + r.h / 2.0 + d.height / 2.0, size, WHITE);
    }
}

/// Returns true when clicked this frame.
pub fn button(r: Rect, label: &str, active: bool, u: f32) -> bool {
    let hover = r.contains(mouse_position().into());
    let bg = if active {
        Color::new(0.45, 0.30, 0.10, 1.0)
    } else if hover {
        Color::new(0.25, 0.18, 0.10, 1.0)
    } else {
        Color::new(0.16, 0.12, 0.08, 1.0)
    };
    draw_rectangle(r.x, r.y, r.w, r.h, bg);
    draw_rectangle_lines(r.x, r.y, r.w, r.h, 1.0, GOLD);
    let size = 17.0 * u;
    let d = measure_text(label, None, size as u16, 1.0);
    text(label, r.x + (r.w - d.width) / 2.0, r.y + r.h / 2.0 + d.height / 2.0, size, if active { YELLOW } else { WHITE });
    hover && is_mouse_button_pressed(MouseButton::Left)
}

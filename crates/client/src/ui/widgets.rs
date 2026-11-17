//! Reusable UI pieces in the AO look (see `theme`). Sizes scale by `u`. Window owners may
//! add their own helpers in their files; generic ones go here (ask the integrator).
#![allow(dead_code)] // toolkit: pieces used by the P3 windows
use super::theme::{font, FontKind, GOLD, SLOT, TEXT};
pub use super::theme::GOLD_TEXT;
use macroquad::prelude::*;

pub fn panel(r: Rect, u: f32) {
    draw_rectangle(r.x, r.y, r.w, r.h, super::theme::WOOD);
    draw_rectangle_lines(r.x, r.y, r.w, r.h, 2.0 * u, GOLD);
    draw_rectangle_lines(r.x + 3.0 * u, r.y + 3.0 * u, r.w - 6.0 * u, r.h - 6.0 * u, 1.0, Color::new(0.30, 0.22, 0.12, 1.0));
}

/// Glyph sizes actually rasterised. macroquad caches every (font, size) pair in one atlas;
/// with `size * u` for any `u` the atlas fills up and later text silently stops drawing
/// (seen at 1920x1080). Sizes snap to this ladder and the rest is done with `font_scale`.
/// Few sizes on purpose: with 9 sizes the atlas still overflowed when a text-heavy window
/// (Character) was open. Text is scaled at most 1.5x from these.
const GLYPH_SIZES: [u16; 4] = [16, 24, 36, 54];

fn glyph(size: f32) -> (u16, f32) {
    let base = GLYPH_SIZES.iter().copied().find(|b| *b as f32 >= size).unwrap_or(48);
    (base, size / base as f32)
}

/// Text in a font, with a 1 px shadow, pixel-aligned. `y` is the baseline.
pub fn text_with(kind: FontKind, s: &str, x: f32, y: f32, size: f32, c: Color) {
    let f = font(kind);
    let (base, scale) = glyph(size);
    let params = |color| TextParams { font: f.as_ref(), font_size: base, font_scale: scale, color, ..Default::default() };
    draw_text_ex(s, x.round() + 1.0, y.round() + 1.0, params(Color::new(0.0, 0.0, 0.0, c.a * 0.8)));
    draw_text_ex(s, x.round(), y.round(), params(c));
}

/// Body text with shadow. `y` is the baseline.
pub fn text(s: &str, x: f32, y: f32, size: f32, c: Color) {
    text_with(FontKind::Body, s, x, y, size, c);
}

pub fn measure(s: &str, size: f32) -> TextDimensions {
    measure_with(FontKind::Body, s, size)
}

/// Size of `s` in a font, matching what `text_with` draws.
pub fn measure_with(kind: FontKind, s: &str, size: f32) -> TextDimensions {
    let (base, scale) = glyph(size);
    measure_text(s, font(kind).as_ref(), base, scale)
}

pub fn text_centered(s: &str, cx: f32, y: f32, size: f32, c: Color) {
    let d = measure(s, size);
    text(s, cx - d.width / 2.0, y, size, c);
}

/// Text cut with "..." to fit `max_w`.
pub fn text_fit(s: &str, x: f32, y: f32, size: f32, max_w: f32, c: Color) {
    if measure(s, size).width <= max_w {
        return text(s, x, y, size, c);
    }
    let mut t: String = s.to_string();
    while !t.is_empty() && measure(&format!("{t}..."), size).width > max_w {
        t.pop();
    }
    text(&format!("{t}..."), x, y, size, c);
}

pub fn bar(r: Rect, v: f32, max: f32, c: Color, label: &str, u: f32) {
    draw_rectangle(r.x, r.y, r.w, r.h, Color::new(0.0, 0.0, 0.0, 0.7));
    let f = if max > 0.0 { (v / max).clamp(0.0, 1.0) } else { 0.0 };
    draw_rectangle(r.x, r.y, r.w * f, r.h, c);
    draw_rectangle(r.x, r.y, r.w * f, r.h * 0.35, Color::new(1.0, 1.0, 1.0, 0.18));
    draw_rectangle_lines(r.x, r.y, r.w, r.h, 1.0, GOLD);
    if !label.is_empty() {
        let size = 16.0 * u;
        let d = measure(label, size);
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
    let d = measure(label, size);
    text(label, r.x + (r.w - d.width) / 2.0, r.y + r.h / 2.0 + d.height / 2.0, size, if active { YELLOW } else { WHITE });
    hover && is_mouse_button_pressed(MouseButton::Left)
}

/// An empty item/spell slot frame; returns (hovered, clicked).
pub fn slot(r: Rect, selected: bool, u: f32) -> (bool, bool) {
    let hover = r.contains(mouse_position().into());
    draw_rectangle(r.x, r.y, r.w, r.h, SLOT);
    let c = if selected { Color::new(1.0, 0.85, 0.3, 1.0) } else if hover { Color::new(0.85, 0.68, 0.36, 1.0) } else { Color::new(0.35, 0.26, 0.14, 1.0) };
    draw_rectangle_lines(r.x, r.y, r.w, r.h, if selected { 2.0 * u } else { 1.0 }, c);
    (hover, hover && is_mouse_button_pressed(MouseButton::Left))
}

/// A tooltip box near the mouse with several lines (first one in gold).
pub fn tooltip(lines: &[String], u: f32) {
    if lines.is_empty() {
        return;
    }
    let size = 16.0 * u;
    let w = lines.iter().map(|l| measure(l, size).width).fold(0.0, f32::max) + 16.0 * u;
    let h = lines.len() as f32 * 19.0 * u + 10.0 * u;
    let (mx, my) = mouse_position();
    let x = (mx + 18.0 * u).min(screen_width() - w - 4.0);
    let y = (my + 18.0 * u).min(screen_height() - h - 4.0);
    draw_rectangle(x, y, w, h, Color::new(0.04, 0.03, 0.02, 0.95));
    draw_rectangle_lines(x, y, w, h, 1.0, GOLD);
    for (i, l) in lines.iter().enumerate() {
        text(l, x + 8.0 * u, y + 20.0 * u + i as f32 * 19.0 * u, size, if i == 0 { GOLD_TEXT } else { TEXT });
    }
}

/// Single-line text field. `id` must be unique; it takes the keyboard focus on click and
/// edits `value` while focused (Enter returns true and keeps the focus). `mask` shows
/// dots (passwords). `filter` decides which characters are accepted.
#[allow(clippy::too_many_arguments)]
pub fn text_field(ui: &mut super::Ui, id: &str, value: &mut String, r: Rect, max: usize, mask: bool, filter: impl Fn(char) -> bool) -> bool {
    let u = ui.u;
    let focused = ui.focus.as_deref() == Some(id);
    if r.contains(mouse_position().into()) && is_mouse_button_pressed(MouseButton::Left) {
        ui.focus = Some(id.to_string());
        ui.mouse_captured = true;
    }
    draw_rectangle(r.x, r.y, r.w, r.h, Color::new(0.0, 0.0, 0.0, 0.6));
    draw_rectangle_lines(r.x, r.y, r.w, r.h, if focused { 2.0 } else { 1.0 }, if focused { GOLD_TEXT } else { GOLD });
    let mut enter = false;
    if focused {
        ui.keyboard_captured = true;
        while let Some(ch) = get_char_pressed() {
            if !ch.is_control() && filter(ch) && value.chars().count() < max {
                value.push(ch);
            }
        }
        if is_key_pressed(KeyCode::Backspace) {
            value.pop();
        }
        enter = is_key_pressed(KeyCode::Enter);
    }
    let shown = if mask { "•".repeat(value.chars().count()) } else { value.clone() };
    let cursor = if focused && (get_time() * 2.0) as i32 % 2 == 0 { "|" } else { "" };
    text_fit(&format!("{shown}{cursor}"), r.x + 8.0 * u, r.y + r.h / 2.0 + 6.0 * u, 18.0 * u, r.w - 16.0 * u, TEXT);
    enter
}

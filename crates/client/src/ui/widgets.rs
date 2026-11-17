//! Reusable UI pieces in the Numic AO look (docs/ESTILO_UI.md, `theme`). Sizes scale by
//! `u`. Window owners may add their own helpers in their files; generic ones go here.
#![allow(dead_code)] // toolkit
use super::fx_ui::{ease, key_of};
use super::theme::{self, font, FontKind, Surface, GOLD, GOLD_BRIGHT, GOLD_DIM, TEXT, TEXT_DIM, TEXT_FAINT};
pub use super::theme::GOLD_TEXT;
use macroquad::prelude::*;

/// A generic framed area (HUD pieces): leather with a metal border. Windows use the full
/// frame of `Theme::window_frame`; sections inside windows use no box (`theme::section`).
pub fn panel(r: Rect, u: f32) {
    theme::surface(r, Surface::Leather, u);
    theme::metal_border(r, 2.0 * u);
}

/// Glyph sizes actually rasterised. macroquad caches every (font, size) pair in one atlas;
/// with `size * u` for any `u` the atlas fills up and later text silently stops drawing
/// (seen at 1920x1080). Sizes snap to this ladder and the rest is done with `font_scale`.
/// Few sizes on purpose: with 9 sizes the atlas still overflowed when a text-heavy window
/// (Character) was open. Text is scaled at most 1.5x from these.
const GLYPH_SIZES: [u16; 4] = [16, 24, 36, 54];

pub(crate) fn glyph(size: f32) -> (u16, f32) {
    let base = GLYPH_SIZES.iter().copied().find(|b| *b as f32 >= size).unwrap_or(54);
    (base, size / base as f32)
}

/// Text in a font, with a soft shadow, pixel-aligned. `y` is the baseline.
pub fn text_with(kind: FontKind, s: &str, x: f32, y: f32, size: f32, c: Color) {
    let f = font(kind);
    let (base, scale) = glyph(size);
    let params = |color| TextParams { font: f.as_ref(), font_size: base, font_scale: scale, color, ..Default::default() };
    let sh = (size / 14.0).clamp(1.0, 2.0);
    draw_text_ex(s, x.round() + sh, y.round() + sh, params(Color::new(0.0, 0.0, 0.0, c.a * 0.75)));
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

/// Centred text in any font.
pub fn text_centered_with(kind: FontKind, s: &str, cx: f32, y: f32, size: f32, c: Color) {
    let d = measure_with(kind, s, size);
    text_with(kind, s, cx - d.width / 2.0, y, size, c);
}

/// Text cut with "…" to fit `max_w`.
pub fn text_fit(s: &str, x: f32, y: f32, size: f32, max_w: f32, c: Color) {
    if measure(s, size).width <= max_w {
        return text(s, x, y, size, c);
    }
    let mut t: String = s.to_string();
    while !t.is_empty() && measure(&format!("{t}…"), size).width > max_w {
        t.pop();
    }
    text(&format!("{t}…"), x, y, size, c);
}

/// Wraps `s` into lines that fit `max_w` (by words).
pub fn wrap_lines(kind: FontKind, s: &str, size: f32, max_w: f32) -> Vec<String> {
    let mut lines = Vec::new();
    for para in s.split('\n') {
        let mut cur = String::new();
        for w in para.split_whitespace() {
            let cand = if cur.is_empty() { w.to_string() } else { format!("{cur} {w}") };
            if measure_with(kind, &cand, size).width > max_w && !cur.is_empty() {
                lines.push(std::mem::take(&mut cur));
                cur = w.to_string();
            } else {
                cur = cand;
            }
        }
        lines.push(cur);
    }
    lines
}

/// A resource bar: recessed track, gradient fill with a gloss, value label inside.
pub fn bar(r: Rect, v: f32, max: f32, c: Color, label: &str, u: f32) {
    theme::surface(r, Surface::Recess, u);
    let f = if max > 0.0 { (v / max).clamp(0.0, 1.0) } else { 0.0 };
    let fill = Rect::new(r.x + 1.0, r.y + 1.0, (r.w - 2.0) * f, r.h - 2.0);
    if fill.w > 0.0 {
        draw_rectangle(fill.x, fill.y, fill.w, fill.h, c);
        draw_rectangle(fill.x, fill.y, fill.w, fill.h * 0.45, Color::new(1.0, 1.0, 1.0, 0.16));
        draw_rectangle(fill.x, fill.bottom() - fill.h * 0.25, fill.w, fill.h * 0.25, Color::new(0.0, 0.0, 0.0, 0.22));
        draw_line(fill.right(), fill.y, fill.right(), fill.bottom(), 1.0, Color::new(1.0, 1.0, 1.0, 0.35));
    }
    draw_rectangle_lines(r.x, r.y, r.w, r.h, 1.0, GOLD_DIM);
    if !label.is_empty() {
        let size = (r.h * 0.72).clamp(12.0 * u, 18.0 * u);
        let d = measure(label, size);
        text(label, r.x + (r.w - d.width) / 2.0, r.y + r.h / 2.0 + d.height * 0.42, size, WHITE);
    }
}

/// Button importance (docs/ESTILO_UI.md §4): one Primary per screen.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Kind {
    Primary,
    Secondary,
    Ghost,
    Danger,
}

/// A button of a kind; `enabled = false` draws it faded and never clicks. Returns true
/// when clicked this frame.
pub fn button_kind(r: Rect, label: &str, kind: Kind, enabled: bool, u: f32) -> bool {
    let hover = enabled && r.contains(mouse_position().into());
    let down = hover && is_mouse_button_down(MouseButton::Left);
    let h = ease(key_of(r), hover, 0.08);
    let press = if down { 1.0 * u } else { 0.0 };
    let rr = Rect::new(r.x, r.y + press, r.w, r.h);
    let a = if enabled { 1.0 } else { 0.45 };
    let (size, color) = match kind {
        Kind::Primary => {
            // Engraved gold: dark base, gold gradient, bright top bevel, glow on hover.
            if h > 0.0 {
                draw_rectangle(rr.x - 3.0 * u, rr.y - 3.0 * u, rr.w + 6.0 * u, rr.h + 6.0 * u, Color::new(1.0, 0.75, 0.3, 0.10 * h));
            }
            draw_rectangle(rr.x, rr.y, rr.w, rr.h, Color::new(0.30 + 0.05 * h, 0.20 + 0.04 * h, 0.08, a));
            draw_rectangle(rr.x, rr.y, rr.w, rr.h * 0.5, Color::new(0.62, 0.46, 0.20, (0.55 + 0.15 * h) * a));
            theme::metal_border(rr, 1.5 * u);
            (19.0 * u, Color::new(1.0, 0.93, 0.74, a))
        }
        Kind::Secondary => {
            theme::surface(rr, Surface::Raised, u);
            if h > 0.0 {
                draw_rectangle(rr.x, rr.y, rr.w, rr.h, Color::new(1.0, 0.85, 0.55, 0.07 * h));
            }
            draw_rectangle_lines(rr.x, rr.y, rr.w, rr.h, 1.0, Color::new(GOLD.r, GOLD.g, GOLD.b, (0.45 + 0.4 * h) * a));
            (17.0 * u, Color::new(TEXT.r, TEXT.g, TEXT.b, a))
        }
        Kind::Ghost => {
            let c = Color::new(GOLD_TEXT.r, GOLD_TEXT.g, GOLD_TEXT.b, (0.75 + 0.25 * h) * a);
            let d = measure(label, 17.0 * u);
            let y = rr.y + rr.h / 2.0 + d.height * 0.5 + 3.0 * u;
            let w = d.width * h;
            draw_line(rr.x + (rr.w - w) / 2.0, y, rr.x + (rr.w + w) / 2.0, y, 1.0, c);
            (17.0 * u, c)
        }
        Kind::Danger => {
            draw_rectangle(rr.x, rr.y, rr.w, rr.h, Color::new(0.30 + 0.12 * h, 0.07, 0.05, a));
            draw_rectangle_lines(rr.x, rr.y, rr.w, rr.h, 1.0, Color::new(0.75, 0.25, 0.2, a));
            (17.0 * u, Color::new(1.0, 0.88, 0.85, a))
        }
    };
    let kind_font = if kind == Kind::Primary { FontKind::Title } else { FontKind::Body };
    let d = measure_with(kind_font, label, size);
    text_with(kind_font, label, rr.x + (rr.w - d.width) / 2.0, rr.y + rr.h / 2.0 + d.height * 0.4, size, color);
    hover && is_mouse_button_pressed(MouseButton::Left)
}

/// Compatibility button: Secondary, or highlighted (selected tab / toggled on) when `active`.
pub fn button(r: Rect, label: &str, active: bool, u: f32) -> bool {
    if active {
        let hover = r.contains(mouse_position().into());
        draw_rectangle(r.x, r.y, r.w, r.h, Color::new(0.36, 0.25, 0.10, 1.0));
        draw_rectangle(r.x, r.y, r.w, r.h * 0.5, Color::new(1.0, 0.8, 0.4, 0.10));
        draw_rectangle_lines(r.x, r.y, r.w, r.h, 1.5, GOLD);
        draw_line(r.x + 3.0, r.bottom() - 2.0, r.right() - 3.0, r.bottom() - 2.0, 2.0, GOLD_BRIGHT);
        let size = 17.0 * u;
        let d = measure(label, size);
        text(label, r.x + (r.w - d.width) / 2.0, r.y + r.h / 2.0 + d.height * 0.4, size, GOLD_BRIGHT);
        return hover && is_mouse_button_pressed(MouseButton::Left);
    }
    button_kind(r, label, Kind::Secondary, true, u)
}

/// An item/spell slot (recessed); returns (hovered, clicked).
pub fn slot(r: Rect, selected: bool, u: f32) -> (bool, bool) {
    let hover = r.contains(mouse_position().into());
    theme::surface(r, Surface::Recess, u);
    let h = ease(key_of(r), hover || selected, 0.08);
    if h > 0.0 {
        draw_rectangle(r.x, r.y, r.w, r.h, Color::new(1.0, 0.8, 0.45, 0.06 * h));
    }
    let c = if selected {
        GOLD_BRIGHT
    } else {
        Color::new(GOLD_DIM.r + (GOLD.r - GOLD_DIM.r) * h, GOLD_DIM.g + (GOLD.g - GOLD_DIM.g) * h, GOLD_DIM.b + (GOLD.b - GOLD_DIM.b) * h, 0.7 + 0.3 * h)
    };
    draw_rectangle_lines(r.x, r.y, r.w, r.h, if selected { 2.0 * u } else { 1.0 }, c);
    (hover, hover && is_mouse_button_pressed(MouseButton::Left))
}

/// A tooltip near the mouse: first line is the title (Title font, gold), lines starting
/// with `~` are role-playing flavour (italic), lines starting with `!` are unmet
/// requirements (red), the rest are data.
pub fn tooltip(lines: &[String], u: f32) {
    if lines.is_empty() {
        return;
    }
    let pad = 12.0 * u;
    let max_w = 340.0 * u;
    let mut rows: Vec<(FontKind, String, Color, f32)> = Vec::new();
    for (i, l) in lines.iter().enumerate() {
        let (kind, txt, col, size) = if i == 0 {
            (FontKind::Title, l.as_str(), GOLD_BRIGHT, 19.0 * u)
        } else if let Some(t) = l.strip_prefix('~') {
            (FontKind::NarrativeItalic, t, TEXT_DIM, 16.0 * u)
        } else if let Some(t) = l.strip_prefix('!') {
            (FontKind::Body, t, theme::BAD, 16.0 * u)
        } else {
            (FontKind::Body, l.as_str(), TEXT, 16.0 * u)
        };
        for w in wrap_lines(kind, txt, size, max_w) {
            rows.push((kind, w, col, size));
        }
        if i == 0 && lines.len() > 1 {
            rows.push((FontKind::Body, String::new(), TEXT, 6.0 * u));
        }
    }
    let w = rows.iter().map(|(k, t, _, s)| measure_with(*k, t, *s).width).fold(0.0, f32::max) + pad * 2.0;
    let h: f32 = rows.iter().map(|(_, _, _, s)| s * 1.25).sum::<f32>() + pad * 1.5;
    let (mx, my) = mouse_position();
    let x = (mx + 20.0 * u).min(screen_width() - w - 6.0);
    let y = (my + 20.0 * u).min(screen_height() - h - 6.0);
    let r = Rect::new(x, y, w, h);
    draw_rectangle(r.x + 4.0, r.y + 6.0, r.w, r.h, Color::new(0.0, 0.0, 0.0, 0.35));
    draw_rectangle(r.x, r.y, r.w, r.h, Color::new(0.045, 0.034, 0.025, 0.97));
    theme::metal_border(r, 1.5 * u);
    let mut cy = r.y + pad * 0.8;
    for (k, t, c, s) in rows {
        cy += s * 1.25;
        if !t.is_empty() {
            text_with(k, &t, r.x + pad, cy - s * 0.25, s, c);
        }
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
    theme::surface(r, Surface::Recess, u);
    let h = ease(key_of(r), focused, 0.1);
    let c = Color::new(GOLD_DIM.r + (GOLD_BRIGHT.r - GOLD_DIM.r) * h, GOLD_DIM.g + (GOLD_BRIGHT.g - GOLD_DIM.g) * h, GOLD_DIM.b + (GOLD_BRIGHT.b - GOLD_DIM.b) * h, 1.0);
    draw_rectangle_lines(r.x, r.y, r.w, r.h, 1.0 + h, c);
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
        paste_into(value, max, &filter);
        enter = is_key_pressed(KeyCode::Enter);
    }
    let shown = if mask { "•".repeat(value.chars().count()) } else { value.clone() };
    let size = (r.h * 0.5).clamp(14.0 * u, 20.0 * u);
    let base_y = r.y + r.h / 2.0 + size * 0.35;
    text_fit(&shown, r.x + 10.0 * u, base_y, size, r.w - 20.0 * u, TEXT);
    if focused && (get_time() * 1.6) as i32 % 2 == 0 {
        let cx = r.x + 10.0 * u + measure(&shown, size).width.min(r.w - 20.0 * u) + 2.0;
        draw_line(cx, r.y + r.h * 0.22, cx, r.bottom() - r.h * 0.22, 1.5, GOLD_BRIGHT);
    }
    if value.is_empty() && !focused {
        // nothing: placeholders are drawn by the caller with `placeholder`
    }
    enter
}

/// For any focused text field: Ctrl+V (or Shift+Insert) pastes the clipboard, keeping the
/// field's character filter and length limit (a trailing newline is dropped).
pub fn paste_into(value: &mut String, max: usize, filter: &dyn Fn(char) -> bool) {
    let ctrl = is_key_down(KeyCode::LeftControl) || is_key_down(KeyCode::RightControl);
    let shift = is_key_down(KeyCode::LeftShift) || is_key_down(KeyCode::RightShift);
    if !((ctrl && is_key_pressed(KeyCode::V)) || (shift && is_key_pressed(KeyCode::Insert))) {
        return;
    }
    let Some(clip) = macroquad::miniquad::window::clipboard_get() else { return };
    for ch in clip.trim_end_matches(['\r', '\n']).chars() {
        if value.chars().count() >= max {
            break;
        }
        if !ch.is_control() && filter(ch) {
            value.push(ch);
        }
    }
}

/// Faint hint text inside an empty field.
pub fn placeholder(r: Rect, hint: &str, u: f32) {
    let size = (r.h * 0.5).clamp(14.0 * u, 20.0 * u);
    text_with(FontKind::NarrativeItalic, hint, r.x + 10.0 * u, r.y + r.h / 2.0 + size * 0.35, size, TEXT_FAINT);
}

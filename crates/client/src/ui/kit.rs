//! Pieces shared by the "second wave" windows (spells, character, guild, party, quests, world
//! map, options, crafting, GM, events, auction): vector icons, parchment pages, list rows,
//! tabs, toggles, sliders, seals and the scaled hero portrait. Everything follows
//! docs/ESTILO_UI.md: no boxes, no 1 px outlines everywhere, three levels of hierarchy.
//!
//! This file lives next to `character.rs` (declared there with `#[path]`) so the rest of the
//! UI tree stays untouched; the other windows reach it as `crate::ui::character::kit`.
#![allow(dead_code)] // toolkit: not every piece is used by every window
use crate::ui::fx_ui::{ease, key_of};
use crate::ui::theme::{self, FontKind, Surface, GOLD, GOLD_BRIGHT, GOLD_DIM, GOLD_TEXT, GOOD, TEXT_DIM, TEXT_FAINT};
use crate::ui::widgets::{measure_with, text_with, wrap_lines};
use crate::ui::{Ui, WindowId};
use macroquad::prelude::*;
use std::cell::Cell;

/// Secondary ink (labels on parchment).
pub const INK_DIM: Color = Color::new(0.43, 0.32, 0.20, 1.0);
/// Deep red of seals and wax.
pub const WAX: Color = Color::new(0.50, 0.09, 0.07, 1.0);
pub const ARCANE: Color = Color::new(0.45, 0.62, 1.0, 1.0);

thread_local! {
    /// Whether the window being drawn is the one under the mouse (controls ignore the mouse
    /// otherwise, so a window behind another does not react through it).
    static ACTIVE: Cell<bool> = const { Cell::new(true) };
    /// Slider being dragged (id of the control).
    static DRAG: Cell<u32> = const { Cell::new(0) };
}

/// Call first in a window's `draw`: tells the controls whether this window takes the mouse.
pub fn begin(ui: &Ui, w: WindowId) {
    let on = ui.topmost_at(mouse_position().into()).is_none_or(|t| t == w) && ui.drag.is_none();
    ACTIVE.set(on);
}

pub fn active() -> bool {
    ACTIVE.get()
}

pub fn hover(r: Rect) -> bool {
    active() && r.contains(mouse_position().into())
}

pub fn clicked(r: Rect) -> bool {
    hover(r) && is_mouse_button_pressed(MouseButton::Left)
}

pub fn alpha(c: Color, a: f32) -> Color {
    Color::new(c.r, c.g, c.b, c.a * a)
}

pub fn mix(a: Color, b: Color, t: f32) -> Color {
    Color::new(a.r + (b.r - a.r) * t, a.g + (b.g - a.g) * t, a.b + (b.b - a.b) * t, a.a + (b.a - a.a) * t)
}

/// 1234567 -> "1.234.567".
pub fn thousands(n: u64) -> String {
    let s = n.to_string();
    let mut out = String::new();
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (s.len() - i).is_multiple_of(3) {
            out.push('.');
        }
        out.push(c);
    }
    out
}

// ---------------------------------------------------------------- text

/// Text right-aligned to `right` (baseline `y`).
pub fn text_right(kind: FontKind, s: &str, right: f32, y: f32, size: f32, c: Color) {
    let w = measure_with(kind, s, size).width;
    text_with(kind, s, right - w, y, size, c);
}

/// Text centred on `cx`.
pub fn text_mid(kind: FontKind, s: &str, cx: f32, y: f32, size: f32, c: Color) {
    let w = measure_with(kind, s, size).width;
    text_with(kind, s, cx - w / 2.0, y, size, c);
}

/// Text cut with an ellipsis to fit `max_w`, in any font.
pub fn fit(kind: FontKind, s: &str, x: f32, y: f32, size: f32, max_w: f32, c: Color) {
    if measure_with(kind, s, size).width <= max_w {
        return text_with(kind, s, x, y, size, c);
    }
    let mut t = s.to_string();
    while !t.is_empty() && measure_with(kind, &format!("{t}…"), size).width > max_w {
        t.pop();
    }
    text_with(kind, &format!("{t}…"), x, y, size, c);
}

/// Text centred on `cx` that shrinks a little, then cuts, to fit `max_w`.
pub fn fit_centered(kind: FontKind, s: &str, cx: f32, y: f32, size: f32, max_w: f32, c: Color) {
    let mut sz = size;
    while sz > size * 0.7 && measure_with(kind, s, sz).width > max_w {
        sz -= 1.0;
    }
    let w = measure_with(kind, s, sz).width.min(max_w);
    fit(kind, s, cx - w / 2.0, y, sz, max_w, c);
}

/// Wrapped paragraph from `top` (the first baseline is `top + size`); returns the y under it.
/// `shadow = false` is for text on parchment (the soft shadow would smear the ink).
#[allow(clippy::too_many_arguments)]
pub fn paragraph(kind: FontKind, s: &str, x: f32, top: f32, w: f32, size: f32, c: Color, max_rows: usize) -> f32 {
    let rows = wrap_lines(kind, s, size, w);
    let cut = rows.len() > max_rows;
    let lh = size * 1.32;
    for (i, row) in rows.iter().take(max_rows).enumerate() {
        let shown = if cut && i + 1 == max_rows { format!("{}…", row.trim_end_matches('.')) } else { row.clone() };
        ink_or_shadow(kind, &shown, x, top + size + i as f32 * lh, size, c);
    }
    top + rows.len().min(max_rows) as f32 * lh + size * 0.2
}

/// Draws `s`; dark colours (ink) go without the drop shadow so they stay crisp.
fn ink_or_shadow(kind: FontKind, s: &str, x: f32, y: f32, size: f32, c: Color) {
    if c.r + c.g + c.b < 1.2 {
        let f = theme::font(kind);
        let (base, scale) = crate::ui::widgets::glyph(size);
        draw_text_ex(s, x.round(), y.round(), TextParams { font: f.as_ref(), font_size: base, font_scale: scale, color: c, ..Default::default() });
    } else {
        text_with(kind, s, x, y, size, c);
    }
}

/// Text in ink on parchment (no shadow).
pub fn ink(kind: FontKind, s: &str, x: f32, y: f32, size: f32, c: Color) {
    ink_or_shadow(kind, s, x, y, size, c);
}

/// Ink text centred on `cx`, shrunk (in steps) when wider than `max_w`.
pub fn ink_mid(kind: FontKind, s: &str, cx: f32, y: f32, size: f32, max_w: f32, c: Color) {
    let mut sz = size;
    while sz > size * 0.6 && measure_with(kind, s, sz).width > max_w {
        sz -= 1.0;
    }
    let w = measure_with(kind, s, sz).width.min(max_w);
    ink_fit(kind, s, cx - w / 2.0, y, sz, max_w, c);
}

/// Ink text cut to fit.
pub fn ink_fit(kind: FontKind, s: &str, x: f32, y: f32, size: f32, max_w: f32, c: Color) {
    if measure_with(kind, s, size).width <= max_w {
        return ink(kind, s, x, y, size, c);
    }
    let mut t = s.to_string();
    while !t.is_empty() && measure_with(kind, &format!("{t}…"), size).width > max_w {
        t.pop();
    }
    ink(kind, &format!("{t}…"), x, y, size, c);
}

// ---------------------------------------------------------------- surfaces

thread_local! {
    static PAPER: std::cell::RefCell<Option<Texture2D>> = const { std::cell::RefCell::new(None) };
}

fn lattice(x: i32, y: i32, period: i32, seed: u32) -> f32 {
    let (x, y) = (x.rem_euclid(period), y.rem_euclid(period));
    let mut h = (x as u32).wrapping_mul(374_761_393) ^ (y as u32).wrapping_mul(668_265_263) ^ seed.wrapping_mul(2_246_822_519);
    h = (h ^ (h >> 13)).wrapping_mul(1_274_126_177);
    ((h ^ (h >> 16)) & 0xffff) as f32 / 65535.0
}

/// Value noise that repeats every `period` lattice cells, so the texture tiles without seams.
fn periodic(x: f32, y: f32, period: i32, seed: u32) -> f32 {
    let (xi, yi) = (x.floor() as i32, y.floor() as i32);
    let (fx, fy) = (x - xi as f32, y - yi as f32);
    let s = |t: f32| t * t * (3.0 - 2.0 * t);
    let (a, b, c, d) = (lattice(xi, yi, period, seed), lattice(xi + 1, yi, period, seed), lattice(xi, yi + 1, period, seed), lattice(xi + 1, yi + 1, period, seed));
    let (u, v) = (s(fx), s(fy));
    a + (b - a) * u + (c - a) * v + (a - b - c + d) * u * v
}

/// A seamless 256 px parchment: warm blotches, a few stains and faint fibres.
fn paper_texture() -> Texture2D {
    PAPER.with(|p| {
        if let Some(t) = p.borrow().as_ref() {
            return t.clone();
        }
        const N: u16 = 256;
        let base = Color::new(0.85, 0.77, 0.60, 1.0);
        let mut img = Image::gen_image_color(N, N, base);
        for y in 0..N as u32 {
            for x in 0..N as u32 {
                let (fx, fy) = (x as f32 / 256.0, y as f32 / 256.0);
                let big = periodic(fx * 4.0, fy * 4.0, 4, 3) * 0.55 + periodic(fx * 8.0, fy * 8.0, 8, 5) * 0.30 + periodic(fx * 16.0, fy * 16.0, 16, 7) * 0.15 - 0.5;
                let fibre = periodic(fx * 128.0, fy * 24.0, 128, 11) - 0.5;
                let speck = periodic(fx * 64.0, fy * 64.0, 64, 13);
                let stain = ((periodic(fx * 2.0, fy * 2.0, 2, 17) - 0.55) * 2.0).max(0.0);
                let k = 1.0 + big * 0.20 + fibre * 0.035 - if speck > 0.93 { 0.05 } else { 0.0 } - stain * 0.08;
                img.set_pixel(x, y, Color::new((base.r * k).clamp(0.0, 1.0), (base.g * k * 0.99).clamp(0.0, 1.0), (base.b * k * 0.96).clamp(0.0, 1.0), 1.0));
            }
        }
        let t = Texture2D::from_image(&img);
        t.set_filter(FilterMode::Linear);
        *p.borrow_mut() = Some(t.clone());
        t
    })
}

/// A page of parchment with a soft shadow and a thin ink edge (descriptions, quests, news).
pub fn page(r: Rect, u: f32) {
    for i in 1..=4 {
        let o = i as f32 * 1.5 * u;
        draw_rectangle(r.x + o * 0.4, r.y + o, r.w, r.h, Color::new(0.0, 0.0, 0.0, 0.10));
    }
    let tex = paper_texture();
    let ts = 256.0 * u.max(1.0);
    let mut y = r.y;
    while y < r.bottom() {
        let h = (r.bottom() - y).min(ts);
        let mut x = r.x;
        while x < r.right() {
            let w = (r.right() - x).min(ts);
            draw_texture_ex(&tex, x, y, WHITE, DrawTextureParams { dest_size: Some(vec2(w, h)), source: Some(Rect::new(0.0, 0.0, w / u.max(1.0), h / u.max(1.0))), ..Default::default() });
            x += ts;
        }
        y += ts;
    }
    theme::inner_vignette(r, 20.0 * u, 0.38);
    let _ = Surface::Parchment;
    draw_rectangle_lines(r.x, r.y, r.w, r.h, 1.5 * u, Color::new(0.26, 0.17, 0.08, 0.85));
    draw_rectangle_lines(r.x + 4.0 * u, r.y + 4.0 * u, r.w - 8.0 * u, r.h - 8.0 * u, 1.0, Color::new(0.35, 0.24, 0.12, 0.32));
}

/// Recessed area (lists, wells).
pub fn well(r: Rect, u: f32) {
    theme::surface(r, Surface::Recess, u);
}

/// A flourish line: fades out to both sides.
pub fn rule(x: f32, y: f32, w: f32, c: Color) {
    let n = 14;
    for i in 0..n {
        let t0 = i as f32 / n as f32;
        let t1 = (i + 1) as f32 / n as f32;
        let a0 = 1.0 - (t0 * 2.0 - 1.0).abs().powf(1.6);
        draw_line(x + w * t0, y, x + w * t1, y, 1.0, alpha(c, a0.max(0.05)));
    }
}

/// A list row: no box. Hover tints, selected gets a warm wash and a gold bar on the left.
/// Returns (hovered, clicked).
pub fn row(r: Rect, selected: bool, u: f32) -> (bool, bool) {
    let hov = hover(r);
    let h = ease(key_of(r), hov || selected, 0.08);
    if h > 0.0 {
        let k = if selected { 1.0 } else { 0.55 };
        let n = 10;
        for i in 0..n {
            let t = i as f32 / n as f32;
            draw_rectangle(r.x + r.w * t, r.y, r.w / n as f32 + 1.0, r.h, Color::new(0.95, 0.72, 0.35, 0.16 * h * k * (1.0 - t * 0.85)));
        }
    }
    if selected {
        draw_rectangle(r.x, r.y + 2.0 * u, 3.0 * u, r.h - 4.0 * u, GOLD_BRIGHT);
    }
    (hov, hov && is_mouse_button_pressed(MouseButton::Left))
}

/// Row on parchment: darker wash instead of the golden one.
pub fn row_ink(r: Rect, selected: bool, u: f32) -> (bool, bool) {
    let hov = hover(r);
    let h = ease(key_of(r), hov || selected, 0.08);
    if h > 0.0 {
        let n = 10;
        for i in 0..n {
            let t = i as f32 / n as f32;
            draw_rectangle(r.x + r.w * t, r.y, r.w / n as f32 + 1.0, r.h, Color::new(0.35, 0.2, 0.06, (if selected { 0.30 } else { 0.16 }) * h * (1.0 - t * 0.8)));
        }
    }
    if selected {
        draw_rectangle(r.x, r.y + 2.0 * u, 3.0 * u, r.h - 4.0 * u, WAX);
    }
    (hov, hov && is_mouse_button_pressed(MouseButton::Left))
}

/// Slim scroll bar at the right edge of `area` when the list overflows.
pub fn scrollbar(area: Rect, total: usize, vis: usize, off: usize, u: f32) {
    if total <= vis {
        return;
    }
    let tr = Rect::new(area.right() - 4.0 * u, area.y + 2.0, 3.0 * u, area.h - 4.0);
    draw_rectangle(tr.x, tr.y, tr.w, tr.h, Color::new(0.0, 0.0, 0.0, 0.35));
    let h = (tr.h * vis as f32 / total as f32).max(16.0 * u);
    let y = tr.y + (tr.h - h) * off as f32 / (total - vis).max(1) as f32;
    draw_rectangle(tr.x, y, tr.w, h, GOLD);
}

/// Mouse wheel over `area` moves `off` by `step` rows; clamps to the list.
pub fn wheel(area: Rect, off: usize, total: usize, vis: usize, step: usize) -> usize {
    let mut o = off;
    if hover(area) {
        let w = mouse_wheel().1;
        if w > 0.0 {
            o = o.saturating_sub(step);
        } else if w < 0.0 {
            o += step;
        }
    }
    o.min(total.saturating_sub(vis))
}

// ---------------------------------------------------------------- tabs, buttons

/// Tab strip with no boxes: Title labels, the active one in bright gold with a lit
/// underline. Returns the clicked index. `y` is the top; returns the height used via `h`.
pub fn tabs(x: f32, y: f32, labels: &[String], active_i: usize, u: f32) -> Option<usize> {
    let size = 15.0 * u;
    let mut cx = x;
    let mut hit = None;
    for (i, l) in labels.iter().enumerate() {
        let up = l.to_uppercase();
        let w = measure_with(FontKind::Title, &up, size).width;
        let r = Rect::new(cx - 8.0 * u, y, w + 16.0 * u, 26.0 * u);
        let on = i == active_i;
        let h = ease(key_of(r), hover(r) || on, 0.08);
        let c = mix(TEXT_DIM, GOLD_BRIGHT, if on { 1.0 } else { h * 0.8 });
        text_with(FontKind::Title, &up, cx, y + 17.0 * u, size, c);
        if on || h > 0.0 {
            let a = if on { 1.0 } else { h * 0.7 };
            draw_rectangle(cx - 2.0 * u, y + 24.0 * u, (w + 4.0 * u) * if on { 1.0 } else { h }, 2.0 * u, alpha(GOLD_BRIGHT, a));
            if on {
                draw_rectangle(cx - 6.0 * u, y + 22.0 * u, w + 12.0 * u, 6.0 * u, Color::new(1.0, 0.8, 0.4, 0.07));
            }
        }
        if clicked(r) {
            hit = Some(i);
        }
        cx += w + 24.0 * u;
    }
    hit
}

/// A pill switch; returns the new value.
pub fn toggle(r: Rect, on: bool, u: f32) -> bool {
    let h = ease(key_of(r), on, 0.12);
    let hv = hover(r);
    let track = Rect::new(r.x, r.y, r.w, r.h);
    let rad = track.h / 2.0;
    let off_c = Color::new(0.10, 0.075, 0.05, 1.0);
    let on_c = Color::new(0.55, 0.38, 0.12, 1.0);
    let c = mix(off_c, on_c, h);
    draw_circle(track.x + rad, track.y + rad, rad, c);
    draw_circle(track.right() - rad, track.y + rad, rad, c);
    draw_rectangle(track.x + rad, track.y, track.w - 2.0 * rad, track.h, c);
    let edge = mix(GOLD_DIM, GOLD, h * 0.8 + if hv { 0.2 } else { 0.0 });
    draw_circle_lines(track.x + rad, track.y + rad, rad, 1.2, edge);
    draw_circle_lines(track.right() - rad, track.y + rad, rad, 1.2, edge);
    draw_line(track.x + rad, track.y, track.right() - rad, track.y, 1.2, edge);
    draw_line(track.x + rad, track.bottom(), track.right() - rad, track.bottom(), 1.2, edge);
    let kx = track.x + rad + (track.w - 2.0 * rad) * h;
    draw_circle(kx + 1.0, track.y + rad + 1.5, rad * 0.78, Color::new(0.0, 0.0, 0.0, 0.4));
    draw_circle(kx, track.y + rad, rad * 0.78, mix(Color::new(0.62, 0.55, 0.45, 1.0), GOLD_BRIGHT, h));
    draw_circle(kx - rad * 0.2, track.y + rad - rad * 0.25, rad * 0.3, Color::new(1.0, 1.0, 1.0, 0.25));
    let _ = u;
    if clicked(r) {
        !on
    } else {
        on
    }
}

/// A slider with a filled track and a round knob; `id` must be unique. Returns the value.
pub fn slider(r: Rect, v: f32, id: u32, enabled: bool, u: f32) -> f32 {
    let track = Rect::new(r.x, r.y + r.h / 2.0 - 3.0 * u, r.w, 6.0 * u);
    let m: Vec2 = mouse_position().into();
    if hover(r) && is_mouse_button_pressed(MouseButton::Left) {
        DRAG.set(id);
    }
    let mut v = v.clamp(0.0, 1.0);
    let dragging = DRAG.get() == id;
    if dragging {
        if is_mouse_button_down(MouseButton::Left) {
            v = ((m.x - r.x) / r.w).clamp(0.0, 1.0);
        } else {
            DRAG.set(0);
        }
    }
    let on = if enabled { Color::new(0.80, 0.58, 0.20, 1.0) } else { Color::new(0.36, 0.31, 0.24, 1.0) };
    draw_rectangle(track.x, track.y, track.w, track.h, Color::new(0.0, 0.0, 0.0, 0.65));
    draw_rectangle(track.x, track.y, track.w * v, track.h, on);
    draw_rectangle(track.x, track.y, track.w * v, track.h * 0.4, Color::new(1.0, 1.0, 1.0, 0.18));
    draw_rectangle_lines(track.x, track.y, track.w, track.h, 1.0, GOLD_DIM);
    // Tick marks every 25 %.
    for i in 1..4 {
        let tx = track.x + track.w * i as f32 / 4.0;
        draw_line(tx, track.bottom() + 2.0 * u, tx, track.bottom() + 5.0 * u, 1.0, alpha(GOLD_DIM, 0.7));
    }
    let h = ease(key_of(r), hover(r) || dragging, 0.1);
    let kx = r.x + r.w * v;
    let kr = (8.0 + 1.5 * h) * u;
    draw_circle(kx + 1.0, r.center().y + 1.5, kr, Color::new(0.0, 0.0, 0.0, 0.45));
    draw_circle(kx, r.center().y, kr, if enabled { mix(GOLD, GOLD_BRIGHT, h) } else { TEXT_FAINT });
    draw_circle(kx, r.center().y, kr * 0.55, Color::new(0.22, 0.14, 0.05, 1.0));
    draw_circle_lines(kx, r.center().y, kr, 1.2, Color::new(0.0, 0.0, 0.0, 0.7));
    v
}

/// Whether a slider is being dragged right now.
pub fn dragging() -> bool {
    DRAG.get() != 0
}

/// A "+" style round button (small, glowing when `glow`). Returns clicked.
pub fn round_btn(c: Vec2, rad: f32, label: &str, enabled: bool, glow: bool, u: f32) -> bool {
    let r = Rect::new(c.x - rad, c.y - rad, rad * 2.0, rad * 2.0);
    let hv = enabled && hover(r);
    let h = ease(key_of(r), hv, 0.08);
    let pulse = if glow && enabled { 0.5 + 0.5 * (get_time() * 3.2).sin() as f32 } else { 0.0 };
    if enabled {
        draw_circle(c.x, c.y, rad + 3.0 * u + 2.0 * u * pulse, Color::new(0.5, 0.9, 0.4, 0.10 + 0.12 * pulse + 0.1 * h));
    }
    let base = if enabled { mix(Color::new(0.20, 0.38, 0.14, 1.0), Color::new(0.34, 0.60, 0.22, 1.0), h) } else { Color::new(0.12, 0.10, 0.08, 1.0) };
    draw_circle(c.x, c.y + if hv && is_mouse_button_down(MouseButton::Left) { 1.0 } else { 0.0 }, rad, base);
    draw_circle_lines(c.x, c.y, rad, 1.4, if enabled { GOOD } else { GOLD_DIM });
    let size = rad * 1.5;
    let w = measure_with(FontKind::Body, label, size).width;
    text_with(FontKind::Body, label, c.x - w / 2.0, c.y + size * 0.32, size, if enabled { Color::new(0.95, 1.0, 0.9, 1.0) } else { TEXT_FAINT });
    hv && is_mouse_button_pressed(MouseButton::Left)
}

// ---------------------------------------------------------------- icons

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Icon {
    Heart,
    Drop,
    Bolt,
    Coin,
    Hourglass,
    Target,
    Sword,
    Shield,
    Crown,
    Check,
    Cross,
    Star,
    Scroll,
    Skull,
    Feather,
    Book,
    Flame,
    Leaf,
    Eye,
    Hammer,
    Banner,
    Key,
}

/// Draws icon `i` centred on (`cx`, `cy`) in a box of `s` pixels.
pub fn icon(i: Icon, cx: f32, cy: f32, s: f32, c: Color) {
    let k = s / 2.0;
    let lw = (s / 9.0).max(1.4);
    let dark = Color::new(0.0, 0.0, 0.0, 0.35 * c.a);
    // soft base shadow
    let p = |x: f32, y: f32| vec2(cx + x * k, cy + y * k);
    match i {
        Icon::Heart => {
            draw_circle(cx - 0.45 * k, cy - 0.25 * k, 0.55 * k, c);
            draw_circle(cx + 0.45 * k, cy - 0.25 * k, 0.55 * k, c);
            draw_triangle(p(-0.97, -0.05), p(0.97, -0.05), p(0.0, 0.95), c);
            draw_circle(cx - 0.55 * k, cy - 0.4 * k, 0.16 * k, Color::new(1.0, 1.0, 1.0, 0.3 * c.a));
        }
        Icon::Drop => {
            draw_triangle(p(0.0, -1.0), p(-0.62, 0.15), p(0.62, 0.15), c);
            draw_circle(cx, cy + 0.3 * k, 0.66 * k, c);
            draw_circle(cx - 0.22 * k, cy + 0.1 * k, 0.16 * k, Color::new(1.0, 1.0, 1.0, 0.35 * c.a));
        }
        Icon::Bolt => {
            draw_triangle(p(0.2, -1.0), p(-0.65, 0.12), p(0.05, 0.12), c);
            draw_triangle(p(-0.2, 1.0), p(0.65, -0.12), p(-0.05, -0.12), c);
        }
        Icon::Coin => {
            draw_circle(cx, cy, k * 0.95, dark);
            draw_circle(cx, cy, k * 0.9, c);
            draw_circle_lines(cx, cy, k * 0.62, lw * 0.8, Color::new(0.35, 0.22, 0.05, 0.8 * c.a));
            draw_circle(cx - 0.3 * k, cy - 0.3 * k, 0.18 * k, Color::new(1.0, 1.0, 1.0, 0.35 * c.a));
        }
        Icon::Hourglass => {
            draw_triangle(p(-0.65, -0.85), p(0.65, -0.85), p(0.0, 0.0), c);
            draw_triangle(p(-0.65, 0.85), p(0.65, 0.85), p(0.0, 0.0), c);
            draw_line(cx - 0.8 * k, cy - 0.9 * k, cx + 0.8 * k, cy - 0.9 * k, lw * 1.2, c);
            draw_line(cx - 0.8 * k, cy + 0.9 * k, cx + 0.8 * k, cy + 0.9 * k, lw * 1.2, c);
        }
        Icon::Target => {
            draw_circle_lines(cx, cy, k * 0.88, lw, c);
            draw_circle_lines(cx, cy, k * 0.5, lw, c);
            draw_circle(cx, cy, k * 0.18, c);
        }
        Icon::Sword => {
            draw_line(cx + 0.7 * k, cy - 0.7 * k, cx - 0.35 * k, cy + 0.35 * k, lw * 1.5, c);
            draw_line(cx - 0.65 * k, cy + 0.05 * k, cx - 0.05 * k, cy + 0.65 * k, lw * 1.2, c);
            draw_line(cx - 0.35 * k, cy + 0.35 * k, cx - 0.8 * k, cy + 0.8 * k, lw * 1.2, c);
        }
        Icon::Shield => {
            draw_rectangle(cx - 0.7 * k, cy - 0.85 * k, 1.4 * k, 1.0 * k, c);
            draw_triangle(p(-0.7, 0.15), p(0.7, 0.15), p(0.0, 0.95), c);
            draw_line(cx, cy - 0.7 * k, cx, cy + 0.6 * k, lw * 0.8, Color::new(0.0, 0.0, 0.0, 0.3 * c.a));
        }
        Icon::Crown => {
            draw_rectangle(cx - 0.8 * k, cy + 0.2 * k, 1.6 * k, 0.55 * k, c);
            draw_triangle(p(-0.8, 0.2), p(-0.8, -0.75), p(-0.2, 0.2), c);
            draw_triangle(p(-0.4, 0.2), p(0.0, -0.95), p(0.4, 0.2), c);
            draw_triangle(p(0.2, 0.2), p(0.8, -0.75), p(0.8, 0.2), c);
        }
        Icon::Check => {
            draw_line(cx - 0.75 * k, cy + 0.05 * k, cx - 0.2 * k, cy + 0.6 * k, lw * 1.5, c);
            draw_line(cx - 0.2 * k, cy + 0.6 * k, cx + 0.8 * k, cy - 0.6 * k, lw * 1.5, c);
        }
        Icon::Cross => {
            draw_line(cx - 0.6 * k, cy - 0.6 * k, cx + 0.6 * k, cy + 0.6 * k, lw * 1.5, c);
            draw_line(cx + 0.6 * k, cy - 0.6 * k, cx - 0.6 * k, cy + 0.6 * k, lw * 1.5, c);
        }
        Icon::Star => {
            let mut pts = [vec2(0.0, 0.0); 10];
            for (n, pt) in pts.iter_mut().enumerate() {
                let a = -std::f32::consts::FRAC_PI_2 + n as f32 * std::f32::consts::PI / 5.0;
                let rad = if n % 2 == 0 { 1.0 } else { 0.42 };
                *pt = vec2(cx + a.cos() * rad * k, cy + a.sin() * rad * k);
            }
            for n in 0..10 {
                draw_triangle(vec2(cx, cy), pts[n], pts[(n + 1) % 10], c);
            }
        }
        Icon::Scroll => {
            draw_rectangle(cx - 0.6 * k, cy - 0.8 * k, 1.2 * k, 1.6 * k, c);
            draw_circle(cx - 0.6 * k, cy - 0.8 * k, 0.2 * k, c);
            draw_circle(cx + 0.6 * k, cy + 0.8 * k, 0.2 * k, c);
            for n in 0..3 {
                let y = cy - 0.35 * k + n as f32 * 0.4 * k;
                draw_line(cx - 0.35 * k, y, cx + 0.35 * k, y, lw * 0.8, Color::new(0.0, 0.0, 0.0, 0.4 * c.a));
            }
        }
        Icon::Skull => {
            draw_circle(cx, cy - 0.15 * k, 0.8 * k, c);
            draw_rectangle(cx - 0.4 * k, cy + 0.3 * k, 0.8 * k, 0.6 * k, c);
            draw_circle(cx - 0.3 * k, cy - 0.1 * k, 0.2 * k, Color::new(0.0, 0.0, 0.0, 0.8 * c.a));
            draw_circle(cx + 0.3 * k, cy - 0.1 * k, 0.2 * k, Color::new(0.0, 0.0, 0.0, 0.8 * c.a));
        }
        Icon::Feather => {
            draw_line(cx - 0.8 * k, cy + 0.8 * k, cx + 0.7 * k, cy - 0.8 * k, lw, c);
            for n in 0..4 {
                let t = 0.15 + n as f32 * 0.2;
                let bx = cx - 0.8 * k + 1.5 * k * t;
                let by = cy + 0.8 * k - 1.6 * k * t;
                draw_line(bx, by, bx + 0.5 * k, by + 0.15 * k, lw * 1.1, c);
                draw_line(bx, by, bx - 0.45 * k, by - 0.05 * k, lw * 1.1, c);
            }
        }
        Icon::Book => {
            draw_triangle(p(-0.95, -0.6), p(0.0, -0.4), p(0.0, 0.8), c);
            draw_triangle(p(-0.95, -0.6), p(0.0, 0.8), p(-0.95, 0.5), c);
            draw_triangle(p(0.95, -0.6), p(0.0, -0.4), p(0.0, 0.8), c);
            draw_triangle(p(0.95, -0.6), p(0.0, 0.8), p(0.95, 0.5), c);
            draw_line(cx, cy - 0.4 * k, cx, cy + 0.8 * k, lw * 0.8, Color::new(0.0, 0.0, 0.0, 0.4 * c.a));
        }
        Icon::Flame => {
            draw_circle(cx, cy + 0.35 * k, 0.62 * k, c);
            draw_triangle(p(0.15, -1.0), p(-0.58, 0.3), p(0.6, 0.25), c);
            draw_triangle(p(-0.55, -0.45), p(-0.62, 0.3), p(-0.05, 0.1), c);
            draw_circle(cx, cy + 0.48 * k, 0.3 * k, Color::new(1.0, 0.92, 0.55, 0.9 * c.a));
        }
        Icon::Leaf => {
            // A lens from the lower-left tip to the upper-right one, with a midrib.
            let (a, b) = (vec2(-0.8, 0.8), vec2(0.8, -0.8));
            let perp = vec2(0.5, 0.5);
            let n = 10;
            for i in 0..n {
                let (t0, t1) = (i as f32 / n as f32, (i + 1) as f32 / n as f32);
                let w0 = (std::f32::consts::PI * t0).sin() * 0.62;
                let w1 = (std::f32::consts::PI * t1).sin() * 0.62;
                let (m0, m1) = (a + (b - a) * t0, a + (b - a) * t1);
                let (u0, l0, u1, l1) = (m0 + perp * w0, m0 - perp * w0, m1 + perp * w1, m1 - perp * w1);
                draw_triangle(p(u0.x, u0.y), p(l0.x, l0.y), p(u1.x, u1.y), c);
                draw_triangle(p(l0.x, l0.y), p(l1.x, l1.y), p(u1.x, u1.y), c);
            }
            draw_line(cx - 0.95 * k, cy + 0.95 * k, cx + 0.55 * k, cy - 0.55 * k, lw * 0.9, Color::new(0.0, 0.0, 0.0, 0.35 * c.a));
        }
        Icon::Eye => {
            draw_circle(cx, cy, 0.45 * k, c);
            draw_triangle(p(-0.95, 0.0), p(0.0, -0.6), p(0.0, 0.6), c);
            draw_triangle(p(0.95, 0.0), p(0.0, -0.6), p(0.0, 0.6), c);
            draw_circle(cx, cy, 0.28 * k, Color::new(0.05, 0.04, 0.03, 0.9 * c.a));
        }
        Icon::Hammer => {
            draw_line(cx - 0.55 * k, cy + 0.8 * k, cx + 0.25 * k, cy - 0.35 * k, lw * 1.3, c);
            draw_rectangle(cx - 0.15 * k, cy - 0.85 * k, 1.0 * k, 0.55 * k, c);
        }
        Icon::Banner => {
            draw_rectangle(cx - 0.65 * k, cy - 0.9 * k, 1.3 * k, 1.35 * k, c);
            draw_triangle(p(-0.65, 0.45), p(0.65, 0.45), p(0.0, 0.95), c);
        }
        Icon::Key => {
            draw_circle_lines(cx - 0.4 * k, cy - 0.4 * k, 0.45 * k, lw * 1.3, c);
            draw_line(cx - 0.1 * k, cy - 0.1 * k, cx + 0.8 * k, cy + 0.8 * k, lw * 1.3, c);
            draw_line(cx + 0.45 * k, cy + 0.45 * k, cx + 0.7 * k, cy + 0.2 * k, lw * 1.3, c);
        }
    }
}

/// An icon on a small dark medallion with a gold ring (stat markers).
pub fn badge_icon(i: Icon, cx: f32, cy: f32, rad: f32, c: Color) {
    draw_circle(cx, cy, rad, Color::new(0.04, 0.03, 0.02, 0.9));
    draw_circle_lines(cx, cy, rad, 1.2, alpha(c, 0.7));
    icon(i, cx, cy, rad * 1.15, c);
}

/// A round seal (level, rank...): wax disc with a gold ring and a Title label.
pub fn seal(cx: f32, cy: f32, rad: f32, label: &str, caption: &str, u: f32) {
    for i in 1..=3 {
        draw_circle(cx + i as f32 * u * 0.6, cy + i as f32 * u * 1.2, rad, Color::new(0.0, 0.0, 0.0, 0.12));
    }
    draw_circle(cx, cy, rad, Color::new(0.34, 0.06, 0.05, 1.0));
    draw_circle(cx - rad * 0.12, cy - rad * 0.15, rad * 0.82, WAX);
    draw_circle_lines(cx, cy, rad, 2.0 * u, GOLD);
    draw_circle_lines(cx, cy, rad * 0.84, 1.0, alpha(GOLD_BRIGHT, 0.55));
    draw_circle(cx - rad * 0.4, cy - rad * 0.45, rad * 0.16, Color::new(1.0, 0.8, 0.7, 0.18));
    let size = if label.chars().count() > 2 { 22.0 * u } else { 28.0 * u };
    text_mid(FontKind::Title, label, cx, cy + size * 0.2, size, GOLD_BRIGHT);
    if !caption.is_empty() {
        text_mid(FontKind::Title, &caption.to_uppercase(), cx, cy - rad - 5.0 * u, 12.0 * u, GOLD_TEXT);
    }
}

/// A heraldic shield with a symbol (guild alignment, faction).
pub fn emblem(cx: f32, cy: f32, w: f32, c: Color, sym: Icon, u: f32) {
    let h = w * 1.18;
    let top = cy - h / 2.0;
    let l = cx - w / 2.0;
    let r = cx + w / 2.0;
    let mid = top + h * 0.58;
    let tip = vec2(cx, top + h);
    let dark = mix(c, Color::new(0.0, 0.0, 0.0, 1.0), 0.55);
    // shadow
    draw_rectangle(l + 2.0 * u, top + 3.0 * u, w, mid - top, Color::new(0.0, 0.0, 0.0, 0.25));
    // body: dark base, bright inner
    draw_rectangle(l, top, w, mid - top, dark);
    draw_triangle(vec2(l, mid), vec2(r, mid), tip, dark);
    let i = w * 0.08;
    draw_rectangle(l + i, top + i, w - 2.0 * i, mid - top - i, mix(c, Color::new(0.1, 0.07, 0.04, 1.0), 0.25));
    draw_triangle(vec2(l + i, mid), vec2(r - i, mid), vec2(cx, tip.y - i * 1.8), mix(c, Color::new(0.1, 0.07, 0.04, 1.0), 0.25));
    draw_rectangle(l + i, top + i, w - 2.0 * i, (mid - top) * 0.35, Color::new(1.0, 1.0, 1.0, 0.07));
    // gold edge
    let g = GOLD;
    draw_line(l, top, r, top, 2.0 * u, g);
    draw_line(l, top, l, mid, 2.0 * u, g);
    draw_line(r, top, r, mid, 2.0 * u, g);
    draw_line(l, mid, cx, tip.y, 2.0 * u, g);
    draw_line(r, mid, cx, tip.y, 2.0 * u, g);
    icon(sym, cx, cy - h * 0.04, w * 0.46, alpha(GOLD_BRIGHT, 0.95));
}

/// A thin bar (skills, progress); `c` is the fill.
pub fn thin_bar(r: Rect, v: f32, max: f32, c: Color) {
    draw_rectangle(r.x, r.y, r.w, r.h, Color::new(0.0, 0.0, 0.0, 0.55));
    let f = if max > 0.0 { (v / max).clamp(0.0, 1.0) } else { 0.0 };
    if f > 0.0 {
        draw_rectangle(r.x, r.y, r.w * f, r.h, c);
        draw_rectangle(r.x, r.y, r.w * f, r.h * 0.4, Color::new(1.0, 1.0, 1.0, 0.18));
    }
}

/// A checkbox-like progress mark for objectives: empty ring or ticked circle.
pub fn tick(cx: f32, cy: f32, rad: f32, done: bool, ink_on_parchment: bool) {
    if done {
        draw_circle(cx, cy, rad, if ink_on_parchment { Color::new(0.20, 0.45, 0.16, 0.9) } else { Color::new(0.2, 0.5, 0.15, 0.9) });
        icon(Icon::Check, cx, cy, rad * 1.3, Color::new(0.95, 1.0, 0.9, 1.0));
    } else {
        draw_circle_lines(cx, cy, rad, 1.6, if ink_on_parchment { INK_DIM } else { TEXT_DIM });
    }
}

// ---------------------------------------------------------------- hero portrait

/// Draws the character facing the player, scaled `s` times and centred on the feet at
/// (`fx`, `fy`): body, head, helmet, weapon and shield from the real graphics. Does nothing
/// if the body is unknown.
#[allow(clippy::too_many_arguments)]
pub fn hero(assets: &mut crate::assets::Assets, body_id: u16, head_id: u16, equip: (u16, u16, u16), fx: f32, fy: f32, s: f32, tint: Color) {
    use ao_formats::graphics::{anchor, Grh};
    use client_core::p3::fx::Anim;
    let heading = 3usize;
    let body_id = match body_id {
        581 if !assets.db.bodies.contains_key(&581) => 249,
        582 if !assets.db.bodies.contains_key(&582) => 248,
        b => b,
    };
    let Some(body) = assets.db.bodies.get(&body_id).cloned() else { return };
    let head = assets.db.heads.get(&head_id).map(|h| h[heading]).unwrap_or(0);
    // The virtual tile: its top-left sits so the feet (bottom-centre of the tile) land on (fx, fy).
    let (tx, ty) = (fx - 16.0 * s, fy - 32.0 * s);
    let b = vec2(body.body_offset.0, body.body_offset.1);
    let h = vec2(b.x + body.head_offset.0, b.y + body.head_offset.1 - 1.0);
    // Contact shadow.
    draw_ellipse(fx, fy - 2.0 * s, 13.0 * s, 4.5 * s, 0.0, Color::new(0.0, 0.0, 0.0, 0.4));
    let put = |assets: &mut crate::assets::Assets, g: Grh, at: Vec2| {
        let (ax, ay) = anchor(&g, at.x, at.y, true);
        if let Some(tex) = assets.texture(g.file) {
            draw_texture_ex(&tex, (tx + ax * s).round(), (ty + ay * s).round(), tint, DrawTextureParams { source: Some(Rect::new(g.sx, g.sy, g.w, g.h)), dest_size: Some(vec2(g.w * s, g.h * s)), ..Default::default() });
        }
    };
    let eq = crate::render::characters::FRAME.with(|f| f.borrow().equip.clone());
    let anim_of = |sel: u16, pick: fn(&client_core::p3::fx::EquipDb) -> &std::collections::HashMap<u16, client_core::p3::fx::EquipAnim>| -> Anim {
        if sel == 0 {
            return Anim::None;
        }
        eq.as_deref().and_then(|d| pick(d).get(&sel)).map(|a| a[heading]).unwrap_or(Anim::None)
    };
    let place_anim = |assets: &mut crate::assets::Assets, a: Anim, at: Vec2| match a {
        Anim::None => {}
        Anim::Grh(g) => {
            if let Some(g) = assets.db.frame(g, 0.0).cloned() {
                put(assets, g, at);
            }
        }
        Anim::Strip { file, x, y, w, h, .. } => put(assets, Grh { frames: vec![], file, sx: x, sy: y, w, h, speed: 0.0 }, at),
    };
    for layer in client_core::p3::fx::draw_order(heading) {
        use client_core::p3::fx::Layer;
        match layer {
            Layer::Body => {
                if let Some(g) = assets.db.frame(body.walk[heading], 0.0).cloned() {
                    put(assets, g, b);
                }
            }
            Layer::Head if head > 0 => {
                if let Some(g) = assets.db.frame(head, 0.0).cloned() {
                    put(assets, g, h);
                }
            }
            Layer::Helmet => place_anim(assets, anim_of(equip.2, |d| &d.helmets), h),
            Layer::Weapon => place_anim(assets, anim_of(equip.0, |d| &d.weapons), b),
            Layer::Shield => place_anim(assets, anim_of(equip.1, |d| &d.shields), b),
            _ => {}
        }
    }
}

thread_local! {
    /// Bounding box of the non-transparent pixels of a sprite, by (file, sx, sy).
    static BOUNDS: std::cell::RefCell<std::collections::HashMap<(u32, i32, i32), Rect>> = std::cell::RefCell::new(std::collections::HashMap::new());
}

/// Source rectangle (in the texture) around the drawn pixels of `g`.
fn opaque_bounds(tex: &Texture2D, g: &ao_formats::graphics::Grh) -> Rect {
    let key = (g.file, g.sx as i32, g.sy as i32);
    if let Some(r) = BOUNDS.with(|b| b.borrow().get(&key).copied()) {
        return r;
    }
    let img = tex.get_texture_data();
    let (mut x0, mut y0, mut x1, mut y1) = (i32::MAX, i32::MAX, i32::MIN, i32::MIN);
    for y in 0..g.h as i32 {
        for x in 0..g.w as i32 {
            let (px, py) = (g.sx as i32 + x, g.sy as i32 + y);
            if px < img.width as i32 && py < img.height as i32 && img.get_image_data()[(py as usize) * img.width as usize + px as usize][3] > 16 {
                x0 = x0.min(x);
                y0 = y0.min(y);
                x1 = x1.max(x);
                y1 = y1.max(y);
            }
        }
    }
    let r = if x1 < x0 { Rect::new(g.sx, g.sy, g.w, g.h) } else { Rect::new(g.sx + x0 as f32, g.sy + y0 as f32, (x1 - x0 + 1) as f32, (y1 - y0 + 1) as f32) };
    BOUNDS.with(|b| b.borrow_mut().insert(key, r));
    r
}

/// A round medallion with the character's head (or its initial when unknown) inside.
#[allow(clippy::too_many_arguments)]
pub fn medallion(assets: &mut crate::assets::Assets, head_id: Option<u16>, name: &str, cx: f32, cy: f32, rad: f32, ring: Color, u: f32) {
    draw_circle(cx + 1.5 * u, cy + 2.0 * u, rad, Color::new(0.0, 0.0, 0.0, 0.35));
    draw_circle(cx, cy, rad, Color::new(0.06, 0.045, 0.03, 1.0));
    for i in 0..5 {
        draw_circle(cx, cy - rad * 0.15, rad * (0.95 - i as f32 * 0.16), Color::new(0.9, 0.62, 0.3, 0.05));
    }
    let heading = 3usize;
    let g = head_id.and_then(|h| assets.db.heads.get(&h).map(|a| a[heading])).and_then(|idx| assets.db.frame(idx, 0.0).cloned());
    match g {
        Some(g) => {
            // Head sprites are tall cells (27x64) with the face in a corner of them: crop to
            // the pixels that are drawn (measured once per sprite) and fill the disc.
            if let Some(tex) = assets.texture(g.file) {
                let b = opaque_bounds(&tex, &g);
                let s = (rad * 1.5) / b.w.max(b.h);
                draw_texture_ex(&tex, (cx - b.w * s / 2.0).round(), (cy - b.h * s / 2.0 + rad * 0.05).round(), WHITE, DrawTextureParams { source: Some(b), dest_size: Some(vec2(b.w * s, b.h * s)), ..Default::default() });
            }
        }
        None => {
            let ch: String = name.chars().next().map(|c| c.to_uppercase().collect()).unwrap_or_else(|| "?".into());
            text_mid(FontKind::Title, &ch, cx, cy + rad * 0.32, rad * 0.95, GOLD_DIM);
        }
    }
    draw_circle_lines(cx, cy, rad, 2.0 * u, ring);
    draw_circle_lines(cx, cy, rad - 3.0 * u, 1.0, Color::new(0.0, 0.0, 0.0, 0.5));
}

// ---------------------------------------------------------------- small composites

/// Draws a N2 section heading (small caps + rule) and returns the y under it.
pub fn section(title: &str, x: f32, y: f32, w: f32, u: f32) -> f32 {
    theme::section(title, x, y, w, u)
}

/// An empty-state message: Narrative italic, centred in `r`.
pub fn empty(r: Rect, line1: &str, line2: &str, u: f32) {
    let cy = r.center().y;
    icon(Icon::Feather, r.center().x, cy - 26.0 * u, 26.0 * u, alpha(GOLD_DIM, 0.8));
    text_mid(FontKind::NarrativeItalic, line1, r.center().x, cy + 12.0 * u, 18.0 * u, TEXT_DIM);
    if !line2.is_empty() {
        text_mid(FontKind::NarrativeItalic, line2, r.center().x, cy + 34.0 * u, 16.0 * u, TEXT_FAINT);
    }
}

/// Empty state on parchment.
pub fn empty_ink(r: Rect, line1: &str, line2: &str, u: f32) {
    let cy = r.center().y;
    icon(Icon::Feather, r.center().x, cy - 26.0 * u, 26.0 * u, alpha(INK_DIM, 0.7));
    let w = measure_with(FontKind::NarrativeItalic, line1, 18.0 * u).width;
    ink(FontKind::NarrativeItalic, line1, r.center().x - w / 2.0, cy + 12.0 * u, 18.0 * u, INK_DIM);
    if !line2.is_empty() {
        let w = measure_with(FontKind::NarrativeItalic, line2, 16.0 * u).width;
        ink(FontKind::NarrativeItalic, line2, r.center().x - w / 2.0, cy + 34.0 * u, 16.0 * u, INK_DIM);
    }
}

/// A tiny pill with text (status, tags).
pub fn pill(x: f32, y: f32, label: &str, c: Color, u: f32) -> f32 {
    let size = 12.0 * u;
    let w = measure_with(FontKind::Body, label, size).width + 12.0 * u;
    let r = Rect::new(x, y, w, 17.0 * u);
    draw_rectangle(r.x, r.y, r.w, r.h, alpha(c, 0.18));
    draw_rectangle_lines(r.x, r.y, r.w, r.h, 1.0, alpha(c, 0.7));
    text_with(FontKind::Body, label, r.x + 6.0 * u, r.y + r.h - 4.5 * u, size, c);
    w
}

/// Soft glow under a hero or emblem.
pub fn glow(cx: f32, cy: f32, rad: f32, c: Color) {
    for i in 0..8 {
        let t = i as f32 / 8.0;
        draw_circle(cx, cy, rad * (1.0 - t * 0.8), alpha(c, 0.05));
    }
}

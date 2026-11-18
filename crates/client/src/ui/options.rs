//! Window: options (screen, sound, interface, keys). Switches and sliders with a hand-made
//! feel, grouped in sections; the keys appear as little keycaps.
//!
//! Saved in `config/options.ini` (see `client_core::p3::hud::Options`). The audio code reads
//! the volumes with [`current`] (in memory) or `client_core::p3::hud::audio_volumes()` (file).
use super::character::kit;
use super::hud::{set_options, set_options_in_memory};
use super::theme::{self, FontKind, Surface, GOLD, GOLD_BRIGHT, GOLD_DIM, GOLD_TEXT, MUTED, TEXT, TEXT_DIM, TEXT_FAINT};
use super::widgets::{measure_with, text_with};
use super::WindowId;
use crate::assets::Assets;
use crate::game::Game;
use crate::ui::Ui;
use client_core::p3::hud::Options;
use macroquad::prelude::*;
use std::cell::Cell;

thread_local! {
    static FULLSCREEN: Cell<bool> = const { Cell::new(false) };
    /// A slider was dragged since the last save.
    static SAVE_PENDING: Cell<bool> = const { Cell::new(false) };
}

/// The current options (the in-memory copy, no disk read). Volumes to apply: [`Options::volumes`].
#[allow(dead_code)] // read by the audio code
pub fn current() -> Options {
    super::hud::options()
}

/// A switch row: label and a small hint on the left, the switch on the right. Returns the value.
fn switch_row(r: Rect, label: &str, hint: &str, on: bool, u: f32) -> bool {
    let (hov, _) = kit::row(r, false, u);
    let _ = hov;
    text_with(FontKind::Body, label, r.x + 10.0 * u, r.y + 20.0 * u, 17.0 * u, TEXT);
    if !hint.is_empty() {
        text_with(FontKind::NarrativeItalic, hint, r.x + 10.0 * u, r.y + 38.0 * u, 14.0 * u, TEXT_DIM);
    }
    let sw = Rect::new(r.right() - 54.0 * u, r.y + (r.h - 24.0 * u) / 2.0, 46.0 * u, 24.0 * u);
    let v = kit::toggle(sw, on, u);
    // The whole row is the click target, not just the switch.
    if kit::clicked(r) && !sw.contains(mouse_position().into()) {
        return !on;
    }
    v
}

/// A keycap: a raised little block with the key name on it. Returns its width.
fn keycap(x: f32, y: f32, key: &str, u: f32) -> f32 {
    let size = 13.0 * u;
    let w = (measure_with(FontKind::Title, key, size).width + 14.0 * u).max(26.0 * u);
    let r = Rect::new(x, y, w, 24.0 * u);
    draw_rectangle(r.x, r.y + 2.0 * u, r.w, r.h, Color::new(0.0, 0.0, 0.0, 0.5));
    theme::surface(r, Surface::Raised, u);
    draw_rectangle_lines(r.x, r.y, r.w, r.h, 1.0, GOLD_DIM);
    kit::text_mid(FontKind::Title, key, r.center().x, r.y + 17.0 * u, size, GOLD_BRIGHT);
    w
}

thread_local! {
    /// The action waiting for its new key.
    static REBINDING: Cell<Option<client_core::p3::keys::Action>> = const { Cell::new(None) };
}

/// Every rebindable action with its keycap (click, then press the new key) and the two
/// movement presets.
fn key_editor(ui: &mut Ui, r: Rect, u: f32) {
    use client_core::p3::keys::{Action, KeyBinds};
    let mut binds = crate::keys::binds();
    let waiting = REBINDING.with(Cell::get);
    if let Some(a) = waiting {
        ui.keyboard_captured = true;
        if is_key_pressed(KeyCode::Escape) {
            REBINDING.with(|c| c.set(None));
        } else if let Some(k) = crate::keys::any_pressed() {
            binds.set(a, &crate::keys::name(k));
            crate::keys::set_binds(binds.clone());
            REBINDING.with(|c| c.set(None));
        }
    }
    // Presets.
    let bw = 110.0 * u;
    for (i, (label, preset)) in [("Flechas", KeyBinds::arrows()), ("WASD", KeyBinds::wasd())].into_iter().enumerate() {
        let b = Rect::new(r.right() - (2 - i) as f32 * (bw + 8.0 * u), r.y - 30.0 * u, bw, 26.0 * u);
        let on = binds.key(Action::MoveUp) == preset.key(Action::MoveUp);
        if super::widgets::button_kind(b, label, if on { super::widgets::Kind::Primary } else { super::widgets::Kind::Secondary }, true, u) {
            crate::keys::set_binds(preset);
        }
    }
    let cols = 3;
    let colw = r.w / cols as f32;
    let m: Vec2 = mouse_position().into();
    for (i, a) in Action::ALL.iter().enumerate() {
        let (cx, cy) = (r.x + (i % cols) as f32 * colw, r.y + 4.0 * u + (i / cols) as f32 * 30.0 * u);
        let label = if waiting == Some(*a) { "…".to_string() } else { crate::keys::label(binds.key(*a)) };
        let w = keycap(cx + 6.0 * u, cy, &label, u);
        let cap = Rect::new(cx + 6.0 * u, cy, w, 24.0 * u);
        if cap.contains(m) && is_mouse_button_pressed(MouseButton::Left) {
            REBINDING.with(|c| c.set(Some(*a)));
        }
        text_with(FontKind::Body, a.label(), cx + 6.0 * u + w + 10.0 * u, cy + 17.0 * u, 14.0 * u, if waiting == Some(*a) { GOLD_BRIGHT } else { TEXT });
    }
}

/// Draws the content of the window inside `r` (the frame is already drawn).
pub fn draw(ui: &mut Ui, _game: &mut Game, _assets: &mut Assets, r: Rect) {
    let u = ui.u;
    kit::begin(ui, WindowId::Options);
    let before = super::hud::options();
    let mut o = before.clone();
    let r = Rect::new(r.x + 8.0 * u, r.y + 2.0 * u, r.w - 16.0 * u, r.h - 4.0 * u);
    let col_w = (r.w - 30.0 * u) / 2.0;
    let (lx, rx) = (r.x, r.x + col_w + 30.0 * u);

    // ---- screen (left)
    let mut y = kit::section("Pantalla", lx, r.y, col_w, u);
    text_with(FontKind::Title, &format!("{} × {}", screen_width() as i32, screen_height() as i32), lx + 10.0 * u, y + 24.0 * u, 22.0 * u, GOLD_TEXT);
    text_with(FontKind::NarrativeItalic, "la ventana se puede redimensionar", lx + 10.0 * u, y + 44.0 * u, 14.0 * u, TEXT_DIM);
    y += 56.0 * u;
    let fs = FULLSCREEN.with(Cell::get);
    let nfs = switch_row(Rect::new(lx, y, col_w, 50.0 * u), "Pantalla completa", "", fs, u);
    if nfs != fs {
        FULLSCREEN.with(|f| f.set(nfs));
        set_fullscreen(nfs);
    }
    y += 50.0 * u;
    text_with(FontKind::Body, &format!("Interfaz ×{u:.2}"), lx + 10.0 * u, y + 22.0 * u, 16.0 * u, TEXT);
    text_with(FontKind::NarrativeItalic, "se adapta sola a la altura", lx + 10.0 * u, y + 40.0 * u, 14.0 * u, TEXT_DIM);
    y += 56.0 * u;
    text_with(FontKind::Body, "F2", lx + 10.0 * u, y + 22.0 * u, 16.0 * u, GOLD_BRIGHT);
    text_with(FontKind::Body, "mundo en píxeles enteros", lx + 40.0 * u, y + 22.0 * u, 16.0 * u, TEXT);

    // ---- sound (right)
    let mut sy = kit::section("Sonido", rx, r.y, col_w, u);
    let muted = before.mute;
    for (i, (name, value)) in [("Música", &mut o.music), ("Efectos", &mut o.sfx)].into_iter().enumerate() {
        text_with(FontKind::Body, name, rx + 10.0 * u, sy + 18.0 * u, 17.0 * u, if muted { TEXT_FAINT } else { TEXT });
        let pct = format!("{}%", (*value * 100.0).round() as i32);
        kit::text_right(FontKind::Title, &pct, rx + col_w - 6.0 * u, sy + 18.0 * u, 16.0 * u, if muted { TEXT_FAINT } else { GOLD_BRIGHT });
        *value = kit::slider(Rect::new(rx + 10.0 * u, sy + 28.0 * u, col_w - 24.0 * u, 24.0 * u), *value, i as u32 + 1, !muted, u);
        sy += 62.0 * u;
    }
    o.mute = switch_row(Rect::new(rx, sy, col_w, 50.0 * u), "Silenciar todo", "", o.mute, u);
    sy += 50.0 * u;
    o.show_fps = switch_row(Rect::new(rx, sy, col_w, 50.0 * u), "Mostrar fps", "", o.show_fps, u);

    // ---- keys
    let ky = kit::section("Teclas", r.x, (y + 44.0 * u).max(sy + 56.0 * u), r.w, u);
    key_editor(ui, Rect::new(r.x, ky, r.w, r.bottom() - ky - 20.0 * u), u);
    text_with(FontKind::NarrativeItalic, "Clic en una tecla y pulsa la nueva · Esc cancela · se guarda solo", r.x + 6.0 * u, r.bottom() - 2.0 * u, 13.0 * u, TEXT_FAINT);
    let _ = (GOLD, MUTED);

    // While a slider is dragged only memory changes; the file is written when it is released.
    let dragging = kit::dragging();
    if o != before {
        if dragging {
            set_options_in_memory(o);
            SAVE_PENDING.with(|p| p.set(true));
        } else {
            set_options(o);
            SAVE_PENDING.with(|p| p.set(false));
        }
    } else if !dragging && SAVE_PENDING.with(Cell::get) {
        set_options(before);
        SAVE_PENDING.with(|p| p.set(false));
    }
}

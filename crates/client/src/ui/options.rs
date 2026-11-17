//! Window: options (screen, sound, interface, keys). OWNER: Sonnet 4 (docs/P3_CONTRATO.md).
//!
//! Saved in `config/options.ini` (see `client_core::p3::hud::Options`). The audio code reads
//! the volumes with [`current`] (in memory) or `client_core::p3::hud::audio_volumes()` (file).
use super::hud::{set_options, set_options_in_memory};
use super::theme::{FontKind, GOLD, GOLD_TEXT, GOOD, MUTED, TEXT};
use super::widgets::{measure, text, text_with};
use super::WindowId;
use crate::assets::Assets;
use crate::game::Game;
use crate::ui::Ui;
use client_core::p3::hud::Options;
use macroquad::prelude::*;
use std::cell::Cell;

thread_local! {
    /// Slider being dragged (0 = music, 1 = effects).
    static DRAG: Cell<Option<u8>> = const { Cell::new(None) };
    static FULLSCREEN: Cell<bool> = const { Cell::new(false) };
}

/// The current options (the in-memory copy, no disk read). Volumes to apply: [`Options::volumes`].
#[allow(dead_code)] // read by the audio code
pub fn current() -> Options {
    super::hud::options()
}

fn slider(r: Rect, v: f32, id: u8, enabled: bool, u: f32) -> f32 {
    let track = Rect::new(r.x, r.y + r.h / 2.0 - 4.0 * u, r.w, 8.0 * u);
    draw_rectangle(track.x, track.y, track.w, track.h, Color::new(0.0, 0.0, 0.0, 0.7));
    let fill = if enabled { Color::new(0.75, 0.55, 0.16, 1.0) } else { Color::new(0.35, 0.3, 0.22, 1.0) };
    draw_rectangle(track.x, track.y, track.w * v, track.h, fill);
    draw_rectangle_lines(track.x, track.y, track.w, track.h, 1.0, GOLD);
    let m: Vec2 = mouse_position().into();
    if r.contains(m) && is_mouse_button_pressed(MouseButton::Left) {
        DRAG.with(|d| d.set(Some(id)));
    }
    let mut v = v;
    if DRAG.with(Cell::get) == Some(id) {
        if is_mouse_button_down(MouseButton::Left) {
            v = ((m.x - r.x) / r.w).clamp(0.0, 1.0);
        } else {
            DRAG.with(|d| d.set(None));
        }
    }
    let tx = r.x + r.w * v;
    draw_rectangle(tx - 5.0 * u, r.y + 2.0 * u, 10.0 * u, r.h - 4.0 * u, if r.contains(m) || DRAG.with(Cell::get) == Some(id) { Color::new(1.0, 0.9, 0.55, 1.0) } else { Color::new(0.85, 0.7, 0.35, 1.0) });
    draw_rectangle_lines(tx - 5.0 * u, r.y + 2.0 * u, 10.0 * u, r.h - 4.0 * u, 1.0, BLACK);
    v
}

/// Check box with a label; returns the new value.
fn checkbox(r: Rect, label: &str, on: bool, u: f32) -> bool {
    let b = Rect::new(r.x, r.y + (r.h - 18.0 * u) / 2.0, 18.0 * u, 18.0 * u);
    draw_rectangle(b.x, b.y, b.w, b.h, Color::new(0.0, 0.0, 0.0, 0.7));
    draw_rectangle_lines(b.x, b.y, b.w, b.h, 1.0, if r.contains(mouse_position().into()) { GOLD_TEXT } else { GOLD });
    if on {
        draw_line(b.x + 4.0 * u, b.y + 9.0 * u, b.x + 8.0 * u, b.y + 14.0 * u, 2.5 * u, GOOD);
        draw_line(b.x + 8.0 * u, b.y + 14.0 * u, b.x + 15.0 * u, b.y + 4.0 * u, 2.5 * u, GOOD);
    }
    text(label, b.right() + 8.0 * u, b.y + 14.5 * u, 16.0 * u, TEXT);
    if r.contains(mouse_position().into()) && is_mouse_button_pressed(MouseButton::Left) {
        !on
    } else {
        on
    }
}

fn heading(label: &str, r: Rect, y: f32, u: f32) {
    text_with(FontKind::Title, label, r.x, y + 15.0 * u, 18.0 * u, GOLD_TEXT);
    draw_line(r.x, y + 20.0 * u, r.right(), y + 20.0 * u, 1.0, Color::new(0.30, 0.22, 0.12, 1.0));
}

/// Draws the content of the window inside `r` (the frame is already drawn).
pub fn draw(ui: &mut Ui, _game: &mut Game, _assets: &mut Assets, r: Rect) {
    let u = ui.u;
    let before = super::hud::options();
    let mut o = before.clone();
    let mut y = r.y;

    heading("Pantalla", r, y, u);
    y += 27.0 * u;
    text(&format!("Ventana de {} x {} px (se puede redimensionar)", screen_width() as i32, screen_height() as i32), r.x, y + 13.0 * u, 15.0 * u, TEXT);
    y += 20.0 * u;
    text(&format!("Interfaz x{u:.2}: se adapta sola a la altura de la ventana"), r.x, y + 13.0 * u, 15.0 * u, MUTED);
    y += 20.0 * u;
    text("F2: pixel-perfect (escalado entero del mundo)", r.x, y + 13.0 * u, 15.0 * u, MUTED);
    y += 22.0 * u;
    let fs = FULLSCREEN.with(Cell::get);
    let nfs = checkbox(Rect::new(r.x, y, r.w, 22.0 * u), "Pantalla completa", fs, u);
    if nfs != fs {
        FULLSCREEN.with(|f| f.set(nfs));
        set_fullscreen(nfs);
    }
    y += 28.0 * u;

    heading("Sonido", r, y, u);
    y += 27.0 * u;
    let label_w = 70.0 * u;
    let val_w = 44.0 * u;
    for (i, (name, value)) in [("Música", &mut o.music), ("Efectos", &mut o.sfx)].into_iter().enumerate() {
        text(name, r.x, y + 17.0 * u, 16.0 * u, TEXT);
        *value = slider(Rect::new(r.x + label_w, y, r.w - label_w - val_w - 8.0 * u, 24.0 * u), *value, i as u8, !before.mute, u);
        let pct = format!("{}%", (*value * 100.0).round() as i32);
        text(&pct, r.right() - measure(&pct, 16.0 * u).width, y + 17.0 * u, 16.0 * u, if before.mute { MUTED } else { WHITE });
        y += 28.0 * u;
    }
    o.mute = checkbox(Rect::new(r.x, y, r.w * 0.5, 22.0 * u), "Silenciar todo", o.mute, u);
    o.show_fps = checkbox(Rect::new(r.x + r.w * 0.5, y, r.w * 0.5, 22.0 * u), "Mostrar fps", o.show_fps, u);
    y += 30.0 * u;

    heading("Teclas", r, y, u);
    y += 27.0 * u;
    let mut keys: Vec<(String, &str)> = WindowId::ALL.iter().filter_map(|w| w.hotkey().map(|k| (format!("{k:?}"), if *w == WindowId::WorldMap { "Mapa" } else { w.title() }))).collect();
    keys.extend([("Enter".into(), "Hablar"), ("Flechas".into(), "Caminar"), ("1-9".into(), "Lanzar"), ("Esc".into(), "Cancelar")]);
    let colw = r.w / 3.0;
    for (i, (k, what)) in keys.iter().enumerate() {
        let (cx, cy) = (r.x + (i % 3) as f32 * colw, y + (i / 3) as f32 * 19.0 * u);
        text(k, cx, cy + 13.0 * u, 15.0 * u, GOLD_TEXT);
        text(what, cx + 52.0 * u, cy + 13.0 * u, 15.0 * u, TEXT);
    }
    text("Los ajustes se guardan solos en config/options.ini", r.x, r.bottom() - 4.0 * u, 13.0 * u, MUTED);

    // While a slider is dragged only memory changes; the file is written when it is released.
    let dragging = DRAG.with(Cell::get).is_some();
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

thread_local! {
    /// A slider was dragged since the last save.
    static SAVE_PENDING: Cell<bool> = const { Cell::new(false) };
}

//! Window: spell book with hotkeys. OWNER: Sonnet 4 (docs/P3_CONTRATO.md).
//!
//! The 40 slots of the book (scrollable), cast, assign the selected spell to keys 1-9
//! (saved in `config/spellkeys.ini`) and meditate. The tooltip describes the spell.
use super::hud::{set_spell_keys, spell_keys};
use super::theme::{BAD, GOLD, GOLD_TEXT, MUTED, TEXT};
use super::widgets::{button, measure, text, text_fit, tooltip};
use crate::assets::Assets;
use crate::game::Game;
use crate::ui::Ui;
use ao_protocol::status;
use client_core::local::SPELL_SLOTS;
use client_core::p3::hud as logic;
use macroquad::prelude::*;
use std::cell::Cell;

thread_local! {
    /// (selected slot 0-based, first visible row, time of the last click on a row)
    static VIEW: Cell<(usize, usize, f64)> = const { Cell::new((0, 0, 0.0)) };
}

/// Draws the content of the window inside `r` (the frame is already drawn).
pub fn draw(ui: &mut Ui, game: &mut Game, _assets: &mut Assets, r: Rect) {
    let u = ui.u;
    let (mut sel, mut first, mut last_click) = VIEW.with(Cell::get);
    let known = game.book().len();
    let mut tip: Vec<String> = Vec::new();

    text(&format!("{known} de {SPELL_SLOTS} hechizos"), r.x, r.y + 14.0 * u, 16.0 * u, GOLD_TEXT);
    let mana = format!("Maná {}/{}", game.stats.mana.get(), game.stats.max_mana.get());
    text(&mana, r.right() - measure(&mana, 16.0 * u).width, r.y + 14.0 * u, 16.0 * u, Color::new(0.45, 0.65, 1.0, 1.0));

    // List.
    let row_h = 24.0 * u;
    let footer_h = 138.0 * u;
    let list = Rect::new(r.x, r.y + 22.0 * u, r.w, (r.h - 22.0 * u - footer_h).max(row_h * 3.0));
    let rows = (list.h / row_h).floor() as usize;
    let list = Rect::new(list.x, list.y, list.w, rows as f32 * row_h);
    draw_rectangle(list.x, list.y, list.w, list.h, Color::new(0.03, 0.025, 0.02, 0.9));
    draw_rectangle_lines(list.x, list.y, list.w, list.h, 1.0, GOLD);
    if list.contains(mouse_position().into()) {
        let wheel = mouse_wheel().1;
        if wheel > 0.0 {
            first = first.saturating_sub(2);
        } else if wheel < 0.0 {
            first += 2;
        }
    }
    first = first.min(SPELL_SLOTS.saturating_sub(rows));
    let keys = spell_keys();
    let slots = game.local.spell_slots.clone();
    for i in 0..rows {
        let slot = first + i;
        let Some(id) = slots.get(slot).copied() else { break };
        let rr = Rect::new(list.x + 1.0, list.y + i as f32 * row_h, list.w - 12.0 * u, row_h);
        let def = game.spells.iter().find(|s| s.id == id).cloned();
        let hover = rr.contains(mouse_position().into());
        if slot == sel {
            draw_rectangle(rr.x, rr.y, rr.w, rr.h, Color::new(0.36, 0.25, 0.08, 0.95));
        } else if hover && def.is_some() {
            draw_rectangle(rr.x, rr.y, rr.w, rr.h, Color::new(0.18, 0.13, 0.07, 1.0));
        } else if slot % 2 == 0 {
            draw_rectangle(rr.x, rr.y, rr.w, rr.h, Color::new(1.0, 1.0, 1.0, 0.025));
        }
        text(&format!("{:>2}", slot + 1), rr.x + 6.0 * u, rr.y + 17.0 * u, 14.0 * u, MUTED);
        match def {
            Some(s) => {
                let enough = game.stats.mana.get() >= s.mana;
                let m = s.mana.to_string();
                let md = measure(&m, 14.0 * u);
                text(&m, rr.right() - md.width - 8.0 * u, rr.y + 17.0 * u, 14.0 * u, if enough { Color::new(0.45, 0.65, 1.0, 1.0) } else { BAD });
                let mut name_right = rr.right() - md.width - 14.0 * u;
                if let Some(k) = keys.key_of(id, &slots) {
                    let badge = Rect::new(name_right - 18.0 * u, rr.y + 4.0 * u, 16.0 * u, 16.0 * u);
                    draw_rectangle(badge.x, badge.y, badge.w, badge.h, Color::new(0.4, 0.28, 0.08, 1.0));
                    draw_rectangle_lines(badge.x, badge.y, badge.w, badge.h, 1.0, GOLD_TEXT);
                    text(&(k + 1).to_string(), badge.x + 4.5 * u, badge.y + 12.5 * u, 13.0 * u, GOLD_TEXT);
                    name_right = badge.x - 4.0 * u;
                }
                text_fit(&s.name, rr.x + 34.0 * u, rr.y + 17.0 * u, 16.0 * u, name_right - rr.x - 34.0 * u, TEXT);
                if hover {
                    tip = logic::spell_lines(&s, game.stats.level);
                    if is_mouse_button_pressed(MouseButton::Left) {
                        let double = sel == slot && get_time() - last_click < 0.4;
                        sel = slot;
                        last_click = get_time();
                        if double {
                            game.arm_spell(id);
                        }
                    }
                }
            }
            None => text("vacío", rr.x + 34.0 * u, rr.y + 17.0 * u, 14.0 * u, Color::new(0.35, 0.32, 0.28, 1.0)),
        }
    }
    // Scrollbar.
    let sb = Rect::new(list.right() - 10.0 * u, list.y + 1.0, 9.0 * u, list.h - 2.0);
    draw_rectangle(sb.x, sb.y, sb.w, sb.h, Color::new(0.0, 0.0, 0.0, 0.4));
    let max_first = SPELL_SLOTS.saturating_sub(rows).max(1);
    let (th, ty) = (sb.h * rows as f32 / SPELL_SLOTS as f32, sb.y + (sb.h - sb.h * rows as f32 / SPELL_SLOTS as f32) * first as f32 / max_first as f32);
    draw_rectangle(sb.x + 1.0, ty, sb.w - 2.0, th, GOLD);

    // Selected spell: details, hotkeys, actions.
    let sel_id = slots.get(sel).copied().unwrap_or(0);
    let sel_def = game.spells.iter().find(|s| s.id == sel_id).cloned();
    let mut y = list.bottom() + 8.0 * u;
    match &sel_def {
        Some(s) => {
            text(&s.name, r.x, y + 14.0 * u, 18.0 * u, GOLD_TEXT);
            let lines = logic::spell_lines(s, game.stats.level);
            text_fit(&lines[1], r.x, y + 33.0 * u, 14.0 * u, r.w, TEXT);
            text(&lines[3], r.x, y + 50.0 * u, 14.0 * u, MUTED);
        }
        None => text("Elige un hechizo de la lista.", r.x, y + 14.0 * u, 16.0 * u, MUTED),
    }
    y += 58.0 * u;
    text("Tecla:", r.x, y + 17.0 * u, 15.0 * u, MUTED);
    let kx = r.x + 46.0 * u;
    let kw = (r.right() - kx - 8.0 * u * 1.0) / 9.0;
    let current = keys.key_of(sel_id, &slots);
    for k in 0..9 {
        let kr = Rect::new(kx + k as f32 * kw, y, kw - 3.0 * u, 24.0 * u);
        let bound = keys.spell_on(k, &slots);
        let owner = game.spells.iter().find(|s| s.id == bound).map(|s| s.name.clone());
        let hover = kr.contains(mouse_position().into());
        if button(kr, &(k + 1).to_string(), current == Some(k), u) && sel_def.is_some() {
            let mut nk = keys.clone();
            nk.bind(k, if current == Some(k) { 0 } else { sel_id }, &slots);
            set_spell_keys(nk);
        }
        if hover {
            tip = vec![format!("Tecla {}", k + 1), match (owner, sel_def.as_ref()) {
                (Some(o), _) if current == Some(k) => format!("{o}: clic para soltarla."),
                (Some(o), Some(s)) => format!("Ahora: {o}. Clic para poner {}.", s.name),
                (None, Some(s)) => format!("Libre. Clic para poner {}.", s.name),
                (Some(o), None) => format!("Ahora: {o}."),
                (None, None) => "Libre. Elige primero un hechizo.".into(),
            }];
        }
    }
    y += 32.0 * u;
    let half = (r.w - 8.0 * u) / 2.0;
    let cast = Rect::new(r.x, y, half, 30.0 * u);
    if button(cast, if game.casting { "Elige objetivo..." } else { "Lanzar" }, game.casting, u) && sel_def.is_some() {
        game.arm_spell(sel_id);
    }
    let meditating = game.me().is_some_and(|e| e.status & status::MEDITATING != 0);
    let med = Rect::new(r.x + half + 8.0 * u, y, half, 30.0 * u);
    if button(med, if meditating { "Dejar de meditar" } else { "Meditar" }, meditating, u) {
        game.toggle_meditation();
    }
    if med.contains(mouse_position().into()) {
        tip = vec!["Meditar".into(), "Recupera maná mientras no te mueves.".into(), "Moverte o recibir un golpe lo interrumpe.".into()];
    }
    if cast.contains(mouse_position().into()) && sel_def.is_some() {
        tip = vec!["Lanzar".into(), "Arma el hechizo: haz clic en el mundo para lanzarlo.".into(), "Doble clic en la lista hace lo mismo.".into()];
    }
    VIEW.with(|v| v.set((sel, first, last_click)));
    tooltip(&tip, u);
}

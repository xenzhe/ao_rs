//! Window: the grimoire (spells and hotkeys). Two parchment pages like an open book: the
//! known spells on the left, the chosen one on the right with its description, numbers and
//! the 1-9 keys. Keys are saved in `config/spellkeys.ini`; double click or "Lanzar" arms it.
use super::character::kit::{self, Icon, ARCANE, INK_DIM, WAX};
use super::hud::{set_spell_keys, spell_keys};
use super::theme::{FontKind, BAD, GOLD, GOLD_BRIGHT, GOLD_DIM, GOLD_TEXT, MANA, TEXT, TEXT_DIM};
use super::widgets::{button_kind, text_with, tooltip, Kind};
use super::WindowId;
use crate::assets::Assets;
use crate::game::Game;
use crate::ui::Ui;
use ao_protocol::status;
use ao_rules::spells::{SpellDef, SpellEffect};
use client_core::local::SPELL_SLOTS;
use client_core::p3::hud as logic;
use macroquad::prelude::*;
use std::cell::Cell;

thread_local! {
    /// (selected slot 0-based, first visible row, time of the last click on a row)
    static VIEW: Cell<(usize, usize, f64)> = const { Cell::new((0, 0, 0.0)) };
}

/// Colour, symbol, school name and flavour text of a spell.
fn lore(s: &SpellDef) -> (Color, Icon, &'static str, &'static str) {
    match s.effect {
        // The server only describes the three effects, so the prose is about the effect and
        // not about a particular spell (it has to read right for every spell that shares it).
        SpellEffect::Heal => (Color::new(0.30, 0.68, 0.42, 1.0), Icon::Leaf, "Magia de sanación", "Concentra tu maná y la energía fluye hacia el objetivo, restaurándolo."),
        SpellEffect::Fireball => (Color::new(0.92, 0.46, 0.12, 1.0), Icon::Flame, "Magia ofensiva", "Concentra tu maná en un proyectil que brota de tu mano y hiere a quien alcance."),
        SpellEffect::Lightning => (Color::new(0.45, 0.66, 1.0, 1.0), Icon::Bolt, "Magia de choque", "Concentra tu maná en un chasquido de energía que cae sobre el objetivo."),
    }
}

/// Draws the content of the window inside `r` (the frame is already drawn).
pub fn draw(ui: &mut Ui, game: &mut Game, _assets: &mut Assets, r: Rect) {
    let u = ui.u;
    kit::begin(ui, WindowId::Spells);
    let (mut sel, mut first, mut last_click) = VIEW.with(Cell::get);
    let slots = game.local.spell_slots.clone();
    let known: Vec<usize> = (0..slots.len()).filter(|i| game.spells.iter().any(|s| s.id == slots[*i])).collect();
    if !known.contains(&sel) {
        sel = known.first().copied().unwrap_or(0);
    }
    let mut tip: Vec<String> = Vec::new();

    // ---- the open book: two pages and a spine between them
    let book = Rect::new(r.x, r.y + 4.0 * u, r.w, r.h - 50.0 * u);
    let gap = 10.0 * u;
    let pw = (book.w - gap) / 2.0;
    let left = Rect::new(book.x, book.y, pw, book.h);
    let right = Rect::new(book.x + pw + gap, book.y, pw, book.h);
    kit::page(left, u);
    kit::page(right, u);
    // Spine shading, towards the centre of the book.
    for i in 0..10 {
        let a = 0.22 * (1.0 - i as f32 / 10.0);
        draw_rectangle(left.right() - (i as f32 + 1.0) * 2.5 * u, left.y, 2.5 * u, left.h, Color::new(0.15, 0.08, 0.02, a));
        draw_rectangle(right.x + i as f32 * 2.5 * u, right.y, 2.5 * u, right.h, Color::new(0.15, 0.08, 0.02, a));
    }
    draw_rectangle(left.right(), book.y, gap, book.h, Color::new(0.10, 0.06, 0.03, 1.0));

    // ---- left page: the list
    let pad = 20.0 * u;
    kit::ink(FontKind::Title, "HECHIZOS CONOCIDOS", left.x + pad, left.y + 32.0 * u, 16.0 * u, WAX);
    kit::ink(FontKind::Body, &format!("{} de {}", known.len(), SPELL_SLOTS), left.right() - pad - 40.0 * u, left.y + 32.0 * u, 14.0 * u, INK_DIM);
    kit::rule(left.x + pad, left.y + 42.0 * u, left.w - 2.0 * pad, Color::new(0.35, 0.2, 0.08, 0.9));
    let list = Rect::new(left.x + 10.0 * u, left.y + 52.0 * u, left.w - 20.0 * u, left.h - 68.0 * u);
    let row_h = 34.0 * u;
    let rows = ((list.h / row_h).floor() as usize).max(1);
    first = kit::wheel(list, first, known.len(), rows, 2);
    let keys = spell_keys();
    let mana_now = game.stats.mana.get();
    if known.is_empty() {
        kit::empty_ink(list, "Tus páginas están en blanco.", "Aprende hechizos para llenarlas.", u);
    }
    for (i, slot) in known.iter().copied().enumerate().skip(first).take(rows) {
        let id = slots[slot];
        let Some(s) = game.spells.iter().find(|s| s.id == id).cloned() else { continue };
        let rr = Rect::new(list.x, list.y + (i - first) as f32 * row_h, list.w, row_h - 2.0 * u);
        let (hov, click) = kit::row_ink(rr, slot == sel, u);
        let (col, sym, _, _) = lore(&s);
        let c = vec2(rr.x + 22.0 * u, rr.center().y);
        draw_circle(c.x, c.y, 13.0 * u, Color::new(col.r * 0.55, col.g * 0.55, col.b * 0.55, 1.0));
        draw_circle_lines(c.x, c.y, 13.0 * u, 1.5, kit::alpha(col, 0.9));
        kit::icon(sym, c.x, c.y, 15.0 * u, Color::new(1.0, 0.95, 0.85, 0.95));
        let enough = mana_now >= s.mana;
        let mana = s.mana.to_string();
        let mw = super::widgets::measure(&mana, 16.0 * u).width;
        let mc = if enough { Color::new(0.12, 0.25, 0.62, 1.0) } else { Color::new(0.65, 0.1, 0.08, 1.0) };
        kit::ink(FontKind::Body, &mana, rr.right() - 12.0 * u - mw, rr.center().y + 5.5 * u, 16.0 * u, mc);
        kit::icon(Icon::Drop, rr.right() - 12.0 * u - mw - 11.0 * u, rr.center().y, 11.0 * u, mc);
        let mut name_right = rr.right() - 12.0 * u - mw - 26.0 * u;
        if let Some(k) = keys.key_of(id, &slots) {
            let kc = vec2(name_right - 9.0 * u, rr.center().y);
            draw_circle(kc.x, kc.y, 9.0 * u, WAX);
            draw_circle_lines(kc.x, kc.y, 9.0 * u, 1.2, GOLD);
            kit::text_mid(FontKind::Title, &(k + 1).to_string(), kc.x, kc.y + 4.5 * u, 13.0 * u, GOLD_BRIGHT);
            name_right = kc.x - 14.0 * u;
        }
        kit::ink_fit(FontKind::Narrative, &s.name, rr.x + 44.0 * u, rr.center().y + 6.0 * u, 18.0 * u, name_right - rr.x - 44.0 * u, if slot == sel { Color::new(0.30, 0.06, 0.04, 1.0) } else { super::theme::INK });
        if click {
            let double = sel == slot && get_time() - last_click < 0.4;
            sel = slot;
            last_click = get_time();
            if double {
                game.arm_spell(id);
            }
        }
        let _ = (hov, &logic::spell_lines);
    }
    kit::scrollbar(list, known.len(), rows, first, u);

    // ---- right page: the chosen spell
    let sel_id = slots.get(sel).copied().unwrap_or(0);
    let sel_def = game.spells.iter().find(|s| s.id == sel_id).cloned();
    let cx = right.center().x;
    let rpad = 24.0 * u;
    let inner_w = right.w - 2.0 * rpad;
    match &sel_def {
        None => {
            kit::empty_ink(Rect::new(right.x, right.y, right.w, right.h - 70.0 * u), "Elige un hechizo de la izquierda.", "Aquí leerás cómo se lanza.", u);
            // Meditating does not need a spell: it stays available with an empty book.
            let meditating = game.me().is_some_and(|e| e.status & status::MEDITATING != 0);
            let med = Rect::new(right.x + rpad, right.bottom() - 56.0 * u, inner_w, 38.0 * u);
            if button_kind(med, if meditating { "Dejar de meditar" } else { "Meditar" }, Kind::Secondary, true, u) {
                game.toggle_meditation();
            }
        }
        Some(s) => {
            let (col, sym, school, flavour) = lore(s);
            // Medallion with a glow that breathes slowly.
            let pulse = 0.5 + 0.5 * (get_time() * 1.6).sin() as f32;
            let m = vec2(cx, right.y + 56.0 * u);
            for i in 0..7 {
                draw_circle(m.x, m.y, (52.0 - i as f32 * 4.0 + 3.0 * pulse) * u, kit::alpha(col, 0.045));
            }
            draw_circle(m.x + 2.0 * u, m.y + 3.0 * u, 34.0 * u, Color::new(0.0, 0.0, 0.0, 0.25));
            draw_circle(m.x, m.y, 34.0 * u, Color::new(col.r * 0.35, col.g * 0.35, col.b * 0.35, 1.0));
            draw_circle(m.x, m.y, 29.0 * u, Color::new(col.r * 0.6, col.g * 0.6, col.b * 0.6, 1.0));
            draw_circle_lines(m.x, m.y, 34.0 * u, 2.5 * u, GOLD);
            draw_circle_lines(m.x, m.y, 29.0 * u, 1.0, kit::alpha(GOLD_BRIGHT, 0.6));
            kit::icon(sym, m.x, m.y, 34.0 * u, Color::new(1.0, 0.96, 0.85, 1.0));
            // Name, school, rule.
            kit::ink_mid(FontKind::Title, &s.name, cx, right.y + 120.0 * u, 28.0 * u, inner_w, Color::new(0.32, 0.06, 0.04, 1.0));
            kit::ink_mid(FontKind::NarrativeItalic, school, cx, right.y + 140.0 * u, 16.0 * u, inner_w, INK_DIM);
            kit::rule(right.x + rpad, right.y + 150.0 * u, inner_w, Color::new(0.35, 0.2, 0.08, 0.9));
            let after = kit::paragraph(FontKind::Narrative, flavour, right.x + rpad, right.y + 156.0 * u, inner_w, 18.0 * u, super::theme::INK, 3);
            // The four numbers.
            let amount = s.base_amount(game.stats.level);
            let (pw_icon, pw_label) = match s.effect {
                SpellEffect::Heal => (Icon::Heart, "Cura"),
                _ => (Icon::Sword, "Daño"),
            };
            let stats: [(Icon, String, &str, Color); 4] = [
                (pw_icon, format!("~{amount}"), pw_label, Color::new(0.62, 0.12, 0.10, 1.0)),
                (Icon::Drop, s.mana.to_string(), "Maná", Color::new(0.12, 0.25, 0.62, 1.0)),
                (Icon::Target, s.range.to_string(), "Alcance", Color::new(0.30, 0.22, 0.10, 1.0)),
                (Icon::Hourglass, format!("{:.1} s", s.cooldown_ms as f32 / 1000.0), "Espera", Color::new(0.30, 0.22, 0.10, 1.0)),
            ];
            let sy = (after + 8.0 * u).max(right.y + 236.0 * u);
            let colw = inner_w / 4.0;
            for (i, (ic, val, label, c)) in stats.iter().enumerate() {
                let x = right.x + rpad + colw * (i as f32 + 0.5);
                kit::icon(*ic, x, sy + 10.0 * u, 20.0 * u, *c);
                kit::ink(FontKind::Title, val, x - super::widgets::measure_with(FontKind::Title, val, 22.0 * u).width / 2.0, sy + 44.0 * u, 22.0 * u, if i == 1 && mana_now < s.mana { Color::new(0.65, 0.1, 0.08, 1.0) } else { super::theme::INK });
                kit::ink(FontKind::Body, label, x - super::widgets::measure_with(FontKind::Body, label, 12.0 * u).width / 2.0, sy + 61.0 * u, 12.0 * u, INK_DIM);
                if i > 0 {
                    draw_line(right.x + rpad + colw * i as f32, sy, right.x + rpad + colw * i as f32, sy + 60.0 * u, 1.0, Color::new(0.35, 0.22, 0.1, 0.35));
                }
            }
            let target = match s.target {
                ao_rules::spells::SpellTarget::Enemy => "Se lanza contra criaturas.",
                ao_rules::spells::SpellTarget::Ally => "Se lanza sobre personajes, tú incluido.",
            };
            kit::ink(FontKind::NarrativeItalic, target, cx - super::widgets::measure_with(FontKind::NarrativeItalic, target, 16.0 * u).width / 2.0, sy + 84.0 * u, 16.0 * u, INK_DIM);

            // Hotkeys 1-9 as little coins.
            let ky = right.bottom() - 84.0 * u;
            kit::ink(FontKind::Title, "ATAJO", right.x + rpad, ky - 2.0 * u, 12.0 * u, INK_DIM);
            let current = keys.key_of(sel_id, &slots);
            let step = (inner_w - 48.0 * u) / 9.0;
            for k in 0..9 {
                let c = vec2(right.x + rpad + 62.0 * u + step * k as f32 + step / 2.0 - 8.0 * u, ky - 5.0 * u);
                let kr = Rect::new(c.x - 12.0 * u, c.y - 12.0 * u, 24.0 * u, 24.0 * u);
                let bound = keys.spell_on(k, &slots);
                let mine = current == Some(k);
                let hv = kit::hover(kr);
                let owner = game.spells.iter().find(|s| s.id == bound).map(|s| s.name.clone());
                let fill = if mine { WAX } else if bound != 0 { Color::new(0.38, 0.28, 0.16, 1.0) } else { Color::new(0.3, 0.2, 0.1, if hv { 0.35 } else { 0.18 }) };
                draw_circle(c.x, c.y, 12.0 * u, fill);
                draw_circle_lines(c.x, c.y, 12.0 * u, if mine { 2.0 } else { 1.2 }, if mine { GOLD } else { Color::new(0.3, 0.2, 0.1, 0.8) });
                kit::text_mid(FontKind::Title, &(k + 1).to_string(), c.x, c.y + 5.0 * u, 14.0 * u, if mine || bound != 0 { GOLD_BRIGHT } else { INK_DIM });
                if kit::clicked(kr) {
                    let mut nk = keys.clone();
                    nk.bind(k, if mine { 0 } else { sel_id }, &slots);
                    set_spell_keys(nk);
                }
                if hv {
                    tip = vec![format!("Tecla {}", k + 1), match (owner, mine) {
                        (Some(o), true) => format!("{o}: clic para soltarla."),
                        (Some(o), false) => format!("Ahora: {o}. Clic para poner {}.", s.name),
                        (None, _) => format!("Libre. Clic para poner {}.", s.name),
                    }];
                }
            }
            // Actions.
            let by = right.bottom() - 56.0 * u;
            let half = (inner_w - 10.0 * u) / 2.0;
            let cast = Rect::new(right.x + rpad, by, half, 38.0 * u);
            if button_kind(cast, if game.casting { "Elige objetivo..." } else { "Lanzar" }, Kind::Primary, true, u) {
                game.arm_spell(sel_id);
            }
            let meditating = game.me().is_some_and(|e| e.status & status::MEDITATING != 0);
            let med = Rect::new(cast.right() + 10.0 * u, by, half, 38.0 * u);
            if button_kind(med, if meditating { "Dejar de meditar" } else { "Meditar" }, Kind::Secondary, true, u) {
                game.toggle_meditation();
            }
            if kit::hover(med) {
                tip = vec!["Meditar".into(), "Recupera maná mientras no te mueves.".into(), "Moverte o recibir un golpe lo interrumpe.".into()];
            }
            if kit::hover(cast) {
                tip = vec!["Lanzar".into(), "Arma el hechizo: haz clic en el mundo para lanzarlo.".into(), "Doble clic en la lista hace lo mismo.".into()];
            }
        }
    }

    // ---- the mana well under the book
    let mw = Rect::new(book.x, book.bottom() + 12.0 * u, book.w, 22.0 * u);
    let (m, mm) = (game.stats.mana.get(), game.stats.max_mana.get().max(1));
    kit::icon(Icon::Drop, mw.x + 10.0 * u, mw.center().y, 18.0 * u, ARCANE);
    text_with(FontKind::Title, "MANÁ", mw.x + 26.0 * u, mw.center().y + 5.0 * u, 14.0 * u, GOLD_TEXT);
    let bar = Rect::new(mw.x + 84.0 * u, mw.center().y - 6.0 * u, mw.w - 84.0 * u - 130.0 * u, 12.0 * u);
    kit::thin_bar(bar, m as f32, mm as f32, MANA);
    draw_rectangle_lines(bar.x, bar.y, bar.w, bar.h, 1.0, GOLD_DIM);
    kit::text_right(FontKind::Body, &format!("{m} / {mm}"), mw.right(), mw.center().y + 5.5 * u, 16.0 * u, if m < 1 { BAD } else { TEXT });
    let _ = TEXT_DIM;

    VIEW.with(|v| v.set((sel, first, last_click)));
    tooltip(&tip, u);
}

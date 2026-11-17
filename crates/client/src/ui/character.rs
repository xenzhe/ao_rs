//! Window: attributes, skills with free points, faction and statistics.
//! OWNER: Sonnet 4 (docs/P3_CONTRATO.md).
use super::hud::{meter, thousands, EXP_GOLD, HP_RED, MANA_BLUE};
use super::theme::{FontKind, BAD, GOLD, GOLD_TEXT, GOOD, MUTED, TEXT};
use super::widgets::{measure, text, text_fit, text_with, tooltip};
use crate::assets::Assets;
use crate::game::Game;
use crate::ui::Ui;
use ao_rules::progression::MAX_SKILL;
use client_core::local::SKILLS;
use client_core::p3::hud as logic;
use macroquad::prelude::*;

const ATTR_HELP: [&str; 5] = [
    "Daño cuerpo a cuerpo y capacidad de carga.",
    "Esquive y velocidad de ataque.",
    "Maná y poder de los hechizos.",
    "Vida y resistencia.",
    "Comercio, liderazgo y trato con la gente.",
];

/// Draws the content of the window inside `r` (the frame is already drawn).
pub fn draw(ui: &mut Ui, game: &mut Game, _assets: &mut Assets, r: Rect) {
    let u = ui.u;
    let mut tip: Vec<String> = Vec::new();
    let mut y = r.y;

    // Name, level, experience.
    let name = game.me().map(|e| e.name.clone()).unwrap_or_default();
    text_with(FontKind::Title, &name, r.x, y + 18.0 * u, 22.0 * u, Color::new(0.80, 0.90, 1.0, 1.0));
    let lvl = format!("Nivel {}", game.stats.level);
    text_with(FontKind::Title, &lvl, r.right() - super::hud::title_width(&lvl, 20.0 * u) - 2.0 * u, y + 18.0 * u, 20.0 * u, GOLD_TEXT);
    y += 26.0 * u;
    let (exp, next) = (game.stats.exp, game.stats.next_exp.max(1));
    meter(Rect::new(r.x, y, r.w, 16.0 * u), exp as f32, next as f32, EXP_GOLD, "Experiencia", &format!("{} / {}", thousands(exp), thousands(next)), u);
    y += 24.0 * u;

    // Attributes.
    let m: Vec2 = mouse_position().into();
    let aw = (r.w - 4.0 * 5.0 * u) / 5.0;
    for (i, nm) in logic::ATTRIBUTE_NAMES.iter().enumerate() {
        let b = Rect::new(r.x + i as f32 * (aw + 5.0 * u), y, aw, 42.0 * u);
        draw_rectangle(b.x, b.y, b.w, b.h, Color::new(0.05, 0.04, 0.03, 0.9));
        draw_rectangle_lines(b.x, b.y, b.w, b.h, 1.0, GOLD);
        let v = game.local.attributes[i].to_string();
        text(&v, b.x + (b.w - measure(&v, 24.0 * u).width) / 2.0, b.y + 24.0 * u, 24.0 * u, Color::new(1.0, 0.92, 0.6, 1.0));
        let d = measure(nm, 12.5 * u);
        text(nm, b.x + (b.w - d.width.min(b.w - 4.0)) / 2.0, b.y + 38.0 * u, 12.5 * u, MUTED);
        if b.contains(m) {
            tip = vec![nm.to_string(), ATTR_HELP[i].to_string()];
        }
    }
    y += 50.0 * u;

    // Skills.
    let free = game.local.free_skill_points;
    text_with(FontKind::Title, "Habilidades", r.x, y + 15.0 * u, 18.0 * u, GOLD_TEXT);
    let fp = format!("Puntos libres: {free}");
    text(&fp, r.right() - measure(&fp, 16.0 * u).width, y + 15.0 * u, 16.0 * u, if free > 0 { Color::new(0.5, 1.0, 0.5, 1.0) } else { MUTED });
    y += 22.0 * u;
    let (col_w, row_h) = ((r.w - 10.0 * u) / 2.0, 19.5 * u);
    let shift = is_key_down(KeyCode::LeftShift) || is_key_down(KeyCode::RightShift);
    let pulse = 0.5 + 0.5 * (get_time() * 4.0).sin() as f32;
    for i in 0..SKILLS {
        let (col, row) = (i / 12, i % 12);
        let rr = Rect::new(r.x + col as f32 * (col_w + 10.0 * u), y + row as f32 * row_h, col_w, row_h - 1.5 * u);
        let id = (i + 1) as u8;
        let v = game.local.skills.get(i).copied().unwrap_or(0);
        draw_rectangle(rr.x, rr.y, rr.w, rr.h, Color::new(0.04, 0.03, 0.02, 0.85));
        draw_rectangle(rr.x, rr.y, rr.w * v as f32 / MAX_SKILL as f32, rr.h, Color::new(0.45, 0.32, 0.10, 0.55));
        text_fit(logic::skill_name(id), rr.x + 4.0 * u, rr.y + 14.0 * u, 14.5 * u, col_w - 56.0 * u, TEXT);
        let vs = v.to_string();
        text(&vs, rr.right() - 22.0 * u - measure(&vs, 14.5 * u).width - 3.0 * u, rr.y + 14.0 * u, 14.5 * u, if v >= MAX_SKILL { GOOD } else { WHITE });
        let btn = Rect::new(rr.right() - 19.0 * u, rr.y + 1.0, 18.0 * u, rr.h - 2.0);
        let can = free > 0 && v < MAX_SKILL;
        if can {
            let hov = btn.contains(m);
            draw_rectangle(btn.x, btn.y, btn.w, btn.h, if hov { Color::new(0.30, 0.55, 0.22, 1.0) } else { Color::new(0.16, 0.34, 0.14, 1.0) });
            draw_rectangle_lines(btn.x, btn.y, btn.w, btn.h, 1.0 + pulse, GOOD);
            text("+", btn.x + 4.5 * u, btn.y + btn.h - 3.5 * u, 17.0 * u, WHITE);
            if hov && is_mouse_button_pressed(MouseButton::Left) {
                game.assign_skill(id, if shift { 5 } else { 1 });
            }
        }
        if rr.contains(m) {
            tip = vec![logic::skill_name(id).to_string(), format!("{v} / {MAX_SKILL}")];
            if can {
                tip.push("Clic: +1 punto.   Mayús + clic: +5.".into());
            } else if v >= MAX_SKILL {
                tip.push("Al máximo.".into());
            } else {
                tip.push("Sin puntos libres: se ganan al subir de nivel.".into());
            }
        }
    }
    y += 12.0 * row_h + 4.0 * u;

    // Faction and standing.
    draw_line(r.x, y, r.right(), y, 1.0, Color::new(0.30, 0.22, 0.12, 1.0));
    y += 4.0 * u;
    let fac = &game.local.social.faction;
    let (stat, c) = if fac.known { (logic::faction_name(fac.status), if logic::is_criminal(fac.status) { BAD } else if fac.status == 1 { GOOD } else { GOLD_TEXT }) } else { ("—", MUTED) };
    text("Estado:", r.x, y + 15.0 * u, 15.0 * u, MUTED);
    text(stat, r.x + 54.0 * u, y + 15.0 * u, 16.0 * u, c);
    if fac.known && !fac.title.trim().is_empty() {
        text_fit(&format!("Rango {}: {}", fac.rank, fac.title), r.x + 170.0 * u, y + 15.0 * u, 15.0 * u, r.right() - r.x - 170.0 * u, TEXT);
    }
    y += 20.0 * u;
    text(&format!("Puntos: {}", fac.score), r.x, y + 14.0 * u, 14.5 * u, TEXT);
    text(&format!("Ciudadanos matados: {}", fac.citizens_killed), r.x + 94.0 * u, y + 14.0 * u, 14.5 * u, TEXT);
    let ck = format!("Criminales: {}", fac.criminals_killed);
    text(&ck, r.right() - measure(&ck, 14.5 * u).width, y + 14.0 * u, 14.5 * u, TEXT);
    y += 22.0 * u;

    // Vitals line.
    let (hp, mhp, mp, mmp) = (game.stats.hp.get(), game.stats.max_hp.get(), game.stats.mana.get(), game.stats.max_mana.get());
    let third = (r.w - 2.0 * 6.0 * u) / 3.0;
    let l = &game.local;
    meter(Rect::new(r.x, y, third, 16.0 * u), hp as f32, mhp as f32, HP_RED, "Vida", &format!("{hp}/{mhp}"), u);
    meter(Rect::new(r.x + third + 6.0 * u, y, third, 16.0 * u), mp as f32, mmp as f32, MANA_BLUE, "Maná", &format!("{mp}/{mmp}"), u);
    meter(Rect::new(r.x + 2.0 * (third + 6.0 * u), y, third, 16.0 * u), l.sta as f32, l.max_sta.max(1) as f32, Color::new(0.20, 0.62, 0.22, 1.0), "Energía", &format!("{}/{}", l.sta, l.max_sta), u);
    tooltip(&tip, u);
}

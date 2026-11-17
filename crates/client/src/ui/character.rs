//! Window: the hero sheet. A portrait of the real character (body, head and equipment from the
//! graphics) with level seal, experience, vitals and faction on the left; attributes with
//! icons and the 24 skills grouped by trade with their bars and "+" buttons on the right.
#[path = "kit.rs"]
pub(crate) mod kit;
use self::kit::{Icon, INK_DIM};
use super::theme::{FontKind, BAD, GOLD, GOLD_BRIGHT, GOLD_DIM, GOLD_TEXT, GOOD, HP, MANA, STAMINA, TEXT, TEXT_DIM, TEXT_FAINT};
use super::widgets::{measure_with, text_with, tooltip};
use super::WindowId;
use crate::assets::Assets;
use crate::game::Game;
use crate::ui::Ui;
use ao_rules::progression::MAX_SKILL;
use client_core::p3::hud as logic;
use macroquad::prelude::*;

const ATTR_HELP: [&str; 5] = [
    "Daño cuerpo a cuerpo y capacidad de carga.",
    "Esquive y velocidad de ataque.",
    "Maná y poder de los hechizos.",
    "Vida y resistencia.",
    "Comercio, liderazgo y trato con la gente.",
];
const ATTR_ICON: [Icon; 5] = [Icon::Sword, Icon::Feather, Icon::Book, Icon::Shield, Icon::Crown];

/// Skill ids (wire order, 1..=24) grouped by what they are for.
const GROUPS: [(&str, Icon, Color, &[u8]); 5] = [
    ("Combate", Icon::Sword, Color::new(0.78, 0.30, 0.22, 1.0), &[3, 4, 6, 10, 12, 13]),
    ("Magia", Icon::Star, Color::new(0.35, 0.55, 0.95, 1.0), &[1, 5, 16]),
    ("Sigilo y saber", Icon::Eye, Color::new(0.62, 0.45, 0.85, 1.0), &[2, 7, 8]),
    ("Mundo", Icon::Crown, Color::new(0.85, 0.66, 0.25, 1.0), &[9, 11, 14, 15, 24]),
    ("Oficios", Icon::Hammer, Color::new(0.45, 0.72, 0.35, 1.0), &[17, 18, 19, 20, 21, 22, 23]),
];

fn faction_look(status: u8, known: bool) -> (Color, Icon) {
    if !known {
        return (Color::new(0.35, 0.32, 0.28, 1.0), Icon::Shield);
    }
    match status {
        1 => (Color::new(0.30, 0.50, 0.85, 1.0), Icon::Shield),
        3 | 5 => (Color::new(0.28, 0.45, 0.92, 1.0), Icon::Crown),
        2 | 4 => (Color::new(0.62, 0.12, 0.10, 1.0), Icon::Skull),
        _ => (Color::new(0.60, 0.25, 0.15, 1.0), Icon::Skull),
    }
}

/// Draws the content of the window inside `r` (the frame is already drawn).
pub fn draw(ui: &mut Ui, game: &mut Game, assets: &mut Assets, r: Rect) {
    let u = ui.u;
    kit::begin(ui, WindowId::Character);
    let mut tip: Vec<String> = Vec::new();
    let left_w = 236.0 * u;
    let left = Rect::new(r.x + 6.0 * u, r.y + 4.0 * u, left_w, r.h - 8.0 * u);
    let right = Rect::new(left.right() + 22.0 * u, r.y + 4.0 * u, r.right() - left.right() - 28.0 * u, r.h - 8.0 * u);
    hero_column(game, assets, left, u, &mut tip);
    stats_column(game, right, u, &mut tip);
    tooltip(&tip, u);
}

fn hero_column(game: &mut Game, assets: &mut Assets, r: Rect, u: f32, tip: &mut Vec<String>) {
    // Stage: a recess with a warm spotlight where the hero stands.
    let stage = Rect::new(r.x, r.y, r.w, 236.0 * u);
    kit::well(stage, u);
    let feet = vec2(stage.center().x, stage.bottom() - 34.0 * u);
    for i in 0..9 {
        let t = i as f32 / 9.0;
        draw_ellipse(feet.x, feet.y - 60.0 * u, (112.0 - t * 80.0) * u, (96.0 - t * 66.0) * u, 0.0, Color::new(0.95, 0.70, 0.35, 0.028));
    }
    draw_ellipse(feet.x, feet.y, 62.0 * u, 12.0 * u, 0.0, Color::new(0.0, 0.0, 0.0, 0.35));
    draw_rectangle_lines(stage.x, stage.y, stage.w, stage.h, 1.0, Color::new(GOLD_DIM.r, GOLD_DIM.g, GOLD_DIM.b, 0.45));
    if let Some(me) = game.me() {
        let (body, head, eq) = (me.body, me.head, (me.weapon, me.shield, me.helmet));
        kit::hero(assets, body, head, eq, feet.x, feet.y, 3.7 * u, WHITE);
    }
    // Level seal on the corner of the stage.
    kit::seal(stage.x + 34.0 * u, stage.y + 44.0 * u, 24.0 * u, &game.stats.level.to_string(), "Nivel", u);

    // Name.
    let name = game.me().map(|e| e.name.clone()).unwrap_or_default();
    let mut y = stage.bottom() + 12.0 * u;
    let nw = super::hud::title_width(&name, 22.0 * u).min(r.w);
    kit::fit(FontKind::Title, &name, r.center().x - nw / 2.0, y + 22.0 * u, 22.0 * u, r.w, GOLD_BRIGHT);
    y += 36.0 * u;

    // Experience: a thin gold bar with what is missing.
    let (exp, next) = (game.stats.exp as u64, game.stats.next_exp.max(1) as u64);
    let bar = Rect::new(r.x, y + 16.0 * u, r.w, 8.0 * u);
    kit::thin_bar(bar, exp as f32, next as f32, Color::new(0.82, 0.64, 0.18, 1.0));
    draw_rectangle_lines(bar.x, bar.y, bar.w, bar.h, 1.0, GOLD_DIM);
    text_with(FontKind::Title, "EXPERIENCIA", r.x, y + 10.0 * u, 12.0 * u, GOLD_TEXT);
    kit::text_right(FontKind::Body, &format!("{} / {}", kit::thousands(exp), kit::thousands(next)), r.right(), y + 11.0 * u, 13.0 * u, TEXT_DIM);
    if kit::hover(Rect::new(r.x, y, r.w, 28.0 * u)) {
        *tip = vec!["Experiencia".into(), format!("Te faltan {} para el nivel {}.", kit::thousands(next.saturating_sub(exp)), game.stats.level + 1)];
    }
    y += 40.0 * u;

    // Vitals.
    let l = &game.local;
    let vitals = [
        (Icon::Heart, "Vida", game.stats.hp.get() as f32, game.stats.max_hp.get() as f32, HP),
        (Icon::Drop, "Maná", game.stats.mana.get() as f32, game.stats.max_mana.get() as f32, MANA),
        (Icon::Bolt, "Energía", l.sta as f32, l.max_sta.max(1) as f32, STAMINA),
    ];
    for (i, (ic, label, v, max, c)) in vitals.iter().enumerate() {
        let vy = y + i as f32 * 24.0 * u;
        kit::icon(*ic, r.x + 9.0 * u, vy + 9.0 * u, 16.0 * u, *c);
        kit::fit(FontKind::Body, label, r.x + 26.0 * u, vy + 14.0 * u, 14.0 * u, 60.0 * u, TEXT_DIM);
        let b = Rect::new(r.x + 70.0 * u, vy + 4.0 * u, r.w - 70.0 * u - 66.0 * u, 10.0 * u);
        kit::thin_bar(b, *v, *max, *c);
        draw_rectangle_lines(b.x, b.y, b.w, b.h, 1.0, Color::new(GOLD_DIM.r, GOLD_DIM.g, GOLD_DIM.b, 0.6));
        kit::text_right(FontKind::Body, &format!("{}/{}", *v as i32, *max as i32), r.right(), vy + 14.0 * u, 14.0 * u, TEXT);
    }
    y += 78.0 * u;

    // Gold.
    kit::icon(Icon::Coin, r.x + 10.0 * u, y + 10.0 * u, 20.0 * u, Color::new(0.95, 0.78, 0.28, 1.0));
    text_with(FontKind::Title, &kit::thousands(game.local.gold as u64), r.x + 28.0 * u, y + 17.0 * u, 18.0 * u, GOLD_BRIGHT);
    y += 34.0 * u;

    // Faction: heraldic shield, name, rank and record.
    let fac = &game.local.social.faction;
    let (col, sym) = faction_look(fac.status, fac.known);
    kit::emblem(r.x + 28.0 * u, y + 32.0 * u, 46.0 * u, col, sym, u);
    let fname = if fac.known { logic::faction_name(fac.status) } else { "Sin facción" };
    let fcol = if !fac.known { TEXT_DIM } else if logic::is_criminal(fac.status) { BAD } else if fac.status == 1 { GOOD } else { GOLD_TEXT };
    kit::fit(FontKind::Title, fname, r.x + 62.0 * u, y + 20.0 * u, 18.0 * u, r.w - 62.0 * u, fcol);
    if fac.known && !fac.title.trim().is_empty() && fac.title.trim() != fname {
        kit::fit(FontKind::NarrativeItalic, &format!("{} · rango {}", fac.title, fac.rank), r.x + 62.0 * u, y + 40.0 * u, 15.0 * u, r.w - 62.0 * u, TEXT_DIM);
    } else {
        text_with(FontKind::NarrativeItalic, if fac.known { "Sin rango todavía" } else { "El reino aún no te conoce" }, r.x + 62.0 * u, y + 40.0 * u, 15.0 * u, TEXT_FAINT);
    }
    if fac.known {
        text_with(FontKind::Body, &format!("{} puntos · {} criminales · {} ciudadanos", fac.score, fac.criminals_killed, fac.citizens_killed), r.x + 62.0 * u, y + 58.0 * u, 12.0 * u, TEXT_DIM);
    }
    if kit::hover(Rect::new(r.x, y, r.w, 64.0 * u)) && fac.known {
        *tip = vec!["Tu historial".into(), format!("Puntos de facción: {}", fac.score), format!("Criminales abatidos: {}", fac.criminals_killed), format!("Ciudadanos abatidos: {}", fac.citizens_killed)];
    }
}

fn stats_column(game: &mut Game, r: Rect, u: f32, tip: &mut Vec<String>) {
    // ---- attributes: five medallions with the number in front
    let y0 = kit::section("Atributos", r.x, r.y, r.w, u);
    let cw = r.w / 5.0;
    for (i, name) in logic::ATTRIBUTE_NAMES.iter().enumerate() {
        let cx = r.x + cw * (i as f32 + 0.5);
        let cell = Rect::new(r.x + cw * i as f32, y0 - 2.0 * u, cw, 66.0 * u);
        let hv = kit::hover(cell);
        kit::badge_icon(ATTR_ICON[i], cx - 30.0 * u, y0 + 22.0 * u, 15.0 * u, if hv { GOLD_BRIGHT } else { GOLD });
        let v = game.local.attributes[i].to_string();
        kit::text_mid(FontKind::Title, &v, cx + 8.0 * u, y0 + 32.0 * u, 28.0 * u, GOLD_BRIGHT);
        kit::text_mid(FontKind::Body, name, cx, y0 + 56.0 * u, 13.0 * u, if hv { TEXT } else { TEXT_DIM });
        if i > 0 {
            draw_line(cell.x, y0 + 6.0 * u, cell.x, y0 + 54.0 * u, 1.0, Color::new(GOLD_DIM.r, GOLD_DIM.g, GOLD_DIM.b, 0.35));
        }
        if hv {
            *tip = vec![name.to_string(), format!("~{}", ATTR_HELP[i])];
        }
    }

    // ---- skills
    let mut y = y0 + 76.0 * u;
    let free = game.local.free_skill_points;
    text_with(FontKind::Title, "HABILIDADES", r.x, y + 14.0 * u, 18.0 * u, GOLD_TEXT);
    if free > 0 {
        let pulse = 0.5 + 0.5 * (get_time() * 3.0).sin() as f32;
        let label = format!("{free} puntos por repartir");
        let w = measure_with(FontKind::Body, &label, 14.0 * u).width + 26.0 * u;
        let pr = Rect::new(r.right() - w, y - 2.0 * u, w, 22.0 * u);
        draw_rectangle(pr.x - 3.0 * u, pr.y - 3.0 * u, pr.w + 6.0 * u, pr.h + 6.0 * u, Color::new(0.4, 0.9, 0.35, 0.06 + 0.07 * pulse));
        draw_rectangle(pr.x, pr.y, pr.w, pr.h, Color::new(0.12, 0.28, 0.10, 0.95));
        draw_rectangle_lines(pr.x, pr.y, pr.w, pr.h, 1.0, GOOD);
        kit::icon(Icon::Star, pr.x + 12.0 * u, pr.center().y, 12.0 * u, GOOD);
        text_with(FontKind::Body, &label, pr.x + 22.0 * u, pr.center().y + 5.0 * u, 14.0 * u, Color::new(0.85, 1.0, 0.8, 1.0));
    } else {
        kit::text_right(FontKind::NarrativeItalic, "Se ganan al subir de nivel", r.right(), y + 14.0 * u, 15.0 * u, TEXT_FAINT);
    }
    y += 26.0 * u;
    let col_gap = 26.0 * u;
    let col_w = (r.w - col_gap) / 2.0;
    let shift = is_key_down(KeyCode::LeftShift) || is_key_down(KeyCode::RightShift);
    // Left column: Combate, Magia, Sigilo. Right: Mundo, Oficios.
    let layout: [&[usize]; 2] = [&[0, 1, 2], &[3, 4]];
    for (ci, groups) in layout.iter().enumerate() {
        let x = r.x + ci as f32 * (col_w + col_gap);
        let mut cy = y;
        for gi in groups.iter() {
            let (title, ic, gc, ids) = GROUPS[*gi];
            kit::icon(ic, x + 8.0 * u, cy + 9.0 * u, 14.0 * u, gc);
            text_with(FontKind::Title, &title.to_uppercase(), x + 22.0 * u, cy + 14.0 * u, 13.0 * u, kit::alpha(gc, 1.0));
            kit::rule(x + 22.0 * u + measure_with(FontKind::Title, &title.to_uppercase(), 13.0 * u).width + 8.0 * u, cy + 9.0 * u, col_w - 30.0 * u - measure_with(FontKind::Title, &title.to_uppercase(), 13.0 * u).width, kit::alpha(gc, 0.6));
            cy += 22.0 * u;
            for id in ids {
                let rr = Rect::new(x, cy, col_w, 25.0 * u);
                skill_row(game, rr, *id, gc, free, shift, u, tip);
                cy += 25.0 * u;
            }
            cy += 8.0 * u;
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn skill_row(game: &mut Game, rr: Rect, id: u8, gc: Color, free: u16, shift: bool, u: f32, tip: &mut Vec<String>) {
    let v = game.local.skills.get(id as usize - 1).copied().unwrap_or(0);
    let name = logic::skill_name(id);
    let hv = kit::hover(rr);
    let can = free > 0 && v < MAX_SKILL;
    if hv {
        draw_rectangle(rr.x - 4.0 * u, rr.y, rr.w + 8.0 * u, rr.h, Color::new(1.0, 0.85, 0.5, 0.05));
    }
    let btn_w = 26.0 * u;
    kit::fit(FontKind::Body, name, rr.x + 4.0 * u, rr.y + 15.0 * u, 15.0 * u, rr.w - btn_w - 40.0 * u, if hv { TEXT } else { Color::new(0.86, 0.82, 0.74, 1.0) });
    let vs = v.to_string();
    let full = v >= MAX_SKILL;
    kit::text_right(FontKind::Body, &vs, rr.right() - btn_w, rr.y + 15.0 * u, 15.0 * u, if full { GOOD } else if v == 0 { TEXT_FAINT } else { GOLD_BRIGHT });
    let b = Rect::new(rr.x + 4.0 * u, rr.y + 19.0 * u, rr.w - btn_w - 4.0 * u, 3.0 * u);
    kit::thin_bar(b, v as f32, MAX_SKILL as f32, if full { GOOD } else { gc });
    if can {
        let c = vec2(rr.right() - 11.0 * u, rr.y + 12.0 * u);
        if kit::round_btn(c, 9.0 * u, "+", true, true, u) {
            game.assign_skill(id, if shift { 5 } else { 1 });
        }
    }
    if hv {
        *tip = vec![name.to_string(), format!("{v} / {MAX_SKILL}")];
        tip.push(if can {
            "Clic en el + : un punto.   Mayús + clic: cinco.".into()
        } else if full {
            "~Has llegado al máximo.".into()
        } else {
            "~Sin puntos libres: se ganan al subir de nivel.".into()
        });
    }
    let _ = (INK_DIM, GOLD);
}

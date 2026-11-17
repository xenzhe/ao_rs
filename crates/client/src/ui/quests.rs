//! Window: quests (NPC offer, active quests, details). OWNER: Sonnet 3 (docs/P3_CONTRATO.md).
//!
//! Two views: "Activas" (my quests in progress with their progress) and "Oferta" (what the
//! quest NPC I just talked to offers, with requirements and an Accept button). The window
//! opens by itself when an offer arrives (`auto_open`, called each frame by the console).
use crate::assets::Assets;
use crate::game::Game;
use crate::ui::console::wrap;
use crate::ui::theme::{FontKind, BAD, GOLD, GOLD_TEXT, GOOD, MUTED, TEXT};
use crate::ui::widgets::{button, text, text_fit, text_with, tooltip};
use crate::ui::{Ui, WindowId};
use ao_formats::dat::npcs::Npc;
use ao_formats::dat::Table;
use ao_protocol::social::quest::{OfferedQuest, QuestInfo};
use client_core::social::quest::ActiveQuest;
use macroquad::prelude::*;
use std::cell::RefCell;
use std::collections::HashSet;

#[derive(Clone, Copy, PartialEq, Eq)]
enum View {
    Active,
    Offer,
}

struct State {
    view: View,
    slot: u8,
    offered: usize,
    requested: HashSet<(u8, u16)>,
    last_draw: f64,
    /// Slot whose abandon button was pressed once (a second press confirms).
    confirm_abandon: Option<u8>,
    /// Identity of the last offer seen (allocation + length), to tell a new one.
    offer_fp: (usize, usize),
    npcs: Option<Table<Npc>>,
}

thread_local! {
    static STATE: RefCell<State> = RefCell::new(State { view: View::Active, slot: 1, offered: 0, requested: HashSet::new(), last_draw: -10.0, confirm_abandon: None, offer_fp: (0, 0), npcs: None });
}

/// Opens the window when a quest NPC's offer arrives (a new `QuestS2C::Offer`, even from
/// the same NPC: every one is a fresh allocation in the state, which is what is compared).
pub fn auto_open(ui: &mut Ui, game: &Game) {
    let fp = game.local.social.quest.offer.as_ref().map(|(_, q)| (q.as_ptr() as usize, q.len())).unwrap_or((0, 0));
    STATE.with(|s| {
        let mut st = s.borrow_mut();
        if fp != st.offer_fp {
            st.offer_fp = fp;
            if fp.1 > 0 {
                st.view = View::Offer;
                st.offered = 0;
                ui.open(WindowId::Quests);
            } else if st.view == View::Offer {
                st.view = View::Active;
            }
        }
    });
}

fn npc_name(st: &mut State, assets: &Assets, id: u16) -> String {
    let table = st.npcs.get_or_insert_with(|| ao_formats::dat::npcs::load(&assets.root).map(|l| l.data).unwrap_or_default());
    // npcs.dat names carry a role tag ("Makrom <Quest de Dungeon Newbie>"): keep the name.
    table.get(id as u32).map(|n| n.name.split(" <").next().unwrap_or(&n.name).trim().to_string()).unwrap_or_else(|| format!("criatura {id}"))
}

fn obj_name(assets: &Assets, id: u16) -> String {
    assets.objs.get(id as u32).map(|o| o.name.clone()).unwrap_or_else(|| format!("objeto {id}"))
}

fn skill_name(id: u8) -> String {
    ao_formats::dat::balance::Skill::from_id(id as i32).and_then(|s| s.spanish_name()).map(str::to_string).unwrap_or_else(|| format!("habilidad {id}"))
}

/// Draws a wrapped paragraph; returns the y below it.
fn paragraph(s: &str, x: f32, y: f32, w: f32, size: f32, c: Color, max_rows: usize) -> f32 {
    let rows = wrap(s, size, w);
    let cut = rows.len() > max_rows;
    for (i, row) in rows.iter().take(max_rows).enumerate() {
        let shown = if cut && i + 1 == max_rows { format!("{row}...") } else { row.clone() };
        text(&shown, x, y + i as f32 * size * 1.1, size, c);
    }
    y + rows.len().min(max_rows) as f32 * size * 1.1
}

fn heading(s: &str, x: f32, y: f32, u: f32) {
    text_with(FontKind::Title, s, x, y, 17.0 * u, GOLD_TEXT);
    draw_line(x, y + 4.0 * u, x + 150.0 * u, y + 4.0 * u, 1.0, GOLD);
}

/// One goal line: `label` at `x`, `right` at the right edge. Done goals are green.
fn goal_row(x: f32, y: f32, w: f32, u: f32, label: &str, right: &str, done: bool) {
    let c = if done { GOOD } else { TEXT };
    text_fit(label, x, y + 15.0 * u, 16.0 * u, w - 70.0 * u, c);
    text(right, x + w - 62.0 * u, y + 15.0 * u, 16.0 * u, c);
}

/// The goals of a quest from `y` down (with my progress when `progress` is given);
/// returns the y below them.
#[allow(clippy::too_many_arguments)]
fn goals(game: &Game, assets: &mut Assets, st: &mut State, info: &QuestInfo, progress: Option<&ActiveQuest>, r: Rect, mut y: f32, u: f32) -> f32 {
    let right = |have: u32, n: u32| if progress.is_some() { format!("{}/{n}", have.min(n)) } else { format!("x{n}") };
    for (i, (npc, n)) in info.kill.iter().enumerate() {
        let killed = progress.and_then(|p| p.killed.get(i)).copied().unwrap_or(0) as u32;
        let name = npc_name(st, assets, *npc);
        goal_row(r.x + 4.0 * u, y, r.w - 4.0 * u, u, &format!("Matar: {name}"), &right(killed, *n as u32), progress.is_some() && killed >= *n as u32);
        y += 20.0 * u;
    }
    for (i, (obj, n)) in info.objs.iter().enumerate() {
        let have = progress.and_then(|p| p.have.get(i)).copied().unwrap_or(0) as u32;
        assets.draw_obj(*obj, Rect::new(r.x + 4.0 * u, y - 2.0 * u, 22.0 * u, 22.0 * u));
        goal_row(r.x + 30.0 * u, y, r.w - 30.0 * u, u, &format!("Entregar: {}", obj_name(assets, *obj)), &right(have, *n as u32), progress.is_some() && have >= *n as u32);
        y += 24.0 * u;
    }
    for (i, sp) in info.spells.iter().enumerate() {
        let known = progress.and_then(|p| p.known.get(i)).copied().unwrap_or(false);
        let name = game.spell_by_id(*sp).map(|s| s.name.clone()).unwrap_or_else(|| format!("hechizo {sp}"));
        let c = if known { GOOD } else { TEXT };
        text_fit(&format!("Conocer el hechizo {name}"), r.x + 4.0 * u, y + 15.0 * u, 16.0 * u, r.w - 10.0 * u, c);
        y += 20.0 * u;
    }
    if info.skill.0 > 0 {
        let have = game.local.skills.get(info.skill.0 as usize - 1).copied().unwrap_or(0);
        let ok = have >= info.skill.1;
        text_fit(&format!("{} {} (tienes {have})", skill_name(info.skill.0), info.skill.1), r.x + 4.0 * u, y + 15.0 * u, 16.0 * u, r.w - 10.0 * u, if ok { GOOD } else { TEXT });
        y += 20.0 * u;
    }
    if info.talk_to != 0 {
        let name = npc_name(st, assets, info.talk_to);
        text_fit(&format!("Habla con {name} (mapa {})", info.pos_map), r.x + 4.0 * u, y + 15.0 * u, 16.0 * u, r.w - 10.0 * u, TEXT);
        y += 20.0 * u;
    }
    if info.kill.is_empty() && info.objs.is_empty() && info.spells.is_empty() && info.skill.0 == 0 && info.talk_to == 0 {
        text("Sin objetivos: habla con el NPC.", r.x + 4.0 * u, y + 15.0 * u, 16.0 * u, MUTED);
        y += 20.0 * u;
    }
    y
}

fn rewards(game: &Game, assets: &mut Assets, info: &QuestInfo, r: Rect, y: f32, u: f32) -> f32 {
    let mut x = r.x + 4.0 * u;
    if info.reward_exp > 0 {
        text(&format!("{} exp", info.reward_exp), x, y + 14.0 * u, 16.0 * u, Color::new(0.6, 0.85, 1.0, 1.0));
        x += 84.0 * u;
    }
    if info.reward_gold > 0 {
        text(&format!("{} oro", info.reward_gold), x, y + 14.0 * u, 16.0 * u, Color::new(1.0, 0.85, 0.3, 1.0));
        x += 84.0 * u;
    }
    for sp in &info.reward_spells {
        let name = game.spell_by_id(*sp).map(|s| s.name.clone()).unwrap_or_else(|| format!("hechizo {sp}"));
        text(&name, x, y + 14.0 * u, 16.0 * u, TEXT);
        x += 90.0 * u;
    }
    let mut ny = y + 22.0 * u;
    if !info.reward_objs.is_empty() {
        let mut ox = r.x + 4.0 * u;
        for (obj, n) in &info.reward_objs {
            let sr = Rect::new(ox, ny, 38.0 * u, 38.0 * u);
            let (hover, _) = crate::ui::widgets::slot(sr, false, u);
            assets.draw_obj(*obj, Rect::new(sr.x + 3.0 * u, sr.y + 3.0 * u, sr.w - 6.0 * u, sr.h - 6.0 * u));
            if *n > 1 {
                text(&n.to_string(), sr.x + 3.0 * u, sr.bottom() - 3.0 * u, 14.0 * u, WHITE);
            }
            if hover {
                tooltip(&[obj_name(assets, *obj), format!("Cantidad: {n}")], u);
            }
            ox += 42.0 * u;
        }
        ny += 42.0 * u;
    }
    ny
}

fn state_label(state: u8) -> (&'static str, Color) {
    match state {
        0 => ("Disponible", GOOD),
        1 => ("En curso", GOLD_TEXT),
        2 => ("Completada", MUTED),
        _ => ("No cumples los requisitos", BAD),
    }
}

fn list_row(r: Rect, selected: bool, title: &str, sub: &str, sub_color: Color, u: f32) -> bool {
    let hover = r.contains(mouse_position().into());
    draw_rectangle(r.x, r.y, r.w, r.h, if selected { Color::new(0.32, 0.22, 0.08, 0.95) } else if hover { Color::new(0.2, 0.14, 0.07, 0.9) } else { Color::new(0.0, 0.0, 0.0, 0.3) });
    draw_rectangle_lines(r.x, r.y, r.w, r.h, 1.0, if selected { GOLD_TEXT } else { GOLD });
    text_fit(title, r.x + 8.0 * u, r.y + 17.0 * u, 17.0 * u, r.w - 14.0 * u, if selected { GOLD_TEXT } else { TEXT });
    text_fit(sub, r.x + 8.0 * u, r.y + 33.0 * u, 14.0 * u, r.w - 14.0 * u, sub_color);
    hover && is_mouse_button_pressed(MouseButton::Left)
}

fn active_done(a: &ActiveQuest) -> bool {
    a.info.kill.iter().enumerate().all(|(i, (_, n))| a.killed.get(i).copied().unwrap_or(0) >= *n)
        && a.info.objs.iter().enumerate().all(|(i, (_, n))| a.have.get(i).copied().unwrap_or(0) >= *n)
        && a.known.iter().all(|k| *k)
}

/// Draws the content of the window inside `r` (the frame is already drawn).
pub fn draw(ui: &mut Ui, game: &mut Game, assets: &mut Assets, r: Rect) {
    let u = ui.u;
    STATE.with(|s| {
        let mut st = s.borrow_mut();
        let st = &mut *st;
        // Reopened (not drawn for a moment): refresh from the server.
        let ask = !crate::ui::console::demo_on();
        if game.now - st.last_draw > 0.6 {
            if ask {
                game.quest_list();
            }
            st.requested.clear();
            st.confirm_abandon = None;
        }
        st.last_draw = game.now;
        for (slot, id) in game.quest_missing_details() {
            if st.requested.insert((slot, id)) && ask {
                game.quest_details(slot);
            }
        }
        let offer = game.local.social.quest.offer.clone();
        let offer_n = offer.as_ref().map(|o| o.1.len()).unwrap_or(0);
        if offer_n == 0 && st.view == View::Offer {
            st.view = View::Active;
        }

        // ---- left column: view tabs and the list
        let left = Rect::new(r.x, r.y, 196.0 * u, r.h);
        let n_active = game.local.social.quest.active.len();
        let tw = if offer_n > 0 { (left.w - 4.0 * u) / 2.0 } else { left.w };
        if button(Rect::new(left.x, left.y, tw, 26.0 * u), &format!("Activas ({n_active})"), st.view == View::Active, u) {
            st.view = View::Active;
        }
        if offer_n > 0 && button(Rect::new(left.x + tw + 4.0 * u, left.y, tw, 26.0 * u), &format!("Oferta ({offer_n})"), st.view == View::Offer, u) {
            st.view = View::Offer;
        }
        let mut y = left.y + 34.0 * u;
        let right = Rect::new(left.right() + 12.0 * u, r.y, r.w - left.w - 12.0 * u, r.h);
        draw_line(left.right() + 6.0 * u, r.y, left.right() + 6.0 * u, r.bottom(), 1.0, GOLD);

        match st.view {
            View::Active => {
                let active = game.local.social.quest.active.clone();
                if active.is_empty() {
                    text("No tienes misiones en curso.", left.x, y + 16.0 * u, 16.0 * u, MUTED);
                    text("Habla con un NPC con", left.x, y + 40.0 * u, 15.0 * u, MUTED);
                    text("un símbolo sobre la cabeza.", left.x, y + 56.0 * u, 15.0 * u, MUTED);
                }
                if !(1..=active.len() as u8).contains(&st.slot) {
                    st.slot = 1;
                }
                for (i, id) in active.iter().enumerate() {
                    let slot = i as u8 + 1;
                    let (title, sub, c) = match game.local.social.quest.quest_in(slot) {
                        Some(a) if active_done(a) => (a.info.name.clone(), "Lista para entregar".to_string(), GOOD),
                        Some(a) => (a.info.name.clone(), "En curso".to_string(), GOLD_TEXT),
                        None => (format!("Misión {id}"), "Cargando...".to_string(), MUTED),
                    };
                    if list_row(Rect::new(left.x, y, left.w, 42.0 * u), st.slot == slot, &title, &sub, c, u) {
                        st.slot = slot;
                        st.confirm_abandon = None;
                    }
                    y += 46.0 * u;
                }
                if let Some(a) = game.local.social.quest.quest_in(st.slot).cloned().filter(|_| !active.is_empty()) {
                    active_details(game, assets, st, &a, right, u);
                }
            }
            View::Offer => {
                let Some((_, quests)) = offer else { return };
                st.offered = st.offered.min(quests.len().saturating_sub(1));
                for (i, q) in quests.iter().enumerate() {
                    let (label, c) = state_label(q.state);
                    if list_row(Rect::new(left.x, y, left.w, 42.0 * u), st.offered == i, &q.info.name, label, c, u) {
                        st.offered = i;
                    }
                    y += 46.0 * u;
                }
                if let Some(q) = quests.get(st.offered) {
                    offer_details(game, assets, st, q, &quests, right, u);
                }
            }
        }
    });
}

fn title_block(info: &QuestInfo, r: Rect, u: f32) -> f32 {
    text_with(FontKind::Title, &info.name, r.x, r.y + 20.0 * u, 22.0 * u, GOLD_TEXT);
    paragraph(&info.desc, r.x, r.y + 42.0 * u, r.w, 16.0 * u, TEXT, 5) + 8.0 * u
}

fn active_details(game: &mut Game, assets: &mut Assets, st: &mut State, a: &ActiveQuest, r: Rect, u: f32) {
    let mut y = title_block(&a.info, r, u);
    heading("Objetivos", r.x, y + 12.0 * u, u);
    y += 22.0 * u;
    y = goals(game, assets, st, &a.info, Some(a), r, y, u) + 8.0 * u;
    if active_done(a) {
        let who = if a.info.talk_to != 0 { npc_name(st, assets, a.info.talk_to) } else { "quien te la dio".into() };
        text_fit(&format!("¡Completada! Entrégala a {who}."), r.x + 4.0 * u, y + 12.0 * u, 17.0 * u, r.w, GOOD);
        y += 22.0 * u;
    }
    heading("Recompensas", r.x, y + 14.0 * u, u);
    rewards(game, assets, &a.info, r, y + 22.0 * u, u);
    // Abandon (two presses).
    let b = Rect::new(r.right() - 150.0 * u, r.bottom() - 28.0 * u, 150.0 * u, 26.0 * u);
    let sure = st.confirm_abandon == Some(st.slot);
    if button(b, if sure { "¿Seguro? Pulsa otra vez" } else { "Abandonar misión" }, sure, u) {
        if sure {
            game.quest_abandon(st.slot);
            st.confirm_abandon = None;
        } else {
            st.confirm_abandon = Some(st.slot);
        }
    }
    if b.contains(mouse_position().into()) {
        tooltip(&["Abandonar la misión".into(), "Pierdes todo el progreso.".into()], u);
    }
}

fn offer_details(game: &mut Game, assets: &mut Assets, st: &mut State, q: &OfferedQuest, all: &[OfferedQuest], r: Rect, u: f32) {
    let info = &q.info;
    let mut y = title_block(info, r, u);
    heading("Requisitos", r.x, y + 12.0 * u, u);
    y += 22.0 * u;
    let lvl = game.stats.level;
    let mut req: Vec<(String, bool)> = vec![(format!("Nivel {} o más (tienes {lvl})", info.required_level), lvl >= info.required_level as u32)];
    if info.limit_level > 0 {
        req.push((format!("Nivel {} como máximo", info.limit_level), lvl <= info.limit_level as u32));
    }
    if info.required_quest != 0 {
        let name = all.iter().find(|o| o.info.id == info.required_quest).map(|o| o.info.name.clone()).unwrap_or_else(|| format!("la misión {}", info.required_quest));
        req.push((format!("Haber completado: {name}"), game.local.social.quest.done.contains(&info.required_quest)));
    }
    if !info.classes.is_empty() {
        let names: Vec<_> = info.classes.iter().filter_map(|c| ao_rules::character::CLASSES.iter().find(|(id, _)| id == c).map(|(_, n)| *n)).collect();
        req.push((format!("Clase: {}", names.join(", ")), true));
    }
    for (text_, ok) in &req {
        text_fit(text_, r.x + 4.0 * u, y + 14.0 * u, 16.0 * u, r.w - 8.0 * u, if *ok { GOOD } else { BAD });
        y += 19.0 * u;
    }
    y += 6.0 * u;
    heading("Objetivos", r.x, y + 12.0 * u, u);
    y += 22.0 * u;
    y = goals(game, assets, st, info, None, r, y, u) + 6.0 * u;
    heading("Recompensas", r.x, y + 12.0 * u, u);
    rewards(game, assets, info, r, y + 20.0 * u, u);
    let b = Rect::new(r.right() - 150.0 * u, r.bottom() - 28.0 * u, 150.0 * u, 26.0 * u);
    if q.state == 0 {
        if button(b, "Aceptar misión", true, u) {
            game.quest_accept(info.id);
        }
    } else {
        let (label, c) = state_label(q.state);
        draw_rectangle(b.x, b.y, b.w, b.h, Color::new(0.0, 0.0, 0.0, 0.4));
        text_fit(label, b.x + 6.0 * u, b.y + 18.0 * u, 15.0 * u, b.w - 10.0 * u, c);
    }
}

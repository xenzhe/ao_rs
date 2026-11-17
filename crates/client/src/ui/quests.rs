//! Window: the quest diary. The list of quests on the left (what is in progress, what the NPC
//! offers), the chosen one on a parchment page with its story, objectives ticked off as you
//! go and the rewards with their icons.
//!
//! Two views: "Activas" (my quests in progress) and "Oferta" (what the quest NPC I just
//! talked to offers, with requirements and an Accept button). The window opens by itself
//! when an offer arrives (`auto_open`, called each frame by the console).
use crate::assets::Assets;
use crate::game::Game;
use crate::ui::character::kit::{self, Icon, INK_DIM, WAX};
use crate::ui::theme::{FontKind, BAD, GOLD, GOLD_BRIGHT, GOLD_DIM, GOLD_TEXT, GOOD, INK, TEXT, TEXT_DIM, TEXT_FAINT};
use crate::ui::widgets::{button_kind, text_with, tooltip, Kind};
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
    /// Slot whose abandon link was pressed once (a second press confirms).
    confirm_abandon: Option<u8>,
    /// Identity of the last offer seen (allocation + length), to tell a new one.
    offer_fp: (usize, usize),
    npcs: Option<Table<Npc>>,
    scroll: usize,
}

thread_local! {
    static STATE: RefCell<State> = RefCell::new(State { view: View::Active, slot: 1, offered: 0, requested: HashSet::new(), last_draw: -10.0, confirm_abandon: None, offer_fp: (0, 0), npcs: None, scroll: 0 });
}

const INK_GOOD: Color = Color::new(0.16, 0.40, 0.13, 1.0);
const INK_BAD: Color = Color::new(0.64, 0.10, 0.08, 1.0);

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

/// One line of the objectives list.
struct Goal {
    label: String,
    have: u32,
    need: u32,
    /// Show "have/need" (a quest in progress) or just "xN" (an offer).
    progress: bool,
    done: bool,
    obj: Option<u16>,
}

fn build_goals(game: &Game, assets: &Assets, st: &mut State, info: &QuestInfo, progress: Option<&ActiveQuest>) -> Vec<Goal> {
    let mut v = Vec::new();
    for (i, (npc, n)) in info.kill.iter().enumerate() {
        let killed = progress.and_then(|p| p.killed.get(i)).copied().unwrap_or(0) as u32;
        v.push(Goal { label: format!("Abatir: {}", npc_name(st, assets, *npc)), have: killed.min(*n as u32), need: *n as u32, progress: progress.is_some(), done: progress.is_some() && killed >= *n as u32, obj: None });
    }
    for (i, (obj, n)) in info.objs.iter().enumerate() {
        let have = progress.and_then(|p| p.have.get(i)).copied().unwrap_or(0) as u32;
        v.push(Goal { label: format!("Entregar: {}", obj_name(assets, *obj)), have: have.min(*n as u32), need: *n as u32, progress: progress.is_some(), done: progress.is_some() && have >= *n as u32, obj: Some(*obj) });
    }
    for (i, sp) in info.spells.iter().enumerate() {
        let known = progress.and_then(|p| p.known.get(i)).copied().unwrap_or(false);
        let name = game.spell_by_id(*sp).map(|s| s.name.clone()).unwrap_or_else(|| format!("hechizo {sp}"));
        v.push(Goal { label: format!("Aprender el hechizo {name}"), have: known as u32, need: 1, progress: false, done: known, obj: None });
    }
    if info.skill.0 > 0 {
        let have = game.local.skills.get(info.skill.0 as usize - 1).copied().unwrap_or(0);
        v.push(Goal { label: format!("{} {} (tienes {have})", skill_name(info.skill.0), info.skill.1), have: 0, need: 0, progress: false, done: have >= info.skill.1, obj: None });
    }
    if info.talk_to != 0 {
        v.push(Goal { label: format!("Habla con {} (mapa {})", npc_name(st, assets, info.talk_to), info.pos_map), have: 0, need: 0, progress: false, done: false, obj: None });
    }
    if v.is_empty() {
        v.push(Goal { label: "Sin objetivos: habla con el NPC.".into(), have: 0, need: 0, progress: false, done: false, obj: None });
    }
    v
}

/// Overall fraction done of a quest in progress (counts what has numbers).
fn fraction(goals: &[Goal]) -> f32 {
    let (mut have, mut need) = (0u32, 0u32);
    for g in goals.iter().filter(|g| g.need > 0) {
        have += g.have;
        need += g.need;
    }
    if need == 0 { 0.0 } else { have as f32 / need as f32 }
}

fn state_label(state: u8) -> (&'static str, Color, Icon) {
    match state {
        0 => ("Disponible", GOOD, Icon::Scroll),
        1 => ("En curso", GOLD_TEXT, Icon::Hourglass),
        2 => ("Completada", TEXT_DIM, Icon::Check),
        _ => ("No cumples los requisitos", BAD, Icon::Cross),
    }
}

fn active_done(a: &ActiveQuest) -> bool {
    a.info.kill.iter().enumerate().all(|(i, (_, n))| a.killed.get(i).copied().unwrap_or(0) >= *n)
        && a.info.objs.iter().enumerate().all(|(i, (_, n))| a.have.get(i).copied().unwrap_or(0) >= *n)
        && a.known.iter().all(|k| *k)
}

/// Draws the content of the window inside `r` (the frame is already drawn).
pub fn draw(ui: &mut Ui, game: &mut Game, assets: &mut Assets, r: Rect) {
    let u = ui.u;
    kit::begin(ui, WindowId::Quests);
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

        let r = Rect::new(r.x + 6.0 * u, r.y + 2.0 * u, r.w - 12.0 * u, r.h - 4.0 * u);
        // ---- left: the index of the diary
        let left = Rect::new(r.x, r.y, 252.0 * u, r.h);
        let n_active = game.local.social.quest.active.len();
        let mut labels = vec![format!("Diario ({n_active})")];
        if offer_n > 0 {
            labels.push(format!("Oferta ({offer_n})"));
        }
        let cur = if st.view == View::Offer { 1 } else { 0 };
        if let Some(i) = kit::tabs(left.x + 4.0 * u, left.y, &labels, cur, u) {
            st.view = if i == 1 { View::Offer } else { View::Active };
        }
        kit::rule(left.x, left.y + 30.0 * u, left.w, GOLD);
        let right = Rect::new(left.right() + 22.0 * u, r.y, r.w - left.w - 22.0 * u, r.h);
        draw_line(left.right() + 11.0 * u, r.y + 36.0 * u, left.right() + 11.0 * u, r.bottom() - 4.0 * u, 1.0, kit::alpha(GOLD, 0.35));
        let list = Rect::new(left.x, left.y + 40.0 * u, left.w, left.h - 40.0 * u);
        let rh = 62.0 * u;
        let vis = ((list.h / rh).floor() as usize).max(1);

        match st.view {
            View::Active => {
                let active = game.local.social.quest.active.clone();
                if active.is_empty() {
                    kit::empty(list, "Tu diario está vacío.", "Habla con quien lleve un signo sobre la cabeza.", u);
                }
                if !(1..=active.len() as u8).contains(&st.slot) {
                    st.slot = 1;
                }
                st.scroll = kit::wheel(list, st.scroll, active.len(), vis, 1);
                for (i, id) in active.iter().enumerate().skip(st.scroll).take(vis) {
                    let slot = i as u8 + 1;
                    let row = Rect::new(list.x, list.y + (i - st.scroll) as f32 * rh, list.w - 8.0 * u, rh - 4.0 * u);
                    let (_, click) = kit::row(row, st.slot == slot, u);
                    let (title, sub, c, frac, ic) = match game.local.social.quest.quest_in(slot).cloned() {
                        Some(a) => {
                            let goals = build_goals(game, assets, st, &a.info, Some(&a));
                            if active_done(&a) {
                                (a.info.name.clone(), "Lista para entregar".to_string(), GOOD, 1.0, Icon::Check)
                            } else {
                                (a.info.name.clone(), "En curso".to_string(), GOLD_TEXT, fraction(&goals), Icon::Hourglass)
                            }
                        }
                        None => (format!("Misión {id}"), "Leyendo el diario...".to_string(), TEXT_DIM, 0.0, Icon::Scroll),
                    };
                    list_row(row, &title, &sub, c, frac, ic, st.slot == slot, u);
                    if click {
                        st.slot = slot;
                        st.confirm_abandon = None;
                    }
                }
                kit::scrollbar(list, active.len(), vis, st.scroll, u);
                if let Some(a) = game.local.social.quest.quest_in(st.slot).cloned().filter(|_| !active.is_empty()) {
                    active_details(game, assets, st, &a, right, u);
                } else if active.is_empty() {
                    kit::page(right, u);
                    kit::empty_ink(right, "Aquí se irán escribiendo tus hazañas.", "", u);
                }
            }
            View::Offer => {
                let Some((_, quests)) = offer else { return };
                st.offered = st.offered.min(quests.len().saturating_sub(1));
                for (i, q) in quests.iter().enumerate().take(vis) {
                    let row = Rect::new(list.x, list.y + i as f32 * rh, list.w - 8.0 * u, rh - 4.0 * u);
                    let (label, c, ic) = state_label(q.state);
                    let (_, click) = kit::row(row, st.offered == i, u);
                    list_row(row, &q.info.name, label, c, 0.0, ic, st.offered == i, u);
                    if click {
                        st.offered = i;
                    }
                }
                if let Some(q) = quests.get(st.offered) {
                    offer_details(game, assets, st, q, &quests, right, u);
                }
            }
        }
    });
}

#[allow(clippy::too_many_arguments)]
fn list_row(r: Rect, title: &str, sub: &str, c: Color, frac: f32, ic: Icon, selected: bool, u: f32) {
    kit::icon(ic, r.x + 20.0 * u, r.y + 20.0 * u, 16.0 * u, c);
    kit::fit(FontKind::Title, title, r.x + 38.0 * u, r.y + 24.0 * u, 16.0 * u, r.w - 44.0 * u, if selected { GOLD_BRIGHT } else { TEXT });
    text_with(FontKind::Body, sub, r.x + 38.0 * u, r.y + 42.0 * u, 13.0 * u, c);
    if frac > 0.0 && frac < 1.0 {
        let b = Rect::new(r.x + 38.0 * u, r.y + 48.0 * u, r.w - 52.0 * u, 3.0 * u);
        kit::thin_bar(b, frac, 1.0, GOLD);
    }
}

/// Title, story and objectives of a quest on the parchment `page`; returns the y under them.
#[allow(clippy::too_many_arguments)]
fn story(info: &QuestInfo, goals: &[Goal], assets: &mut Assets, page: Rect, show_progress: bool, stop_y: f32, u: f32) -> f32 {
    let px = page.x + 28.0 * u;
    let pw = page.w - 56.0 * u;
    kit::ink_fit(FontKind::Title, &info.name, px, page.y + 46.0 * u, 26.0 * u, pw, Color::new(0.32, 0.06, 0.04, 1.0));
    kit::rule(px, page.y + 58.0 * u, pw, Color::new(0.35, 0.2, 0.08, 0.9));
    let mut y = kit::paragraph(FontKind::Narrative, &info.desc, px, page.y + 66.0 * u, pw, 17.0 * u, INK, 5);
    y += 10.0 * u;
    kit::ink(FontKind::Title, "OBJETIVOS", px, y + 14.0 * u, 13.0 * u, WAX);
    y += 24.0 * u;
    let rows_fit = (((stop_y - y) / (28.0 * u)).floor() as usize).max(1);
    for (i, g) in goals.iter().enumerate() {
        if i >= rows_fit {
            kit::ink(FontKind::NarrativeItalic, &format!("... y {} más", goals.len() - i), px + 30.0 * u, y + 18.0 * u, 15.0 * u, INK_DIM);
            y += 24.0 * u;
            break;
        }
        let row = Rect::new(px, y, pw, 28.0 * u);
        kit::tick(row.x + 11.0 * u, row.center().y, 9.0 * u, g.done, true);
        let mut tx = row.x + 30.0 * u;
        if let Some(obj) = g.obj {
            let sq = Rect::new(tx, row.center().y - 11.0 * u, 22.0 * u, 22.0 * u);
            draw_rectangle(sq.x, sq.y, sq.w, sq.h, Color::new(0.12, 0.08, 0.04, 0.85));
            assets.draw_obj(obj, Rect::new(sq.x + 1.0, sq.y + 1.0, sq.w - 2.0, sq.h - 2.0));
            tx += 28.0 * u;
        }
        let right_w = if g.need > 0 { 78.0 * u } else { 0.0 };
        let c = if g.done { INK_GOOD } else { INK };
        kit::ink_fit(FontKind::Narrative, &g.label, tx, row.center().y + 6.0 * u, 17.0 * u, row.right() - tx - right_w, c);
        if g.need > 0 {
            let s = if g.progress { format!("{}/{}", g.have, g.need) } else { format!("x{}", g.need) };
            let w = crate::ui::widgets::measure_with(FontKind::Body, &s, 15.0 * u).width;
            kit::ink(FontKind::Body, &s, row.right() - w, row.center().y + 5.0 * u, 15.0 * u, c);
            if g.progress && show_progress {
                let b = Rect::new(row.right() - 70.0 * u, row.bottom() - 5.0 * u, 70.0 * u, 3.0 * u);
                draw_rectangle(b.x, b.y, b.w, b.h, Color::new(0.35, 0.22, 0.1, 0.25));
                draw_rectangle(b.x, b.y, b.w * (g.have as f32 / g.need as f32).clamp(0.0, 1.0), b.h, if g.done { INK_GOOD } else { Color::new(0.62, 0.40, 0.10, 1.0) });
            }
        }
        y += 28.0 * u;
    }
    y
}

/// The rewards strip at the bottom of the page: experience, gold, spells and objects.
fn rewards(game: &Game, assets: &mut Assets, info: &QuestInfo, page: Rect, y: f32, u: f32, tip: &mut Vec<String>) {
    let px = page.x + 28.0 * u;
    let pw = page.w - 56.0 * u;
    kit::ink(FontKind::Title, "RECOMPENSAS", px, y + 14.0 * u, 13.0 * u, WAX);
    kit::rule(px + 100.0 * u, y + 9.0 * u, pw - 100.0 * u, Color::new(0.35, 0.2, 0.08, 0.7));
    let mut x = px;
    let cy = y + 44.0 * u;
    let chip = |x: &mut f32, ic: Icon, c: Color, label: &str, ink_c: Color| {
        kit::icon(ic, *x + 11.0 * u, cy, 20.0 * u, c);
        let w = crate::ui::widgets::measure_with(FontKind::Title, label, 17.0 * u).width;
        kit::ink(FontKind::Title, label, *x + 28.0 * u, cy + 6.0 * u, 17.0 * u, ink_c);
        *x += 38.0 * u + w + 14.0 * u;
    };
    if info.reward_exp > 0 {
        chip(&mut x, Icon::Star, Color::new(0.20, 0.42, 0.78, 1.0), &format!("{} exp", kit::thousands(info.reward_exp as u64)), Color::new(0.14, 0.28, 0.58, 1.0));
    }
    if info.reward_gold > 0 {
        chip(&mut x, Icon::Coin, Color::new(0.78, 0.55, 0.10, 1.0), &format!("{} oro", kit::thousands(info.reward_gold as u64)), Color::new(0.50, 0.34, 0.05, 1.0));
    }
    for sp in &info.reward_spells {
        let name = game.spell_by_id(*sp).map(|s| s.name.clone()).unwrap_or_else(|| format!("hechizo {sp}"));
        chip(&mut x, Icon::Book, Color::new(0.45, 0.20, 0.55, 1.0), &name, Color::new(0.30, 0.12, 0.40, 1.0));
    }
    let mut ox = x;
    for (obj, n) in &info.reward_objs {
        let sr = Rect::new(ox, cy - 20.0 * u, 40.0 * u, 40.0 * u);
        draw_rectangle(sr.x, sr.y, sr.w, sr.h, Color::new(0.12, 0.08, 0.04, 0.9));
        draw_rectangle_lines(sr.x, sr.y, sr.w, sr.h, 1.5 * u, Color::new(0.45, 0.30, 0.12, 1.0));
        assets.draw_obj(*obj, Rect::new(sr.x + 3.0 * u, sr.y + 3.0 * u, sr.w - 6.0 * u, sr.h - 6.0 * u));
        if *n > 1 {
            text_with(FontKind::Body, &n.to_string(), sr.x + 4.0 * u, sr.bottom() - 4.0 * u, 13.0 * u, WHITE);
        }
        if kit::hover(sr) {
            *tip = vec![obj_name(assets, *obj), format!("Cantidad: {n}")];
        }
        ox += 46.0 * u;
    }
}

fn active_details(game: &mut Game, assets: &mut Assets, st: &mut State, a: &ActiveQuest, right: Rect, u: f32) {
    let mut tip: Vec<String> = Vec::new();
    kit::page(right, u);
    let goals = build_goals(game, assets, st, &a.info, Some(a));
    let rewards_y = right.bottom() - 138.0 * u;
    let end = story(&a.info, &goals, assets, right, true, rewards_y - 40.0 * u, u);
    let px = right.x + 28.0 * u;
    if active_done(a) {
        let who = if a.info.talk_to != 0 { npc_name(st, assets, a.info.talk_to) } else { "quien te la dio".into() };
        let y = end.min(rewards_y - 44.0 * u) + 6.0 * u;
        draw_rectangle(px - 6.0 * u, y, right.w - 44.0 * u, 28.0 * u, Color::new(0.20, 0.45, 0.16, 0.18));
        kit::icon(Icon::Check, px + 10.0 * u, y + 14.0 * u, 16.0 * u, INK_GOOD);
        kit::ink_fit(FontKind::Title, &format!("¡Completada! Entrégala a {who}."), px + 28.0 * u, y + 20.0 * u, 16.0 * u, right.w - 90.0 * u, INK_GOOD);
    }
    rewards(game, assets, &a.info, right, rewards_y, u, &mut tip);
    // Abandon: an ink link pressed twice.
    let sure = st.confirm_abandon == Some(st.slot);
    let b = Rect::new(right.right() - 220.0 * u, right.bottom() - 34.0 * u, 196.0 * u, 22.0 * u);
    let label = if sure { "¿Seguro? Pulsa otra vez" } else { "Abandonar misión" };
    let hv = kit::hover(b);
    let c = if sure || hv { INK_BAD } else { INK_DIM };
    let w = crate::ui::widgets::measure_with(FontKind::Body, label, 15.0 * u).width;
    kit::ink(FontKind::Body, label, b.right() - w, b.y + 15.0 * u, 15.0 * u, c);
    if hv {
        draw_line(b.right() - w, b.y + 19.0 * u, b.right(), b.y + 19.0 * u, 1.0, c);
        tip = vec!["Abandonar la misión".into(), "~Pierdes todo el progreso.".into()];
    }
    if kit::clicked(b) {
        if sure {
            game.quest_abandon(st.slot);
            st.confirm_abandon = None;
        } else {
            st.confirm_abandon = Some(st.slot);
        }
    }
    tooltip(&tip, u);
}

fn offer_details(game: &mut Game, assets: &mut Assets, st: &mut State, q: &OfferedQuest, all: &[OfferedQuest], right: Rect, u: f32) {
    let info = &q.info;
    let mut tip: Vec<String> = Vec::new();
    kit::page(right, u);
    let goals = build_goals(game, assets, st, info, None);
    let rewards_y = right.bottom() - 138.0 * u;
    let px = right.x + 28.0 * u;
    // Requirements first, in ink with ticks.
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
    let req_h = 24.0 * u + req.len() as f32 * 22.0 * u;
    let end = story(info, &goals, assets, right, false, rewards_y - req_h - 20.0 * u, u);
    let mut y = end.max(right.y + 120.0 * u) + 6.0 * u;
    kit::ink(FontKind::Title, "REQUISITOS", px, y + 14.0 * u, 13.0 * u, WAX);
    y += 24.0 * u;
    for (t, ok) in &req {
        kit::icon(if *ok { Icon::Check } else { Icon::Cross }, px + 9.0 * u, y + 10.0 * u, 13.0 * u, if *ok { INK_GOOD } else { INK_BAD });
        kit::ink_fit(FontKind::Narrative, t, px + 28.0 * u, y + 16.0 * u, 16.0 * u, right.w - 90.0 * u, if *ok { INK_GOOD } else { INK_BAD });
        y += 22.0 * u;
    }
    rewards(game, assets, info, right, rewards_y, u, &mut tip);
    let b = Rect::new(right.right() - 214.0 * u, right.bottom() - 48.0 * u, 190.0 * u, 36.0 * u);
    if q.state == 0 {
        if button_kind(b, "Aceptar misión", Kind::Primary, true, u) {
            game.quest_accept(info.id);
        }
    } else {
        let (label, _, _) = state_label(q.state);
        kit::ink(FontKind::NarrativeItalic, label, b.right() - crate::ui::widgets::measure_with(FontKind::NarrativeItalic, label, 16.0 * u).width, b.y + 24.0 * u, 16.0 * u, INK_BAD);
    }
    let _ = (GOLD_DIM, TEXT_FAINT);
    tooltip(&tip, u);
}

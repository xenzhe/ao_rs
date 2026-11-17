//! Window: the workbench. One tab per trade (blacksmith, carpenter, alchemist, tailor; the
//! open one is lit), the recipes the server sent on the left (icon, name, skill, how many the
//! bag covers) and on the right the chosen object on a plinth: its materials against what you
//! carry, the amount and the build button; while an order runs, its progress and a stop button.
//!
//! The server opens the trade (`CraftS2C::Open`) when a tool is used or an anvil clicked;
//! `CraftState::open_request` asks the UI to show this window (the integrator calls
//! [`auto_window`] from `ui::auto_windows`). Clicking a tab asks the server to switch.
use super::character::kit::{self, Icon};
use super::theme::{FontKind, BAD, GOLD, GOLD_BRIGHT, GOLD_DIM, GOLD_TEXT, GOOD, TEXT, TEXT_DIM, TEXT_FAINT};
use super::widgets::{button_kind, measure_with, text_field, text_with, tooltip, Kind};
use crate::assets::Assets;
use crate::game::Game;
use crate::ui::{Ui, WindowId};
use ao_protocol::p4::craft::{Recipe, Trade};
use client_core::p4::craft::affordable;
use macroquad::prelude::*;
use std::cell::RefCell;

const W: WindowId = WindowId::Crafting;
const ROW: f32 = 48.0;

#[derive(Default)]
struct View {
    selected: Option<u16>,
    scroll: usize,
    qty: String,
    error: Option<String>,
    /// `AO_CRAFT` (screenshots) already sent.
    dev_sent: bool,
    dev_since: Option<f64>,
    dev_built: bool,
}

thread_local! {
    static VIEW: RefCell<View> = RefCell::new(View { qty: "1".into(), ..Default::default() });
}

fn view<T>(f: impl FnOnce(&mut View) -> T) -> T {
    VIEW.with(|v| f(&mut v.borrow_mut()))
}

/// For `ui::auto_windows` (integrator): opens the window when the server opens a trade,
/// closes it when the server closes it, and tells the server when the player closed it
/// with its X.
#[allow(dead_code)] // wired by the integrator in `ui::auto_windows`
pub fn auto_window(ui: &mut Ui, game: &mut Game) {
    let st = &mut game.local.p4.craft;
    if std::mem::take(&mut st.close_request) {
        ui.close(W);
        ui.server_windows.remove(&W);
        return;
    }
    if std::mem::take(&mut st.open_request) {
        ui.open(W);
        ui.server_windows.insert(W);
        return;
    }
    if st.open.is_some() && !ui.is_open(W) && ui.server_windows.remove(&W) {
        game.craft_close();
    }
}

/// `AO_CRAFT=carpinteria` / `herreria:34:65`...: opens a trade once (screenshots).
fn dev_open(game: &mut Game) {
    // Wait a moment so `AO_SAY=/ir ...` has placed the character.
    let t = get_time();
    if view(|v| v.dev_sent || t - *v.dev_since.get_or_insert(t) < 1.5) {
        return;
    }
    view(|v| v.dev_sent = true);
    let Ok(spec) = std::env::var("AO_CRAFT") else { return };
    let mut it = spec.split(':');
    let trade = match it.next().unwrap_or("") {
        "herreria" => Trade::Blacksmith,
        "carpinteria" => Trade::Carpenter,
        "alquimia" => Trade::Alchemy,
        "sastreria" => Trade::Tailor,
        _ => return,
    };
    let n: Vec<i32> = it.filter_map(|s| s.parse().ok()).collect();
    let station = (n.len() == 2).then(|| (n[0], n[1]));
    game.craft_open(trade, station);
    if let Ok(sel) = std::env::var("AO_CRAFT_SELECT") {
        view(|v| v.selected = sel.parse().ok());
    }
}

fn tab_icon(trade: Trade) -> u16 {
    // The tool of each trade (obj.dat): hammer, saw, pot, sewing box.
    match trade {
        Trade::Blacksmith => 390,
        Trade::Carpenter => 198,
        Trade::Alchemy => 887,
        Trade::Tailor => 886,
    }
}

fn obj_name(assets: &Assets, obj: u16) -> String {
    assets.objs.get(obj as u32).map(|o| o.name.clone()).filter(|n| !n.is_empty()).unwrap_or_else(|| format!("Objeto {obj}"))
}

/// Tooltip lines of an object (the inventory's: stats and requirements), then `extra`.
fn obj_tip(game: &Game, assets: &Assets, obj: u16, amount: u32, extra: &[String]) -> Vec<String> {
    super::inventory::item_tip(game, assets, obj, amount, false, extra)
}

pub fn draw(ui: &mut Ui, game: &mut Game, assets: &mut Assets, r: Rect) {
    let u = ui.u;
    kit::begin(ui, W);
    dev_open(game);
    let open = game.local.p4.craft.open.clone();
    let mut tip: Vec<String> = Vec::new();
    let r = Rect::new(r.x + 6.0 * u, r.y + 2.0 * u, r.w - 12.0 * u, r.h - 4.0 * u);

    // ---- trade tabs: the tool of each trade with its name, the open one lit
    let tab_h = 36.0 * u;
    let tw = r.w / 4.0;
    for (i, t) in Trade::ALL.into_iter().enumerate() {
        let tr = Rect::new(r.x + i as f32 * tw, r.y, tw, tab_h);
        let on = open.as_ref().is_some_and(|o| o.trade == t);
        let hv = kit::hover(tr);
        if on {
            draw_rectangle(tr.x + 4.0 * u, tr.bottom() - 10.0 * u, tr.w - 8.0 * u, 10.0 * u, Color::new(1.0, 0.8, 0.4, 0.07));
        }
        let ic = Rect::new(tr.x + 10.0 * u, tr.y + 3.0 * u, 28.0 * u, 28.0 * u);
        if !on {
            draw_rectangle(ic.x, ic.y, ic.w, ic.h, Color::new(0.0, 0.0, 0.0, 0.0));
        }
        assets.draw_obj(tab_icon(t), ic);
        let name = t.name().to_uppercase();
        text_with(FontKind::Title, &name, tr.x + 46.0 * u, tr.y + 24.0 * u, 15.0 * u, if on { GOLD_BRIGHT } else if hv { TEXT } else { TEXT_DIM });
        draw_rectangle(tr.x + 4.0 * u, tr.bottom() - 3.0 * u, (tr.w - 8.0 * u) * if on { 1.0 } else if hv { 0.5 } else { 0.0 }, 2.0 * u, if on { GOLD_BRIGHT } else { GOLD_DIM });
        if hv {
            if kit::clicked(tr) && !on {
                let station = if t == Trade::Blacksmith { open.as_ref().and_then(|o| o.station) } else { None };
                game.craft_open(t, station);
            }
            let how = match t {
                Trade::Blacksmith => "Martillo de herrero equipado y clic en un yunque",
                Trade::Carpenter => "Serrucho equipado",
                Trade::Alchemy => "Olla de alquimia equipada",
                Trade::Tailor => "Costurero equipado",
            };
            tip = vec![t.name().to_string(), format!("~{how}")];
        }
    }
    kit::rule(r.x, r.y + tab_h + 2.0 * u, r.w, GOLD);

    let body = Rect::new(r.x, r.y + tab_h + 14.0 * u, r.w, r.h - tab_h - 14.0 * u);
    // `AO_CRAFT_BUILD=n` (screenshots): starts an order of the selected object once.
    if open.is_some() && !view(|v| std::mem::replace(&mut v.dev_built, true)) {
        if let (Some(n), Some(sel)) = (std::env::var("AO_CRAFT_BUILD").ok().and_then(|s| s.parse().ok()), view(|v| v.selected)) {
            view(|v| v.qty = format!("{n}"));
            let _ = game.craft_build(sel, n);
        }
    }
    let Some(open) = open else {
        kit::empty(body, "Ningún oficio abierto.", "Empuña la herramienta de tu oficio y úsala.", u);
        let lines = ["Serrucho, olla de alquimia o costurero: úsalos desde el inventario.", "Para la herrería, con el martillo equipado haz clic en un yunque.", "Solo los trabajadores pueden usar las herramientas."];
        for (i, l) in lines.iter().enumerate() {
            kit::text_mid(FontKind::Body, l, body.center().x, body.center().y + 74.0 * u + i as f32 * 22.0 * u, 15.0 * u, if i == 2 { TEXT_FAINT } else { TEXT_DIM });
        }
        tooltip(&tip, u);
        return;
    };

    // ---- the recipe list
    let list = Rect::new(body.x, body.y, 318.0 * u, body.h);
    let rows = ((list.h / (ROW * u)).floor().max(1.0)) as usize;
    let recipes = open.recipes.clone();
    let scroll = kit::wheel(list, view(|v| v.scroll), recipes.len(), rows, 1);
    let mut selected = view(|v| v.selected).filter(|s| recipes.iter().any(|r| r.obj == *s)).or_else(|| recipes.first().map(|r| r.obj));
    let mine = game.local.skills.get(open.trade.skill()).copied().unwrap_or(0);
    if recipes.is_empty() {
        kit::empty(list, "Todavía no sabes construir nada aquí.", "", u);
    }
    for (k, rc) in recipes.iter().enumerate().skip(scroll).take(rows) {
        let rr = Rect::new(list.x, list.y + (k - scroll) as f32 * ROW * u, list.w - 10.0 * u, (ROW - 3.0) * u);
        let (hov, click) = kit::row(rr, Some(rc.obj) == selected, u);
        let icon = Rect::new(rr.x + 8.0 * u, rr.y + 4.0 * u, 38.0 * u, 38.0 * u);
        kit::well(icon, u);
        assets.draw_obj(rc.obj, Rect::new(icon.x + 3.0 * u, icon.y + 3.0 * u, icon.w - 6.0 * u, icon.h - 6.0 * u));
        let can = affordable(&game.local, rc);
        let locked = mine < rc.skill;
        let badge = if locked { String::new() } else if can == u32::MAX { "∞".to_string() } else { format!("×{}", can.min(9999)) };
        kit::text_right(FontKind::Title, &badge, rr.right() - 10.0 * u, rr.y + 24.0 * u, 18.0 * u, if can > 0 { GOOD } else { TEXT_FAINT });
        kit::fit(FontKind::Title, &obj_name(assets, rc.obj), icon.right() + 10.0 * u, rr.y + 20.0 * u, 16.0 * u, rr.right() - icon.right() - 70.0 * u, if Some(rc.obj) == selected { GOLD_BRIGHT } else { TEXT });
        let (sc, st) = if locked { (BAD, format!("Pide {} {} (tienes {mine})", open.trade.name(), rc.skill)) } else { (TEXT_DIM, format!("{} {}", open.trade.name(), rc.skill)) };
        text_with(FontKind::Body, &st, icon.right() + 10.0 * u, rr.y + 38.0 * u, 13.0 * u, sc);
        if click {
            selected = Some(rc.obj);
        }
        if hov {
            tip = obj_tip(game, assets, rc.obj, 1, &[format!("Requiere {} {}", open.trade.name(), rc.skill)]);
        }
    }
    kit::scrollbar(list, recipes.len(), rows, scroll, u);
    view(|v| {
        v.scroll = scroll;
        v.selected = selected;
    });
    draw_line(list.right() + 6.0 * u, body.y + 4.0 * u, list.right() + 6.0 * u, body.bottom() - 4.0 * u, 1.0, kit::alpha(GOLD, 0.35));

    // ---- the bench: the chosen recipe
    let d = Rect::new(list.right() + 24.0 * u, body.y, body.right() - list.right() - 24.0 * u, body.h);
    if let Some(rc) = selected.and_then(|s| recipes.iter().find(|r| r.obj == s)).cloned() {
        detail(ui, game, assets, &open.trade, open.per_step, &rc, d, &mut tip);
    }
    tooltip(&tip, u);
}

#[allow(clippy::too_many_arguments)]
fn detail(ui: &mut Ui, game: &mut Game, assets: &mut Assets, trade: &Trade, per_step: u32, rc: &Recipe, d: Rect, tip: &mut Vec<String>) {
    let u = ui.u;
    // The plinth: the object big on a lit recess, then name and requirement.
    let plinth = Rect::new(d.x, d.y, 96.0 * u, 96.0 * u);
    kit::well(plinth, u);
    kit::glow(plinth.center().x, plinth.center().y, 46.0 * u, Color::new(1.0, 0.75, 0.35, 1.0));
    draw_rectangle_lines(plinth.x, plinth.y, plinth.w, plinth.h, 1.0, kit::alpha(GOLD, 0.6));
    assets.draw_obj(rc.obj, Rect::new(plinth.x + 8.0 * u, plinth.y + 8.0 * u, plinth.w - 16.0 * u, plinth.h - 16.0 * u));
    if kit::hover(plinth) {
        *tip = obj_tip(game, assets, rc.obj, 1, &[]);
    }
    let name = obj_name(assets, rc.obj);
    let tx = plinth.right() + 16.0 * u;
    kit::fit(FontKind::Title, &name, tx, d.y + 34.0 * u, 24.0 * u, d.right() - tx, GOLD_BRIGHT);
    let mine = game.local.skills.get(trade.skill()).copied().unwrap_or(0);
    let ok_skill = mine >= rc.skill;
    kit::icon(if ok_skill { Icon::Check } else { Icon::Cross }, tx + 8.0 * u, d.y + 56.0 * u, 13.0 * u, if ok_skill { GOOD } else { BAD });
    text_with(FontKind::Body, &format!("{} {} · tú tienes {mine}", trade.name(), rc.skill), tx + 22.0 * u, d.y + 61.0 * u, 15.0 * u, if ok_skill { TEXT_DIM } else { BAD });
    let skill_bar = Rect::new(tx, d.y + 72.0 * u, d.right() - tx, 5.0 * u);
    kit::thin_bar(skill_bar, mine as f32, ao_rules::progression::MAX_SKILL as f32, if ok_skill { GOLD } else { BAD });
    // Mark where the recipe asks.
    let mx = skill_bar.x + skill_bar.w * (rc.skill as f32 / ao_rules::progression::MAX_SKILL as f32).clamp(0.0, 1.0);
    draw_line(mx, skill_bar.y - 3.0 * u, mx, skill_bar.bottom() + 3.0 * u, 2.0, GOLD_BRIGHT);

    // Materials (per the amount asked).
    let qty: u32 = view(|v| v.qty.parse().unwrap_or(0));
    let n = qty.max(1);
    let mut y = kit::section("Materiales", d.x, d.y + 108.0 * u, d.w, u);
    if rc.materials.is_empty() {
        text_with(FontKind::NarrativeItalic, "No necesita materiales.", d.x + 6.0 * u, y + 18.0 * u, 16.0 * u, TEXT_DIM);
        y += 28.0 * u;
    }
    for (obj, per) in &rc.materials {
        let need = per.saturating_mul(n);
        let have = game.local.count_of(*obj);
        let mr = Rect::new(d.x, y, d.w, 36.0 * u);
        let (hov, _) = kit::row(mr, false, u);
        let ic = Rect::new(mr.x + 4.0 * u, mr.y + 3.0 * u, 30.0 * u, 30.0 * u);
        kit::well(ic, u);
        assets.draw_obj(*obj, Rect::new(ic.x + 2.0 * u, ic.y + 2.0 * u, ic.w - 4.0 * u, ic.h - 4.0 * u));
        let enough = have >= need;
        let c = if enough { GOOD } else { BAD };
        let count = format!("{have} / {need}");
        let cw = measure_with(FontKind::Title, &count, 16.0 * u).width;
        kit::text_right(FontKind::Title, &count, mr.right() - 8.0 * u, mr.y + 20.0 * u, 16.0 * u, c);
        kit::fit(FontKind::Body, &obj_name(assets, *obj), ic.right() + 10.0 * u, mr.y + 17.0 * u, 16.0 * u, mr.w - ic.w - cw - 40.0 * u, TEXT);
        let bar = Rect::new(ic.right() + 10.0 * u, mr.y + 24.0 * u, mr.w - ic.w - cw - 34.0 * u, 4.0 * u);
        kit::thin_bar(bar, have as f32, need.max(1) as f32, c);
        if hov {
            *tip = obj_tip(game, assets, *obj, have, &[format!("Por unidad: {per}"), if enough { format!("Tienes {have}: te sobra.") } else { format!("!Tienes {have}, necesitas {need}.") }]);
        }
        y += 38.0 * u;
    }

    // Amount: stepper and "max".
    let can = affordable(&game.local, rc);
    let max = can.min(ao_rules::craft::MAX_ORDER);
    let ay = (y + 14.0 * u).min(d.bottom() - 150.0 * u);
    text_with(FontKind::Title, "CANTIDAD", d.x, ay + 18.0 * u, 14.0 * u, GOLD_TEXT);
    let minus = Rect::new(d.x + 96.0 * u, ay, 30.0 * u, 28.0 * u);
    let field = Rect::new(minus.right() + 4.0 * u, ay, 66.0 * u, 28.0 * u);
    let plus = Rect::new(field.right() + 4.0 * u, ay, 30.0 * u, 28.0 * u);
    let maxb = Rect::new(plus.right() + 8.0 * u, ay, 60.0 * u, 28.0 * u);
    if button_kind(minus, "-", Kind::Secondary, qty > 1, u) && qty > 1 {
        view(|v| v.qty = qty.saturating_sub(1).max(1).to_string());
    }
    if button_kind(plus, "+", Kind::Secondary, qty < ao_rules::craft::MAX_ORDER, u) && qty < ao_rules::craft::MAX_ORDER {
        view(|v| v.qty = (qty + 1).min(ao_rules::craft::MAX_ORDER).to_string());
    }
    if button_kind(maxb, "Máx", Kind::Ghost, max > 0, u) && max > 0 {
        view(|v| v.qty = max.to_string());
    }
    let mut s = view(|v| v.qty.clone());
    let enter = text_field(ui, "craft_qty", &mut s, field, 5, false, |c| c.is_ascii_digit());
    view(|v| v.qty = s);
    let per = if per_step > 1 { format!("Fabricas hasta {per_step} por golpe.") } else { "Fabricas de uno en uno.".to_string() };

    // Order: progress or build.
    let by = ay + 44.0 * u;
    let order = game.local.p4.craft.order;
    if let Some(o) = order {
        let stop = Rect::new(d.right() - 104.0 * u, by, 104.0 * u, 34.0 * u);
        let pr = Rect::new(d.x, by, d.w - 114.0 * u, 34.0 * u);
        kit::well(pr, u);
        let f = if o.total > 0 { o.done as f32 / o.total as f32 } else { 0.0 };
        draw_rectangle(pr.x + 2.0, pr.y + 2.0, (pr.w - 4.0) * f, pr.h - 4.0, Color::new(0.62, 0.44, 0.12, 1.0));
        draw_rectangle(pr.x + 2.0, pr.y + 2.0, (pr.w - 4.0) * f, (pr.h - 4.0) * 0.4, Color::new(1.0, 1.0, 1.0, 0.16));
        kit::text_mid(FontKind::Title, &format!("{} {} / {}", obj_name(assets, o.obj), o.done, o.total), pr.center().x, pr.center().y + 6.0 * u, 16.0 * u, TEXT);
        if button_kind(stop, "Detener", Kind::Danger, true, u) {
            game.craft_stop();
        }
    } else {
        let ok = (1..=ao_rules::craft::MAX_ORDER).contains(&qty) && mine >= rc.skill;
        let label = format!("Construir {}", qty.max(1));
        let b = Rect::new(d.x, by, d.w, 40.0 * u);
        if button_kind(b, &label, Kind::Primary, ok, u) || (enter && ok) {
            let r = game.craft_build(rc.obj, qty);
            view(|v| v.error = r.err());
        }
        if kit::hover(b) && can < qty {
            *tip = vec!["Te faltan materiales".into(), format!("Alcanzan para {can}")];
        }
    }

    // Feedback: why the last order stopped, or what was built.
    let fy = by + 60.0 * u;
    let st = &game.local.p4.craft;
    let (msg, c) = if let Some(e) = view(|v| v.error.clone()) {
        (e, BAD)
    } else if let Some(s) = st.last_stop.clone() {
        (s, BAD)
    } else if let Some((obj, n)) = st.built.last() {
        (format!("Has construido {n} {}.", obj_name(assets, *obj)), GOOD)
    } else {
        (String::new(), TEXT)
    };
    text_with(FontKind::NarrativeItalic, &per, d.x, fy, 15.0 * u, TEXT_DIM);
    if !msg.is_empty() {
        kit::fit(FontKind::NarrativeItalic, &msg, d.x, fy + 22.0 * u, 16.0 * u, d.w, c);
    }
    let _ = GOLD_BRIGHT;
}

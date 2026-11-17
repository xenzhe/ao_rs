//! Window: crafting at a station. OWNER: Opus B (docs/P4_CONTRATO.md).
//!
//! One tab per trade (blacksmith, carpenter, alchemist, tailor; the open one is lit), the
//! list the server sent on the left (icon, name, skill, how many the inventory covers) and
//! the selected object on the right: materials needed against what you carry, amount and
//! the build button; while an order runs, its progress and a stop button.
//!
//! The server opens the trade (`CraftS2C::Open`) when a tool is used or an anvil clicked;
//! `CraftState::open_request` asks the UI to show this window (the integrator calls
//! [`auto_window`] from `ui::auto_windows`). Clicking a tab asks the server to switch.
use super::bank::item_name;
use super::inventory::{btn_w, ibtn, item_tip, lmb_pressed, mouse, tooltip_at};
use crate::assets::Assets;
use crate::game::Game;
use crate::ui::theme::{FontKind, BAD, GOLD, GOLD_TEXT, GOOD, MUTED, SLOT, TEXT, WOOD_LIGHT};
use crate::ui::widgets::{bar, measure, text, text_field, text_fit, text_with};
use crate::ui::{Ui, WindowId};
use ao_protocol::p4::craft::{Recipe, Trade};
use client_core::p4::craft::affordable;
use macroquad::prelude::*;
use std::cell::RefCell;

const W: WindowId = WindowId::Crafting;
const ROW: f32 = 34.0;

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

pub fn draw(ui: &mut Ui, game: &mut Game, assets: &mut Assets, r: Rect) {
    let u = ui.u;
    dev_open(game);
    let act = ui.topmost_at(mouse()) == Some(W) && ui.drag.is_none();
    let m = mouse();
    let open = game.local.p4.craft.open.clone();
    let mut tip: Option<Vec<String>> = None;

    // ---- tabs ----
    let tab_h = 30.0 * u;
    let tw = (r.w - 3.0 * 4.0 * u) / 4.0;
    for (i, t) in Trade::ALL.into_iter().enumerate() {
        let tr = Rect::new(r.x + i as f32 * (tw + 4.0 * u), r.y, tw, tab_h);
        let on = open.as_ref().is_some_and(|o| o.trade == t);
        let hover = act && tr.contains(m);
        let bg = if on { Color::new(0.36, 0.25, 0.10, 1.0) } else if hover { Color::new(0.22, 0.16, 0.09, 1.0) } else { WOOD_LIGHT };
        draw_rectangle(tr.x, tr.y, tr.w, tr.h, bg);
        draw_rectangle_lines(tr.x, tr.y, tr.w, tr.h, if on { 2.0 * u } else { 1.0 }, if on { GOLD_TEXT } else { GOLD });
        assets.draw_obj(tab_icon(t), Rect::new(tr.x + 4.0 * u, tr.y + 3.0 * u, 24.0 * u, 24.0 * u));
        text_fit(t.name(), tr.x + 32.0 * u, tr.y + 20.0 * u, 17.0 * u, tr.w - 36.0 * u, if on { GOLD_TEXT } else { TEXT });
        if hover {
            if lmb_pressed() && !on {
                let station = if t == Trade::Blacksmith { open.as_ref().and_then(|o| o.station) } else { None };
                game.craft_open(t, station);
            }
            let how = match t {
                Trade::Blacksmith => "Martillo de herrero equipado y clic en un yunque",
                Trade::Carpenter => "Serrucho equipado",
                Trade::Alchemy => "Olla de alquimia equipada",
                Trade::Tailor => "Costurero equipado",
            };
            tip = Some(vec![t.name().to_string(), how.to_string()]);
        }
    }

    let body = Rect::new(r.x, r.y + tab_h + 8.0 * u, r.w, r.h - tab_h - 8.0 * u);
    // `AO_CRAFT_BUILD=n` (screenshots): starts an order of the selected object once.
    if open.is_some() && !view(|v| std::mem::replace(&mut v.dev_built, true)) {
        if let (Some(n), Some(sel)) = (std::env::var("AO_CRAFT_BUILD").ok().and_then(|s| s.parse().ok()), view(|v| v.selected)) {
            view(|v| v.qty = format!("{n}"));
            let _ = game.craft_build(sel, n);
        }
    }
    let Some(open) = open else {
        text_with(FontKind::Title, "Ningún oficio abierto", body.x + 8.0 * u, body.y + 30.0 * u, 22.0 * u, GOLD_TEXT);
        let lines = [
            "Equipate la herramienta de tu oficio y usala desde el inventario:",
            "el serrucho, la olla de alquimia o el costurero.",
            "Para la herrería, con el martillo equipado hacé clic en un yunque.",
            "Solo los trabajadores pueden usar las herramientas.",
        ];
        for (i, l) in lines.iter().enumerate() {
            text(l, body.x + 8.0 * u, body.y + 62.0 * u + i as f32 * 22.0 * u, 17.0 * u, if i == 3 { MUTED } else { TEXT });
        }
        if let Some(t) = tip {
            if act {
                tooltip_at(&t, m, u);
            }
        }
        return;
    };

    // ---- list ----
    let list = Rect::new(body.x, body.y, (body.w * 0.47).floor(), body.h);
    draw_rectangle(list.x, list.y, list.w, list.h, Color::new(0.03, 0.025, 0.02, 0.6));
    let rows = ((list.h - 4.0 * u) / (ROW * u)).floor().max(1.0) as usize;
    let recipes = open.recipes.clone();
    let mut scroll = view(|v| v.scroll).min(recipes.len().saturating_sub(rows));
    if act && list.contains(m) {
        let wheel = mouse_wheel().1;
        if wheel > 0.0 {
            scroll = scroll.saturating_sub(1);
        } else if wheel < 0.0 && scroll + rows < recipes.len() {
            scroll += 1;
        }
    }
    let mut selected = view(|v| v.selected).filter(|s| recipes.iter().any(|r| r.obj == *s)).or_else(|| recipes.first().map(|r| r.obj));
    let row_w = list.w - if recipes.len() > rows { 10.0 * u } else { 2.0 * u };
    for (k, rc) in recipes.iter().skip(scroll).take(rows).enumerate() {
        let rr = Rect::new(list.x + 1.0, list.y + 2.0 * u + k as f32 * ROW * u, row_w, (ROW - 2.0) * u);
        let hover = act && rr.contains(m);
        if Some(rc.obj) == selected {
            draw_rectangle(rr.x, rr.y, rr.w, rr.h, Color::new(0.35, 0.25, 0.08, 0.8));
        } else if hover {
            draw_rectangle(rr.x, rr.y, rr.w, rr.h, Color::new(0.2, 0.15, 0.08, 0.8));
        }
        let icon = Rect::new(rr.x + 3.0 * u, rr.y + 1.0 * u, 30.0 * u, 30.0 * u);
        draw_rectangle(icon.x, icon.y, icon.w, icon.h, SLOT);
        draw_rectangle_lines(icon.x, icon.y, icon.w, icon.h, 1.0, Color::new(0.35, 0.26, 0.14, 1.0));
        assets.draw_obj(rc.obj, Rect::new(icon.x + 1.0, icon.y + 1.0, icon.w - 2.0, icon.h - 2.0));
        let can = affordable(&game.local, rc);
        let badge = if can == u32::MAX { "∞".to_string() } else { format!("x{}", can.min(9999)) };
        let bw = measure(&badge, 15.0 * u).width;
        text(&badge, rr.right() - bw - 6.0 * u, rr.y + 20.0 * u, 15.0 * u, if can > 0 { GOOD } else { BAD });
        text_fit(&item_name(assets, rc.obj), icon.right() + 6.0 * u, rr.y + 14.0 * u, 16.0 * u, rr.w - icon.w - bw - 22.0 * u, TEXT);
        text(&format!("{} {}", open.trade.name(), rc.skill), icon.right() + 6.0 * u, rr.y + 28.0 * u, 13.0 * u, MUTED);
        if hover {
            if lmb_pressed() {
                selected = Some(rc.obj);
            }
            tip = Some(item_tip(game, assets, rc.obj, 1, false, &[format!("Requiere {} {}", open.trade.name(), rc.skill)]));
        }
    }
    if recipes.is_empty() {
        text_fit("Todavía no sabés construir nada aquí.", list.x + 8.0 * u, list.y + 24.0 * u, 16.0 * u, list.w - 16.0 * u, MUTED);
    }
    if recipes.len() > rows {
        let sb = Rect::new(list.right() - 7.0 * u, list.y + 2.0 * u, 6.0 * u, list.h - 4.0 * u);
        draw_rectangle(sb.x, sb.y, sb.w, sb.h, Color::new(0.0, 0.0, 0.0, 0.5));
        let (th, ty) = (sb.h * rows as f32 / recipes.len() as f32, sb.h * scroll as f32 / recipes.len() as f32);
        draw_rectangle(sb.x, sb.y + ty, sb.w, th, GOLD);
    }
    draw_rectangle_lines(list.x, list.y, list.w, list.h, 1.0, Color::new(0.35, 0.26, 0.14, 1.0));
    view(|v| {
        v.scroll = scroll;
        v.selected = selected;
    });

    // ---- detail ----
    let d = Rect::new(list.right() + 10.0 * u, body.y, body.right() - list.right() - 10.0 * u, body.h);
    if let Some(rc) = selected.and_then(|s| recipes.iter().find(|r| r.obj == s)).cloned() {
        detail(ui, game, assets, &open.trade, open.per_step, &rc, d, act, &mut tip);
    }

    if let Some(t) = tip {
        if act {
            tooltip_at(&t, m, u);
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn detail(ui: &mut Ui, game: &mut Game, assets: &mut Assets, trade: &Trade, per_step: u32, rc: &Recipe, d: Rect, act: bool, tip: &mut Option<Vec<String>>) {
    let u = ui.u;
    let m = mouse();
    // Header: big icon, name, skill.
    let icon = Rect::new(d.x, d.y, 52.0 * u, 52.0 * u);
    draw_rectangle(icon.x, icon.y, icon.w, icon.h, SLOT);
    draw_rectangle_lines(icon.x, icon.y, icon.w, icon.h, 1.0, GOLD);
    assets.draw_obj(rc.obj, Rect::new(icon.x + 4.0 * u, icon.y + 4.0 * u, icon.w - 8.0 * u, icon.h - 8.0 * u));
    let name = item_name(assets, rc.obj);
    text_with(FontKind::Title, &name, icon.right() + 10.0 * u, d.y + 20.0 * u, 19.0 * u, GOLD_TEXT);
    let mine = game.local.skills.get(trade_skill(*trade)).copied().unwrap_or(0);
    text(&format!("Requiere {} {} (tenés {mine})", trade.name(), rc.skill), icon.right() + 10.0 * u, d.y + 42.0 * u, 15.0 * u, MUTED);
    if act && icon.contains(m) {
        *tip = Some(item_tip(game, assets, rc.obj, 1, false, &[]));
    }

    // Materials (per the amount asked).
    let qty: u32 = view(|v| v.qty.parse().unwrap_or(0));
    let n = qty.max(1);
    let my = d.y + 64.0 * u;
    text("Materiales", d.x, my + 14.0 * u, 17.0 * u, GOLD_TEXT);
    let mut y = my + 22.0 * u;
    if rc.materials.is_empty() {
        text("No necesita materiales.", d.x + 4.0 * u, y + 18.0 * u, 15.0 * u, MUTED);
        y += 26.0 * u;
    }
    for (obj, per) in &rc.materials {
        let need = per.saturating_mul(n);
        let have = game.local.count_of(*obj);
        let mr = Rect::new(d.x, y, d.w, 28.0 * u);
        draw_rectangle(mr.x, mr.y, mr.w, mr.h, Color::new(0.03, 0.025, 0.02, 0.55));
        let ic = Rect::new(mr.x + 2.0 * u, mr.y + 2.0 * u, 24.0 * u, 24.0 * u);
        draw_rectangle(ic.x, ic.y, ic.w, ic.h, SLOT);
        assets.draw_obj(*obj, Rect::new(ic.x + 1.0, ic.y + 1.0, ic.w - 2.0, ic.h - 2.0));
        let count = format!("{have} / {need}");
        let cw = measure(&count, 16.0 * u).width;
        text(&count, mr.right() - cw - 6.0 * u, mr.y + 19.0 * u, 16.0 * u, if have >= need { GOOD } else { BAD });
        text_fit(&item_name(assets, *obj), ic.right() + 6.0 * u, mr.y + 19.0 * u, 15.0 * u, mr.w - ic.w - cw - 20.0 * u, TEXT);
        if act && mr.contains(m) {
            *tip = Some(item_tip(game, assets, *obj, have, false, &[format!("Por unidad: {per}"), format!("Tenés {have}, necesitás {need}")]));
        }
        y += 30.0 * u;
    }

    // Amount.
    let can = affordable(&game.local, rc);
    let max = can.min(ao_rules::craft::MAX_ORDER);
    let ay = y + 14.0 * u;
    text("Cantidad", d.x, ay + 18.0 * u, 17.0 * u, GOLD_TEXT);
    let minus = Rect::new(d.x + 76.0 * u, ay, 26.0 * u, 26.0 * u);
    let field = Rect::new(minus.right() + 4.0 * u, ay, 70.0 * u, 26.0 * u);
    let plus = Rect::new(field.right() + 4.0 * u, ay, 26.0 * u, 26.0 * u);
    let mw = btn_w("Máx", u);
    let maxb = Rect::new(plus.right() + 4.0 * u, ay, mw, 26.0 * u);
    if ibtn(minus, "-", qty > 1, act, u) {
        view(|v| v.qty = qty.saturating_sub(1).max(1).to_string());
    }
    if ibtn(plus, "+", qty < ao_rules::craft::MAX_ORDER, act, u) {
        view(|v| v.qty = (qty + 1).min(ao_rules::craft::MAX_ORDER).to_string());
    }
    if ibtn(maxb, "Máx", max > 0, act, u) {
        view(|v| v.qty = max.to_string());
    }
    let mut s = view(|v| v.qty.clone());
    let enter = text_field(ui, "craft_qty", &mut s, field, 5, false, |c| c.is_ascii_digit());
    view(|v| v.qty = s);
    let per = if per_step > 1 { format!("Fabricás hasta {per_step} por golpe.") } else { "Fabricás de a uno.".to_string() };
    text(&per, d.x, ay + 46.0 * u, 14.0 * u, MUTED);

    // Order: progress or build.
    let by = ay + 58.0 * u;
    let order = game.local.p4.craft.order;
    let bw = d.w;
    if let Some(o) = order {
        let pr = Rect::new(d.x, by, bw - btn_w("Detener", u) - 6.0 * u, 26.0 * u);
        bar(pr, o.done as f32, o.total as f32, Color::new(0.55, 0.40, 0.12, 1.0), &format!("{} {} / {}", item_name(assets, o.obj), o.done, o.total), u);
        if ibtn(Rect::new(pr.right() + 6.0 * u, by, btn_w("Detener", u), 26.0 * u), "Detener", true, act, u) {
            game.craft_stop();
        }
    } else {
        let ok = (1..=ao_rules::craft::MAX_ORDER).contains(&qty) && mine >= rc.skill;
        let label = format!("Construir {}", qty.max(1));
        let b = Rect::new(d.x, by, bw, 30.0 * u);
        if ibtn(b, &label, ok, act, u) || (enter && ok) {
            let r = game.craft_build(rc.obj, qty);
            view(|v| v.error = r.err());
        }
        if act && b.contains(m) && can < qty {
            *tip = Some(vec!["Te faltan materiales".into(), format!("Alcanzan para {can}")]);
        }
    }

    // Feedback: why the last order stopped, or what was built.
    let fy = by + 48.0 * u;
    let st = &game.local.p4.craft;
    let (msg, c) = if let Some(e) = view(|v| v.error.clone()) {
        (e, BAD)
    } else if let Some(s) = st.last_stop.clone() {
        (s, BAD)
    } else if let Some((obj, n)) = st.built.last() {
        (format!("Construiste {n} {}.", item_name(assets, *obj)), GOOD)
    } else {
        (String::new(), TEXT)
    };
    if !msg.is_empty() {
        text_fit(&msg, d.x, fy, 15.0 * u, d.w, c);
    }
    let _ = d.bottom();
}

fn trade_skill(t: Trade) -> usize {
    t.skill()
}

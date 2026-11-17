//! Window: trade between players (offers, double confirmation). OWNER: Sonnet 2 (docs/P3_CONTRATO.md).
//!
//! Two tables face each other: yours on the left (what you put, and your bag to take it
//! from) and theirs on the right (what they put, and the log of every change they make). In
//! the middle the two steps of the double confirmation, each with a lock (step 1) or a seal
//! (step 2) per side, so who has done what is seen at a glance. Any change of either offer
//! wipes the confirmations: the window says so and lights the table that changed.
//!
//! Hooks for the world: [`draw_overlay`] (the invitation banner, needs one call per frame
//! from `ui::frame`, and the watcher that turns the end of a trade into a toast) and
//! [`Game::trade_with_tile`] (a click on a player, see `input.rs`).
use super::inventory::{self as inv, active, aux, btn_w, chip, draw_cell, draw_grid, fmt_gold, glyph, handle, ibtn, ibtn_kind, item_name, item_tip, mouse, open_prompt, selected_inv, tray, track, CellOpts, Glyph, Grid, GridView, Hit, Pending, TradeChange, TradeSeen, Zone};
use crate::assets::Assets;
use crate::game::Game;
use crate::render::layout::Layout;
use crate::ui::fx_ui::{ease, key_of, toast, ToastKind};
use crate::ui::theme::{self, FontKind, BAD, GOLD, GOLD_BRIGHT, GOLD_DIM, GOLD_TEXT, GOOD, TEXT, TEXT_DIM, TEXT_FAINT};
use crate::ui::widgets::{self, measure, panel, text, text_fit, text_with, wrap_lines, Kind};
use crate::ui::{Ui, WindowId};
use client_core::p3::items::{trade_phase, usable_slots, TradePhase, MAX_OFFER_ITEMS};
use client_core::social::trade::{Offer, PlayerTrade};
use client_core::ItemSlot;
use macroquad::prelude::*;
use std::cell::Cell;

thread_local! {
    static OVERLAY_AT: Cell<f64> = const { Cell::new(-10.0) };
}

/// Whether `draw_overlay` is being called every frame (the integrator wired it).
fn overlay_wired() -> bool {
    get_time() - OVERLAY_AT.with(|t| t.get()) < 0.5
}

/// What changed between two offers of the same player.
fn diff_offers(old: &Offer, new: &Offer) -> Vec<TradeChange> {
    let mut v = Vec::new();
    for (obj, n) in &new.items {
        match old.items.iter().find(|(o, _)| o == obj) {
            None => v.push(TradeChange::Added(*obj, *n)),
            Some((_, was)) if was != n => v.push(TradeChange::Changed(*obj, *was, *n)),
            _ => {}
        }
    }
    for (obj, _) in &old.items {
        if !new.items.iter().any(|(o, _)| o == obj) {
            v.push(TradeChange::Removed(*obj));
        }
    }
    if old.gold != new.gold {
        v.push(TradeChange::Gold(old.gold, new.gold));
    }
    v
}

/// Watches the trade every frame (even with the window closed): records what changed in
/// each offer, when the confirmations were wiped, and says in a toast how it ended.
fn watch(game: &Game) {
    let now = get_time();
    let cur = game.local.social.trade.player.as_ref();
    let seen = aux(|a| a.trade_seen.clone());
    match (cur, seen) {
        (Some(t), Some(s)) if s.name == t.name => {
            let (dm, dt) = (diff_offers(&s.mine, &t.mine), diff_offers(&s.theirs, &t.theirs));
            let had_seal = s.i_confirmed || s.they_confirmed || s.i_accepted || s.they_accepted;
            let wiped = had_seal && !(t.i_confirmed || t.they_confirmed) && (!dm.is_empty() || !dt.is_empty());
            aux(|a| {
                if !dm.is_empty() {
                    a.trade_flash[0] = now;
                }
                if !dt.is_empty() {
                    a.trade_flash[1] = now;
                }
                for c in dm {
                    a.trade_log.push((now, c, false));
                }
                for c in dt {
                    a.trade_log.push((now, c, true));
                }
                if wiped {
                    a.trade_banner = now;
                    a.trade_log.push((now, TradeChange::Wiped, true));
                }
                if a.trade_log.len() > 30 {
                    a.trade_log.drain(..a.trade_log.len() - 30);
                }
            });
        }
        (Some(_), _) => {
            // A new trade: a clean slate.
            aux(|a| {
                a.trade_log.clear();
                a.trade_flash = [-10.0; 2];
                a.trade_banner = -10.0;
            });
        }
        (None, Some(s)) => {
            // It ended: closed with a deal (both accepted) or cancelled.
            if s.i_accepted && s.they_accepted {
                toast(format!("Trato cerrado con {}", s.name), ToastKind::Good);
            } else {
                toast(format!("Comercio con {} cancelado", s.name), ToastKind::Warn);
            }
        }
        (None, None) => {}
    }
    let next = cur.map(|t| TradeSeen { rev: t.rev, mine: t.mine.clone(), theirs: t.theirs.clone(), i_confirmed: t.i_confirmed, they_confirmed: t.they_confirmed, i_accepted: t.i_accepted, they_accepted: t.they_accepted, name: t.name.clone() });
    aux(|a| a.trade_seen = next);
}

/// Invitation banner for the whole screen: "X wants to trade [Accept] [Decline]". Call it
/// every frame from `ui::frame`, after the windows (it is the only way to answer an
/// invitation while no item window is open).
#[allow(dead_code)] // until the integrator wires it into ui::frame (see the P3 report)
pub fn draw_overlay(ui: &mut Ui, game: &mut Game) {
    OVERLAY_AT.with(|t| t.set(get_time()));
    inv::purge(ui);
    watch(game);
    if game.local.social.trade.invited_by.is_none() || game.local.social.trade.player.is_some() {
        return;
    }
    let u = ui.u;
    let (w, h) = (440.0 * u, 52.0 * u);
    let r = Rect::new((screen_width() / 2.0 - w / 2.0 - 116.0 * u).max(4.0), Layout::compute(false).console.bottom() + 12.0 * u, w, h);
    panel(r, u);
    ui.capture_mouse_if_over(r);
    invite_banner(game, Rect::new(r.x + 14.0 * u, r.y + 10.0 * u, r.w - 28.0 * u, r.h - 20.0 * u), true, u);
}

/// "X te invita a comerciar" with Accept / Decline inside `r`.
fn invite_banner(game: &mut Game, r: Rect, act: bool, u: f32) {
    let Some((_, name)) = game.local.social.trade.invited_by.clone() else { return };
    let (a, d) = (btn_w("Aceptar", u), btn_w("Rechazar", u));
    glyph(Glyph::Coin, vec2(r.x + 9.0 * u, r.y + r.h / 2.0), 9.0 * u, GOLD);
    widgets::text_fit(&format!("{name} te invita a comerciar"), r.x + 26.0 * u, r.y + r.h / 2.0 + 6.0 * u, 18.0 * u, r.w - a - d - 50.0 * u, GOLD_TEXT);
    let dr = Rect::new(r.right() - d, r.y, d, r.h);
    if ibtn_kind(dr, "Rechazar", Kind::Ghost, true, act, u) {
        game.trade_decline();
    }
    if ibtn_kind(Rect::new(dr.x - 6.0 * u - a, r.y, a, r.h), "Aceptar", Kind::Primary, true, act, u) {
        game.trade_accept_invite();
    }
}

/// The trade line at the bottom of the inventory window: the pending invitation, or the
/// players close enough to ask, one button each.
pub fn inventory_row(ui: &mut Ui, game: &mut Game, row: Rect, act: bool) {
    let u = ui.u;
    if game.local.social.trade.invited_by.is_some() && !overlay_wired() {
        return invite_banner(game, row, act, u);
    }
    if let Some(t) = &game.local.social.trade.player {
        let s = format!("Comerciando con {}", t.name);
        return text_fit(&s, row.x + 4.0 * u, row.y + 18.0 * u, 17.0 * u, row.w, GOLD_TEXT);
    }
    let near = game.nearby_players();
    if near.is_empty() {
        return text_with(FontKind::NarrativeItalic, "Nadie cerca con quien comerciar", row.x + 4.0 * u, row.y + 18.0 * u, 16.0 * u, TEXT_FAINT);
    }
    text("Comerciar con", row.x + 4.0 * u, row.y + 18.0 * u, 15.0 * u, TEXT_DIM);
    let mut x = row.x + 4.0 * u + measure("Comerciar con", 15.0 * u).width + 10.0 * u;
    for (id, name) in near.into_iter().take(3) {
        let label: String = name.chars().take(12).collect();
        let bw = btn_w(&label, u);
        if x + bw > row.right() {
            break;
        }
        if ibtn(Rect::new(x, row.y, bw, row.h), &label, true, act, u) {
            game.trade_propose(id);
        }
        x += bw + 4.0 * u;
    }
}

/// How long ago a flash started, as 1 (just now) .. 0 (gone).
fn flash(since: f64) -> f32 {
    (1.0 - (get_time() - since) as f32 / 2.0).clamp(0.0, 1.0)
}

/// A wax seal: round, gold, the check cut in it (`done`), or an empty ring (waiting).
fn seal(c: Vec2, rad: f32, done: bool, ready: bool, u: f32) {
    if done {
        draw_circle(c.x + 1.5 * u, c.y + 2.0 * u, rad, Color::new(0.0, 0.0, 0.0, 0.5));
        for i in 0..8 {
            let a = i as f32 * std::f32::consts::TAU / 8.0;
            draw_circle(c.x + a.cos() * rad * 0.92, c.y + a.sin() * rad * 0.92, rad * 0.22, Color::new(0.55, 0.38, 0.12, 1.0));
        }
        draw_circle(c.x, c.y, rad, Color::new(0.60, 0.42, 0.14, 1.0));
        draw_circle(c.x, c.y, rad * 0.78, Color::new(0.90, 0.70, 0.30, 1.0));
        draw_circle_lines(c.x, c.y, rad * 0.62, 1.0, Color::new(0.55, 0.38, 0.12, 1.0));
        glyph(Glyph::Check, c, rad * 0.5, Color::new(0.24, 0.14, 0.04, 1.0));
    } else {
        let a = if ready { 0.55 + 0.25 * (get_time() as f32 * 3.0).sin() } else { 0.25 };
        draw_circle(c.x, c.y, rad, Color::new(0.0, 0.0, 0.0, 0.35));
        draw_circle_lines(c.x, c.y, rad, 1.5 * u, Color::new(GOLD.r, GOLD.g, GOLD.b, a));
        draw_circle_lines(c.x, c.y, rad * 0.7, 1.0, Color::new(GOLD.r, GOLD.g, GOLD.b, a * 0.6));
    }
}

/// A padlock: closed and lit when `done`, open and faint when waiting.
fn padlock(c: Vec2, rad: f32, done: bool, u: f32) {
    if done {
        draw_circle(c.x, c.y, rad * 1.25, Color::new(GOOD.r, GOOD.g, GOOD.b, 0.13));
        draw_circle_lines(c.x, c.y, rad * 1.25, 1.5 * u, Color::new(GOOD.r, GOOD.g, GOOD.b, 0.7));
        glyph(Glyph::Lock, c, rad * 0.85, GOOD);
    } else {
        draw_circle_lines(c.x, c.y, rad * 1.25, 1.0, Color::new(GOLD_DIM.r, GOLD_DIM.g, GOLD_DIM.b, 0.7));
        glyph(Glyph::Unlock, c, rad * 0.85, Color::new(0.62, 0.55, 0.44, 0.8));
    }
}

/// One table: the six places of an offer. Returns (retract index, hovered tooltip, rect of the
/// whole offer for dropping).
#[allow(clippy::too_many_arguments)]
fn offer_table(ui: &Ui, game: &Game, assets: &mut Assets, area: Rect, offer: &Offer, mine: bool, act: bool, lit: f32) -> (Option<u8>, Option<Vec<String>>) {
    let u = ui.u;
    let m = mouse();
    let (mut retract, mut tip) = (None, None);
    let hot = mine && ui.drag.as_ref().is_some_and(|d| d.from == WindowId::Inventory) && act && area.contains(m);
    tray(area, ease(key_of(area), hot, 0.1), u);
    if lit > 0.0 {
        draw_rectangle(area.x, area.y, area.w, area.h, Color::new(GOLD_BRIGHT.r, GOLD_BRIGHT.g, GOLD_BRIGHT.b, 0.10 * lit));
        draw_rectangle_lines(area.x - 2.0 * u, area.y - 2.0 * u, area.w + 4.0 * u, area.h + 4.0 * u, 2.0 * u, Color::new(GOLD_BRIGHT.r, GOLD_BRIGHT.g, GOLD_BRIGHT.b, 0.9 * lit));
    }
    let (cols, gap) = (3usize, 8.0 * u);
    let cell = ((area.w - 16.0 * u - 2.0 * gap) / cols as f32).floor();
    let label_h = 17.0 * u;
    let gx = area.x + (area.w - (cols as f32 * cell + (cols as f32 - 1.0) * gap)) / 2.0;
    for i in 0..MAX_OFFER_ITEMS {
        let (c, row) = (i % cols, i / cols);
        let cr = Rect::new(gx + c as f32 * (cell + gap), area.y + 10.0 * u + row as f32 * (cell + label_h + 6.0 * u), cell, cell);
        let item = offer.items.get(i).copied();
        let hover = act && cr.contains(m);
        let opts = CellOpts { hover, ..Default::default() };
        draw_cell(assets, cr, item.map(|(o, n)| ItemSlot { obj: o, amount: n, equipped: false }), &opts, u);
        match item {
            Some((obj, n)) => {
                text_fit(&item_name(assets, obj), cr.x, cr.bottom() + 14.0 * u, 13.0 * u, cr.w, TEXT_DIM);
                if hover && ui.drag.is_none() {
                    let mut extra = Vec::new();
                    if mine {
                        extra.push("Clic: quitarlo de la oferta".to_string());
                    }
                    tip = Some(item_tip(game, assets, obj, n, false, &extra));
                    if mine && inv::lmb_pressed() {
                        retract = Some(i as u8);
                    }
                }
            }
            None => {
                let c = vec2(cr.x + cr.w / 2.0, cr.y + cr.h / 2.0);
                if mine {
                    draw_line(c.x - 7.0 * u, c.y, c.x + 7.0 * u, c.y, 1.5 * u, Color::new(GOLD.r, GOLD.g, GOLD.b, 0.28));
                    draw_line(c.x, c.y - 7.0 * u, c.x, c.y + 7.0 * u, 1.5 * u, Color::new(GOLD.r, GOLD.g, GOLD.b, 0.28));
                } else {
                    draw_circle(c.x, c.y, 2.2 * u, Color::new(GOLD.r, GOLD.g, GOLD.b, 0.25));
                }
            }
        }
    }
    (retract, tip)
}

/// A line of the log, in words.
fn change_text(assets: &Assets, who: Option<&str>, c: &TradeChange) -> String {
    let n = |o: &u16| item_name(assets, *o);
    let g = |v: &u32| fmt_gold(*v as u64);
    match (who, c) {
        (_, TradeChange::Wiped) => "Las confirmaciones se anularon".to_string(),
        (None, TradeChange::Added(o, k)) => format!("Pusiste {}× {}", g(k), n(o)),
        (None, TradeChange::Removed(o)) => format!("Quitaste {}", n(o)),
        (None, TradeChange::Changed(o, from, to)) => format!("Cambiaste {} de {} a {}", n(o), g(from), g(to)),
        (None, TradeChange::Gold(from, 0)) => format!("Retiraste tus {} oro", g(from)),
        (None, TradeChange::Gold(_, to)) => format!("Ofreces {} oro", g(to)),
        (Some(w), TradeChange::Added(o, k)) => format!("{w} puso {}× {}", g(k), n(o)),
        (Some(w), TradeChange::Removed(o)) => format!("{w} quitó {}", n(o)),
        (Some(w), TradeChange::Changed(o, from, to)) => format!("{w} cambió {} de {} a {}", n(o), g(from), g(to)),
        (Some(w), TradeChange::Gold(from, 0)) => format!("{w} retiró sus {} oro", g(from)),
        (Some(w), TradeChange::Gold(_, to)) => format!("{w} ofrece {} oro", g(to)),
    }
}

/// Draws the content of the window inside `r` (the frame is already drawn).
pub fn draw(ui: &mut Ui, game: &mut Game, assets: &mut Assets, r: Rect) {
    let u = ui.u;
    let w = WindowId::Trade;
    inv::begin(ui, game, assets);
    let Some(t) = game.local.social.trade.player.clone() else { return };
    let inventory = game.local.inventory.clone();
    let act = active(ui, w);
    let m = mouse();
    let mut zones = Vec::new();
    let mut tip: Option<Vec<String>> = None;
    let bp = inventory.iter().flatten().any(|i| i.equipped && assets.objs.get(i.obj as u32).is_some_and(|o| o.obj_type == ao_formats::dat::obj::ObjType::Backpack));
    let usable = usable_slots(bp);
    let phase = trade_phase(&t);
    let tw = 292.0 * u;
    let (x0, x1) = (r.x, r.right() - tw);
    let who: String = t.name.clone();
    let [fl_me, fl_they] = aux(|a| a.trade_flash);
    let top = theme::section("Tu mesa", x0, r.y - 4.0 * u, tw, u);
    theme::section(&format!("Mesa de {who}"), x1, r.y - 4.0 * u, tw, u);
    // Who confirmed, next to each table's heading.
    for (x, ok) in [(x0, t.i_confirmed), (x1, t.they_confirmed)] {
        let label = if t.i_accepted && x == x0 || t.they_accepted && x == x1 { "Aceptó" } else if ok { "Confirmó" } else { "Armando" };
        let col = if label == "Armando" { TEXT_FAINT } else { GOOD };
        let d = measure(label, 15.0 * u);
        text(label, x + tw - d.width, r.y + 14.0 * u, 15.0 * u, col);
        if label != "Armando" {
            glyph(Glyph::Lock, vec2(x + tw - d.width - 10.0 * u, r.y + 8.0 * u), 6.0 * u, GOOD);
        }
    }

    // ---- the two offers ----
    let (th, gold_h) = (168.0 * u, 40.0 * u);
    let mine_r = Rect::new(x0 + 4.0 * u, top + 6.0 * u, tw - 8.0 * u, th);
    let theirs_r = Rect::new(x1 + 4.0 * u, top + 6.0 * u, tw - 8.0 * u, th);
    let (retract, tp) = offer_table(ui, game, assets, mine_r, &t.mine, true, act, flash(fl_me));
    tip = tip.or(tp);
    zones.push((Zone::OfferMine, mine_r));
    if let Some(i) = retract {
        game.trade_retract(i);
    }
    let (_, tp) = offer_table(ui, game, assets, theirs_r, &t.theirs, false, act, flash(fl_they));
    tip = tip.or(tp);
    // Gold on each table.
    let gy = mine_r.bottom() + 12.0 * u;
    for (x, gold, mine) in [(x0, t.mine.gold, true), (x1, t.theirs.gold, false)] {
        glyph(Glyph::Coin, vec2(x + 14.0 * u, gy + 16.0 * u), 12.0 * u, if gold > 0 { GOLD_BRIGHT } else { Color::new(0.5, 0.42, 0.3, 0.7) });
        text_with(FontKind::Title, &fmt_gold(gold as u64), x + 36.0 * u, gy + 25.0 * u, 24.0 * u, if gold > 0 { GOLD_BRIGHT } else { TEXT_FAINT });
        if mine {
            let (g1, g2) = (btn_w("Oro...", u), btn_w("Quitar", u));
            let gb = Rect::new(x + tw - g1, gy + 2.0 * u, g1, 28.0 * u);
            if ibtn(gb, "Oro...", game.local.gold > 0, act, u) {
                open_prompt(w, Pending::OfferGold, "Oro que ofreces", game.local.gold, t.mine.gold.clamp(1, game.local.gold.max(1)), None);
            }
            if gold > 0 && ibtn_kind(Rect::new(gb.x - 4.0 * u - g2, gy + 2.0 * u, g2, 28.0 * u), "Quitar", Kind::Ghost, true, act, u) {
                game.trade_offer_gold(0);
            }
        }
    }

    // ---- under my table: my bag; under theirs: what changed ----
    let by = gy + gold_h + 6.0 * u;
    text("Tu bolsa: arrastra a la mesa o doble clic", x0 + 2.0 * u, by + 10.0 * u, 14.0 * u, TEXT_FAINT);
    let grid = Grid::fit(7, 6, tw - 12.0 * u, r.bottom() - (by + 22.0 * u) - 6.0 * u, 3.0 * u, 40.0 * u);
    let io = vec2(x0 + 6.0 * u + (tw - 12.0 * u - grid.size().x) / 2.0, by + 20.0 * u);
    let sel_i = selected_inv();
    let (hit, z2) = draw_grid(ui, assets, &grid, io, &GridView::plain(&inventory, WindowId::Inventory, sel_i, usable), act);
    zones.extend(z2);
    if let (Some(h), None) = (hit, &ui.drag) {
        let eq = game.local.slot(h.slot).is_some_and(|i| i.equipped);
        tip = Some(item_tip(game, assets, h.obj, h.amount, eq, &["Doble clic: ofrecer".to_string()]));
    }
    if let Some(ev) = track(ui, w, act, hit.map(|h: Hit| h)) {
        handle(game, assets, w, ev);
    }
    log_panel(assets, Rect::new(x1 + 4.0 * u, by + 20.0 * u, tw - 8.0 * u, r.bottom() - (by + 20.0 * u) - 6.0 * u), &who, u);
    text("Cambios en la mesa", x1 + 2.0 * u, by + 10.0 * u, 14.0 * u, TEXT_FAINT);

    // ---- the middle: the double confirmation ----
    let bx = x0 + tw + 14.0 * u;
    let bw = x1 - 14.0 * u - bx;
    steps(game, &t, phase, assets, Rect::new(bx, top + 2.0 * u, bw, r.bottom() - top - 2.0 * u), act, u);

    inv::register(w, r, u, zones);
    inv::menu_ui(ui, game, assets, w);
    inv::prompt_ui(ui, game, assets, w, r);
    if let Some(tp) = tip {
        if ui.drag.is_none() && act {
            inv::tooltip_at(&tp, m, u);
        }
    }
    let _ = (GOLD_DIM, TEXT);
}

/// The log of changes, newest first, fading with age.
fn log_panel(assets: &Assets, area: Rect, who: &str, u: f32) {
    tray(area, 0.0, u);
    let log = aux(|a| a.trade_log.clone());
    if log.is_empty() {
        text_with(FontKind::NarrativeItalic, "Aquí verás cada cambio de la mesa.", area.x + 12.0 * u, area.y + 26.0 * u, 15.0 * u, TEXT_FAINT);
        return;
    }
    let now = get_time();
    let rows = ((area.h - 12.0 * u) / (24.0 * u)).floor() as usize;
    for (i, (at, c, theirs)) in log.iter().rev().take(rows).enumerate() {
        let y = area.y + 8.0 * u + i as f32 * 24.0 * u;
        let fresh = ((1.0 - (now - at) as f32 / 8.0).clamp(0.0, 1.0)) * 0.5;
        let (who_s, col) = match (theirs, c) {
            (_, TradeChange::Wiped) => (None, BAD),
            (true, _) => (Some(who), GOLD_TEXT),
            (false, _) => (None, TEXT_DIM),
        };
        if fresh > 0.0 {
            draw_rectangle(area.x + 4.0 * u, y, area.w - 8.0 * u, 22.0 * u, Color::new(GOLD.r, GOLD.g, GOLD.b, 0.12 * fresh * 2.0));
        }
        let line = change_text(assets, who_s, c);
        draw_circle(area.x + 13.0 * u, y + 11.0 * u, 2.6 * u, col);
        text_fit(&line, area.x + 22.0 * u, y + 16.0 * u, 15.0 * u, area.w - 30.0 * u, col);
    }
}

/// The two steps with a lock or a seal for each side, the buttons and what to do next.
fn steps(game: &mut Game, t: &PlayerTrade, phase: TradePhase, assets: &Assets, p: Rect, act: bool, u: f32) {
    let both = t.i_confirmed && t.they_confirmed;
    let name: String = t.name.chars().take(9).collect();
    let banner = flash(aux(|a| a.trade_banner));
    let card_h = 100.0 * u;
    let _ = assets;
    for (k, (title, me, they, open)) in [("1 · Confirmar", t.i_confirmed, t.they_confirmed, true), ("2 · Aceptar", t.i_accepted, t.they_accepted, both)].into_iter().enumerate() {
        let cr = Rect::new(p.x, p.y + k as f32 * (card_h + 8.0 * u), p.w, card_h);
        let done = me && they;
        draw_rectangle(cr.x, cr.y, cr.w, cr.h, Color::new(0.07, 0.05, 0.035, 0.95));
        draw_rectangle(cr.x, cr.y, cr.w, cr.h * 0.4, Color::new(1.0, 0.9, 0.7, 0.03));
        let edge = if done { GOOD } else if open { GOLD } else { GOLD_DIM };
        draw_rectangle_lines(cr.x, cr.y, cr.w, cr.h, if done { 2.0 * u } else { 1.0 }, Color::new(edge.r, edge.g, edge.b, if open { 0.85 } else { 0.45 }));
        let tc = if open { GOLD_TEXT } else { TEXT_FAINT };
        widgets::text_centered_with(FontKind::Title, title, cr.x + cr.w / 2.0, cr.y + 21.0 * u, 16.0 * u, tc);
        for (j, (done_side, label)) in [(me, "Tú".to_string()), (they, name.clone())].into_iter().enumerate() {
            let c = vec2(cr.x + cr.w * (0.27 + 0.46 * j as f32), cr.y + 56.0 * u);
            if k == 0 {
                padlock(c, 15.0 * u, done_side && open, u);
            } else {
                seal(c, 16.0 * u, done_side && open, open, u);
            }
            let d = measure(&label, 14.0 * u);
            text(&label, c.x - d.width / 2.0, cr.y + 91.0 * u, 14.0 * u, if done_side && open { GOOD } else { TEXT_DIM });
        }
    }
    // The buttons.
    let by = p.y + 2.0 * (card_h + 8.0 * u) + 4.0 * u;
    let bh = 32.0 * u;
    let can_confirm = game.plan_confirm().is_ok();
    let can_accept = game.plan_accept().is_ok();
    let step1_kind = if can_confirm { Kind::Primary } else { Kind::Secondary };
    if ibtn_kind(Rect::new(p.x, by, p.w, bh), "Confirmar oferta", step1_kind, can_confirm, act, u) {
        game.trade_confirm();
    }
    let step2_kind = if can_accept { Kind::Primary } else { Kind::Secondary };
    if ibtn_kind(Rect::new(p.x, by + bh + 6.0 * u, p.w, bh), "Aceptar trato", step2_kind, can_accept, act, u) {
        game.trade_accept();
    }
    if ibtn_kind(Rect::new(p.x, by + 2.0 * (bh + 6.0 * u), p.w, 28.0 * u), "Cancelar comercio", Kind::Danger, true, act, u) {
        game.trade_cancel();
    }
    // What to do next, in words; a red notice for a moment after a change wiped the seals.
    let my = by + 2.0 * (bh + 6.0 * u) + 28.0 * u + 12.0 * u;
    if banner > 0.0 {
        let msg = "¡La oferta cambió! Se anularon las confirmaciones: revisen y confirmen otra vez.".to_string();
        let lines = wrap_lines(FontKind::Body, &msg, 15.0 * u, p.w - 12.0 * u);
        let bh2 = lines.len() as f32 * 17.0 * u + 12.0 * u;
        draw_rectangle(p.x, my, p.w, bh2, Color::new(BAD.r * 0.5, BAD.g * 0.3, BAD.b * 0.3, 0.55 * banner));
        draw_rectangle_lines(p.x, my, p.w, bh2, 1.0, Color::new(BAD.r, BAD.g, BAD.b, 0.9 * banner));
        for (i, l) in lines.iter().enumerate() {
            text(l, p.x + 6.0 * u, my + 17.0 * u + i as f32 * 17.0 * u, 15.0 * u, Color::new(1.0, 0.85, 0.8, banner.max(0.55)));
        }
        return;
    }
    let msg = match phase {
        TradePhase::Building => "Pongan lo que quieran en las mesas. Si algo cambia, se anulan las confirmaciones.".to_string(),
        TradePhase::WaitingThem => format!("Esperando que {} confirme su oferta.", t.name),
        TradePhase::WaitingMe => format!("{} ya confirmó. Revisa su mesa y confirma la tuya.", t.name),
        TradePhase::BothConfirmed => "Los dos confirmaron. Ahora, «Aceptar trato» cierra el intercambio.".to_string(),
        TradePhase::WaitingAccept => format!("Aceptaste. Esperando que {} acepte.", t.name),
        TradePhase::TheyAccepted => format!("{} ya aceptó. Pulsa «Aceptar trato» para terminar.", t.name),
    };
    let col = if matches!(phase, TradePhase::BothConfirmed | TradePhase::TheyAccepted | TradePhase::WaitingMe) { GOLD_TEXT } else { TEXT_DIM };
    for (i, l) in wrap_lines(FontKind::NarrativeItalic, &msg, 15.0 * u, p.w - 4.0 * u).iter().enumerate() {
        text_with(FontKind::NarrativeItalic, l, p.x + 2.0 * u, my + 14.0 * u + i as f32 * 17.0 * u, 15.0 * u, col);
    }
    let _ = chip;
}

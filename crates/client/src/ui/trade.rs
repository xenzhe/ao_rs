//! Window: trade between players (offers, double confirmation). OWNER: Sonnet 2 (docs/P3_CONTRATO.md).
//!
//! Two columns: yours (offer + your inventory to add things from) and theirs (offer + the
//! state of the double confirmation). Any change of either offer clears the confirmations,
//! so the window says in words what step you are in and what is missing.
//!
//! Hooks for the world: [`draw_overlay`] (the invitation banner, needs one call per frame
//! from `ui::frame`) and [`Game::trade_with_tile`] (a click on a player, see `input.rs`).
use super::bank::item_name;
use super::inventory::{self as inv, active, btn_w, draw_grid, fmt_gold, handle, ibtn, item_tip, mouse, open_prompt, selected_inv, track, Grid, GridView, Hit, Pending, Zone};
use crate::assets::Assets;
use crate::game::Game;
use crate::render::layout::Layout;
use crate::ui::theme::{BAD, GOLD, GOLD_TEXT, GOOD, MUTED, SLOT, TEXT};
use crate::ui::widgets::{measure, panel, text, text_fit};
use crate::ui::{Ui, WindowId};
use client_core::p3::items::{trade_phase, TradePhase, MAX_OFFER_ITEMS};
use client_core::social::trade::{Offer, PlayerTrade};
use macroquad::prelude::*;
use std::cell::Cell;

thread_local! {
    static OVERLAY_AT: Cell<f64> = const { Cell::new(-10.0) };
}

/// Whether `draw_overlay` is being called every frame (the integrator wired it).
fn overlay_wired() -> bool {
    get_time() - OVERLAY_AT.with(|t| t.get()) < 0.5
}

/// Invitation banner for the whole screen: "X wants to trade [Accept] [Decline]". Call it
/// every frame from `ui::frame`, after the windows (it is the only way to answer an
/// invitation while no item window is open).
#[allow(dead_code)] // until the integrator wires it into ui::frame (see the P3 report)
pub fn draw_overlay(ui: &mut Ui, game: &mut Game) {
    OVERLAY_AT.with(|t| t.set(get_time()));
    inv::purge(ui);
    if game.local.social.trade.invited_by.is_none() || game.local.social.trade.player.is_some() {
        return;
    }
    let u = ui.u;
    let (w, h) = (400.0 * u, 44.0 * u);
    let r = Rect::new((screen_width() / 2.0 - w / 2.0 - 116.0 * u).max(4.0), Layout::compute(false).console.bottom() + 12.0 * u, w, h);
    panel(r, u);
    ui.capture_mouse_if_over(r);
    invite_banner(game, Rect::new(r.x + 10.0 * u, r.y + 8.0 * u, r.w - 20.0 * u, r.h - 16.0 * u), true, u);
}

/// "X te invita a comerciar" with Accept / Decline inside `r`.
fn invite_banner(game: &mut Game, r: Rect, act: bool, u: f32) {
    let Some((_, name)) = game.local.social.trade.invited_by.clone() else { return };
    let (a, d) = (btn_w("Aceptar", u), btn_w("Rechazar", u));
    text_fit(&format!("{name} te invita a comerciar"), r.x, r.y + r.h / 2.0 + 5.5 * u, 17.0 * u, r.w - a - d - 14.0 * u, GOLD_TEXT);
    let dr = Rect::new(r.right() - d, r.y, d, r.h);
    if ibtn(dr, "Rechazar", true, act, u) {
        game.trade_decline();
    }
    if ibtn(Rect::new(dr.x - 4.0 * u - a, r.y, a, r.h), "Aceptar", true, act, u) {
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
        return text_fit("Comerciar: no hay jugadores cerca", row.x + 4.0 * u, row.y + 18.0 * u, 16.0 * u, row.w, MUTED);
    }
    text("Comerciar con:", row.x + 4.0 * u, row.y + 18.0 * u, 16.0 * u, MUTED);
    let mut x = row.x + 4.0 * u + measure("Comerciar con:", 16.0 * u).width + 8.0 * u;
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

/// Word wrap of `s` into lines of at most `w` pixels.
fn wrap(s: &str, size: f32, w: f32) -> Vec<String> {
    let mut lines = Vec::new();
    let mut cur = String::new();
    for word in s.split_whitespace() {
        let t = if cur.is_empty() { word.to_string() } else { format!("{cur} {word}") };
        if measure(&t, size).width > w && !cur.is_empty() {
            lines.push(std::mem::take(&mut cur));
            cur = word.to_string();
        } else {
            cur = t;
        }
    }
    if !cur.is_empty() {
        lines.push(cur);
    }
    lines
}

const ROW: f32 = 26.0;

/// An offer: 6 rows of objects and the gold line. Returns (retract index, hovered tooltip).
#[allow(clippy::too_many_arguments)]
fn offer_column(ui: &Ui, game: &Game, assets: &mut Assets, x: f32, y: f32, w: f32, offer: &Offer, mine: bool, act: bool) -> (Option<u8>, Option<Vec<String>>) {
    let u = ui.u;
    let m = mouse();
    let (mut retract, mut tip) = (None, None);
    for i in 0..MAX_OFFER_ITEMS {
        let rr = Rect::new(x, y + i as f32 * ROW * u, w, (ROW - 2.0) * u);
        draw_rectangle(rr.x, rr.y, rr.w, rr.h, Color::new(0.03, 0.025, 0.02, 0.6));
        draw_rectangle_lines(rr.x, rr.y, rr.w, rr.h, 1.0, Color::new(0.25, 0.19, 0.11, 1.0));
        let Some((obj, n)) = offer.items.get(i).copied() else { continue };
        let icon = Rect::new(rr.x + 2.0 * u, rr.y + 1.0 * u, 22.0 * u, 22.0 * u);
        draw_rectangle(icon.x, icon.y, icon.w, icon.h, SLOT);
        assets.draw_obj(obj, Rect::new(icon.x + 1.0, icon.y + 1.0, icon.w - 2.0, icon.h - 2.0));
        let q = format!("x{}", fmt_gold(n as u64));
        let qw = measure(&q, 16.0 * u).width;
        let xw = if mine { 24.0 * u } else { 0.0 };
        text_fit(&item_name(assets, obj), rr.x + 30.0 * u, rr.y + 17.0 * u, 16.0 * u, rr.w - 30.0 * u - qw - xw - 12.0 * u, TEXT);
        text(&q, rr.right() - qw - xw - 6.0 * u, rr.y + 17.0 * u, 16.0 * u, GOLD_TEXT);
        if mine && ibtn(Rect::new(rr.right() - 22.0 * u, rr.y + 1.0 * u, 20.0 * u, rr.h - 2.0 * u), "x", true, act, u) {
            retract = Some(i as u8);
        }
        if act && rr.contains(m) && ui.drag.is_none() {
            tip = Some(item_tip(game, assets, obj, n, false, &[]));
        }
    }
    (retract, tip)
}

fn dot(cx: f32, cy: f32, on: bool, u: f32) {
    if on {
        draw_circle(cx, cy, 6.0 * u, GOOD);
    } else {
        draw_circle_lines(cx, cy, 5.5 * u, 1.5, MUTED);
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
    let (cw, x0) = (262.0 * u, r.x);
    let x1 = r.x + 274.0 * u;
    let phase = trade_phase(&t);
    let list_y = r.y + 24.0 * u;
    let gold_y = list_y + MAX_OFFER_ITEMS as f32 * ROW * u + 4.0 * u;

    // ---- my offer ----
    text("Tu oferta", x0 + 2.0 * u, r.y + 16.0 * u, 19.0 * u, GOLD_TEXT);
    let (retract, tp) = offer_column(ui, game, assets, x0, list_y, cw, &t.mine, true, act);
    tip = tip.or(tp);
    zones.push((Zone::OfferMine, Rect::new(x0, list_y, cw, MAX_OFFER_ITEMS as f32 * ROW * u)));
    if let Some(i) = retract {
        game.trade_retract(i);
    }
    text(&format!("Oro: {}", fmt_gold(t.mine.gold as u64)), x0 + 4.0 * u, gold_y + 18.0 * u, 18.0 * u, GOLD_TEXT);
    let (g1, g2) = (btn_w("Cambiar", u), btn_w("Quitar", u));
    let gb = Rect::new(x0 + cw - g1, gold_y, g1, 24.0 * u);
    if ibtn(gb, "Cambiar", game.local.gold > 0, act, u) {
        open_prompt(w, Pending::OfferGold, "Oro que ofreces", game.local.gold, t.mine.gold.clamp(1, game.local.gold.max(1)), None);
    }
    if ibtn(Rect::new(gb.x - 4.0 * u - g2, gold_y, g2, 24.0 * u), "Quitar", t.mine.gold > 0, act, u) {
        game.trade_offer_gold(0);
    }

    // ---- my inventory: add things to the offer ----
    let iy = gold_y + 30.0 * u;
    text("Tu inventario (arrastra o doble clic)", x0 + 2.0 * u, iy + 12.0 * u, 15.0 * u, MUTED);
    let grid = Grid { cols: 10, rows: 5, cell: 24.0 * u, gap: 2.0 * u };
    let io = vec2(x0 + (cw - grid.size().x) / 2.0, iy + 18.0 * u);
    let sel_i = selected_inv();
    let (hit, z2) = draw_grid(ui, assets, &grid, io, &GridView { slots: &inventory, from: WindowId::Inventory, selected: sel_i, locked_from: usize::MAX }, act);
    zones.extend(z2);
    if let (Some(h), None) = (hit, &ui.drag) {
        let eq = game.local.slot(h.slot).is_some_and(|i| i.equipped);
        tip = Some(item_tip(game, assets, h.obj, h.amount, eq, &["Doble clic: ofrecer".to_string()]));
    }
    if let Some(ev) = track(ui, w, act, hit.map(|h: Hit| h)) {
        handle(game, assets, w, ev);
    }

    // ---- their offer ----
    let who: String = t.name.clone();
    text_fit(&format!("Oferta de {who}"), x1 + 2.0 * u, r.y + 16.0 * u, 19.0 * u, cw, GOLD_TEXT);
    let (_, tp) = offer_column(ui, game, assets, x1, list_y, cw, &t.theirs, false, act);
    tip = tip.or(tp);
    text(&format!("Oro: {}", fmt_gold(t.theirs.gold as u64)), x1 + 4.0 * u, gold_y + 18.0 * u, 18.0 * u, GOLD_TEXT);

    // ---- the double confirmation ----
    status_panel(game, &t, phase, Rect::new(x1, gold_y + 30.0 * u, cw, r.bottom() - (gold_y + 30.0 * u)), act, u);

    inv::register(w, r, u, zones);
    inv::menu_ui(ui, game, assets, w);
    inv::prompt_ui(ui, game, w, r);
    if let Some(tp) = tip {
        if ui.drag.is_none() && act {
            inv::tooltip_at(&tp, m, u);
        }
    }
}

/// Steps, who did what, instructions and the buttons.
fn status_panel(game: &mut Game, t: &PlayerTrade, phase: TradePhase, p: Rect, act: bool, u: f32) {
    draw_rectangle(p.x, p.y, p.w, p.h, Color::new(0.03, 0.025, 0.02, 0.6));
    draw_rectangle_lines(p.x, p.y, p.w, p.h, 1.0, GOLD);
    let both = t.i_confirmed && t.they_confirmed;
    let name: String = t.name.chars().take(10).collect();
    let steps = [("1  Confirmar", t.i_confirmed, t.they_confirmed, true), ("2  Aceptar", t.i_accepted, t.they_accepted, both)];
    for (k, (label, me, they, open)) in steps.iter().enumerate() {
        let y = p.y + 6.0 * u + k as f32 * 24.0 * u;
        text(label, p.x + 8.0 * u, y + 17.0 * u, 17.0 * u, if *open { TEXT } else { MUTED });
        let tw = measure(&name, 15.0 * u).width;
        let rx = p.right() - 8.0 * u - tw;
        dot(rx - 12.0 * u, y + 11.0 * u, *they && *open, u);
        text(&name, rx, y + 16.0 * u, 15.0 * u, MUTED);
        let yw = measure("Tú", 15.0 * u).width;
        let lx = rx - 24.0 * u - yw - 12.0 * u;
        dot(lx - 12.0 * u, y + 11.0 * u, *me && *open, u);
        text("Tú", lx, y + 16.0 * u, 15.0 * u, MUTED);
    }
    let msg = match phase {
        TradePhase::Building => "Arma las ofertas y confirma. Si algo cambia, se anulan las confirmaciones.".to_string(),
        TradePhase::WaitingThem => format!("Esperando que {} confirme la oferta.", t.name),
        TradePhase::WaitingMe => format!("{} ya confirmó. Revisa su oferta y confirma la tuya.", t.name),
        TradePhase::BothConfirmed => "Los dos confirmaron. Pulsa «Aceptar trato» para cerrar el intercambio.".to_string(),
        TradePhase::WaitingAccept => format!("Aceptaste. Esperando que {} acepte.", t.name),
        TradePhase::TheyAccepted => format!("{} ya aceptó. Pulsa «Aceptar trato» para terminar.", t.name),
    };
    let col = if matches!(phase, TradePhase::BothConfirmed | TradePhase::TheyAccepted | TradePhase::WaitingMe) { GOLD_TEXT } else { TEXT };
    let mut y = p.y + 58.0 * u;
    for l in wrap(&msg, 15.0 * u, p.w - 16.0 * u) {
        text(&l, p.x + 8.0 * u, y + 12.0 * u, 15.0 * u, col);
        y += 16.0 * u;
    }
    let by = p.bottom() - 56.0 * u;
    let hw = (p.w - 16.0 * u - 4.0 * u) / 2.0;
    let confirm = Rect::new(p.x + 8.0 * u, by, hw, 24.0 * u);
    if ibtn(confirm, "Confirmar oferta", game.plan_confirm().is_ok(), act, u) {
        game.trade_confirm();
    }
    if ibtn(Rect::new(confirm.right() + 4.0 * u, by, hw, 24.0 * u), "Aceptar trato", game.plan_accept().is_ok(), act, u) {
        game.trade_accept();
    }
    if ibtn(Rect::new(p.x + 8.0 * u, by + 28.0 * u, p.w - 16.0 * u, 24.0 * u), "Cancelar comercio", true, act, u) {
        game.trade_cancel();
    }
    let _ = BAD;
}

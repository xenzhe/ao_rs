//! Window: the vault (deposit/withdraw items and gold). OWNER: Sonnet 2 (docs/P3_CONTRATO.md).
//!
//! Two chests side by side: the vault on the left, your bag on the right. Drag from one to
//! the other (or double click, the right-click menu); a quantity prompt asks "how many" when
//! there is more than one unit. The gold sits under them: what is kept and what you carry,
//! and between them a field with "Retirar" and "Guardar". Closing sends `BankClose`.
use super::inventory::{self as inv, active, btn_w, chip, draw_grid, fmt_gold, glyph, handle, ibtn, ibtn_kind, item_tip, mouse, selected_inv, selected_other, settle_tx, aux, track, watch_gold, Glyph, Grid, GridView};
use crate::assets::Assets;
use crate::game::Game;
use crate::ui::fx_ui::{toast, ToastKind};
use crate::ui::theme::{self, FontKind, GOLD, GOLD_BRIGHT, TEXT, TEXT_DIM, TEXT_FAINT};
use crate::ui::widgets::{self, measure, text, text_with, Kind};
use crate::ui::{Ui, WindowId};
use client_core::p3::items::usable_slots;
use macroquad::prelude::*;

pub(crate) use super::inventory::item_name;

/// One gold figure: coin, big number, caption.
fn gold_block(x: f32, y: f32, amount: u32, caption: &str, right: bool, u: f32) -> f32 {
    let s = fmt_gold(amount as u64);
    let size = 26.0 * u;
    let tw = widgets::measure_with(FontKind::Title, &s, size).width;
    let total = 38.0 * u + tw;
    let x0 = if right { x - total } else { x };
    glyph(Glyph::Coin, vec2(x0 + 14.0 * u, y + 14.0 * u), 13.0 * u, GOLD_BRIGHT);
    text_with(FontKind::Title, &s, x0 + 36.0 * u, y + 23.0 * u, size, GOLD_BRIGHT);
    text(caption, x0 + 36.0 * u, y + 42.0 * u, 14.0 * u, TEXT_DIM);
    total
}

/// Draws the content of the window inside `r` (the frame is already drawn).
pub fn draw(ui: &mut Ui, game: &mut Game, assets: &mut Assets, r: Rect) {
    let u = ui.u;
    let w = WindowId::Bank;
    inv::begin(ui, game, assets);
    settle_tx(game, assets);
    let Some(bank) = game.local.bank.as_ref() else { return };
    let (vault, vgold) = (bank.slots.clone(), bank.gold);
    let inventory = game.local.inventory.clone();
    let act = active(ui, w);
    let m = mouse();
    let mut zones = Vec::new();
    let mut tip: Option<Vec<String>> = None;
    let bp = game.local.inventory.iter().flatten().any(|i| i.equipped && assets.objs.get(i.obj as u32).is_some_and(|o| o.obj_type == ao_formats::dat::obj::ObjType::Backpack));
    let usable = usable_slots(bp);

    let pane_w = (r.w - 30.0 * u) / 2.0;
    let (lx, rx) = (r.x, r.x + pane_w + 30.0 * u);
    // ---- headings ----
    let used_v = vault.iter().filter(|s| s.is_some()).count();
    let used_i = inventory.iter().take(usable).filter(|s| s.is_some()).count();
    let top = theme::section("Tu bóveda", lx, r.y - 4.0 * u, pane_w, u);
    theme::section("Tu mochila", rx, r.y - 4.0 * u, pane_w, u);
    for (x, s) in [(lx, format!("{used_v} / {}", vault.len())), (rx, format!("{used_i} / {usable}"))] {
        let d = measure(&s, 16.0 * u);
        text(&s, x + pane_w - d.width, r.y + 14.0 * u, 16.0 * u, TEXT_DIM);
    }
    // ---- the two chests ----
    let gh = r.h - (top - r.y) - 130.0 * u;
    let grid = Grid::fit(7, 6, pane_w - 12.0 * u, gh, 3.0 * u, 50.0 * u);
    let vo = vec2(lx + 6.0 * u + (pane_w - 12.0 * u - grid.size().x) / 2.0, top + 6.0 * u);
    let io = vec2(rx + 6.0 * u + (pane_w - 12.0 * u - grid.size().x) / 2.0, top + 6.0 * u);
    let sel_v = selected_other(w);
    let (h1, z1) = draw_grid(ui, assets, &grid, vo, &GridView::plain(&vault, WindowId::Bank, sel_v, usize::MAX), act);
    zones.extend(z1);
    let (h2, z2) = draw_grid(ui, assets, &grid, io, &GridView::plain(&inventory, WindowId::Inventory, selected_inv(), usable), act);
    zones.extend(z2);
    // An arrow pair between the chests says it goes both ways.
    let mid = vec2((lx + pane_w + rx) / 2.0, vo.y + grid.size().y / 2.0);
    for (dy, dir) in [(-9.0, 1.0f32), (9.0, -1.0)] {
        let a = vec2(mid.x, mid.y + dy * u);
        draw_triangle(a + vec2(-dir * 7.0 * u, -6.0 * u), a + vec2(-dir * 7.0 * u, 6.0 * u), a + vec2(dir * 7.0 * u, 0.0), Color::new(GOLD.r, GOLD.g, GOLD.b, 0.7));
    }
    let hy = vo.y + grid.size().y + 22.0 * u;
    text("Arrastra aquí lo que quieras guardar", lx, hy, 14.0 * u, TEXT_FAINT);
    text("Arrastra aquí lo que quieras sacar", rx, hy, 14.0 * u, TEXT_FAINT);

    // ---- the gold: what is kept, what you carry, and how much to move ----
    let gy = hy + 14.0 * u;
    theme::divider(r.x + r.w / 2.0, gy, r.w * 0.96, u);
    let gy = gy + 12.0 * u;
    gold_block(lx, gy, vgold, "en la bóveda", false, u);
    gold_block(r.right(), gy, game.local.gold, "contigo", true, u);
    let cw = 120.0 * u;
    let cx = r.x + r.w / 2.0;
    let field = Rect::new(cx - cw / 2.0, gy + 2.0 * u, cw, 34.0 * u);
    let mut gtxt = aux(|a| std::mem::take(&mut a.gold_text));
    let enter = widgets::text_field(ui, "bank_gold", &mut gtxt, field, 9, false, |c| c.is_ascii_digit());
    if gtxt.is_empty() && ui.focus.as_deref() != Some("bank_gold") {
        widgets::placeholder(field, "cantidad", u);
    }
    let amount = client_core::p3::items::parse_qty(&gtxt, u32::MAX);
    let (bw, bh) = (116.0 * u, 34.0 * u);
    let take = Rect::new(field.x - 8.0 * u - bw, field.y, bw, bh);
    let put = Rect::new(field.right() + 8.0 * u, field.y, bw, bh);
    let can_take = amount.is_some() && vgold > 0;
    let can_put = amount.is_some() && game.local.gold > 0;
    if ibtn(take, "Retirar", can_take, act, u) {
        if let Some(n) = amount {
            watch_gold(game, false);
            game.bank_withdraw_gold(n);
        }
    }
    if ibtn_kind(put, "Guardar", Kind::Primary, can_put, act, u) || (enter && can_put) {
        if let Some(n) = amount {
            watch_gold(game, true);
            game.bank_deposit_gold(n);
        }
    }
    if enter && !can_put && amount.is_some() {
        toast("No llevas oro para guardar", ToastKind::Warn);
    }
    // Quick amounts and "everything".
    let qy = field.bottom() + 8.0 * u;
    let mut x = cx - (3.0 * 62.0 * u + 2.0 * 4.0 * u) / 2.0;
    for (label, v) in [("1.000", 1_000u32), ("10.000", 10_000), ("100.000", 100_000)] {
        if chip(Rect::new(x, qy, 62.0 * u, 24.0 * u), label, gtxt == v.to_string(), act, u) {
            gtxt = v.to_string();
        }
        x += 66.0 * u;
    }
    aux(|a| a.gold_text = gtxt);
    let tw = btn_w("Retirar todo", u);
    if ibtn_kind(Rect::new(lx + 24.0 * u, qy + 12.0 * u, tw, 24.0 * u), "Retirar todo", Kind::Ghost, vgold > 0, act, u) {
        watch_gold(game, false);
        game.bank_withdraw_gold(vgold);
    }
    let tw2 = btn_w("Guardar todo", u);
    if ibtn_kind(Rect::new(r.right() - tw2 - 14.0 * u, qy + 12.0 * u, tw2, 24.0 * u), "Guardar todo", Kind::Ghost, game.local.gold > 0, act, u) {
        watch_gold(game, true);
        game.bank_deposit_gold(game.local.gold);
    }
    // The window's X closes the vault (`ui::auto_windows` tells the server).

    // ---- input ----
    let hit = h1.or(h2);
    if let (Some(h), None) = (hit, &ui.drag) {
        let eq = h.from == WindowId::Inventory && game.local.slot(h.slot).is_some_and(|i| i.equipped);
        let extra = if h.from == WindowId::Inventory { "Doble clic: guardar · arrastra a la bóveda" } else { "Doble clic: retirar · arrastra a tu mochila" };
        tip = Some(item_tip(game, assets, h.obj, h.amount, eq, &[extra.to_string()]));
    }
    if let Some(ev) = track(ui, w, act, hit) {
        handle(game, assets, w, ev);
    }
    inv::register(w, r, u, zones);
    inv::menu_ui(ui, game, assets, w);
    inv::prompt_ui(ui, game, assets, w, r);
    if let Some(t) = tip {
        if ui.drag.is_none() && act {
            inv::tooltip_at(&t, m, u);
        }
    }
    let _ = (GOLD, TEXT);
}

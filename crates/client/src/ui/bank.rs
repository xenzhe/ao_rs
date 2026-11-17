//! Window: the vault (deposit/withdraw items and gold). OWNER: Sonnet 2 (docs/P3_CONTRATO.md).
//!
//! Two grids (the vault and the inventory): drag from one to the other, double click, the
//! right-click menu or the buttons under each grid. A quantity prompt asks "how many" when
//! there is more than one unit. Closing sends `BankClose`.
use super::inventory::{self as inv, active, btn_w, draw_grid, fmt_gold, handle, ibtn, item_tip, mouse, open_prompt, selected_inv, selected_other, track, Grid, GridView, Op, Pending};
use crate::assets::Assets;
use crate::game::Game;
use crate::ui::theme::{GOLD_TEXT, MUTED, TEXT};
use crate::ui::widgets::{text, text_fit};
use crate::ui::{Ui, WindowId};
use macroquad::prelude::*;

pub(crate) fn item_name(assets: &Assets, obj: u16) -> String {
    assets.objs.get(obj as u32).map(|o| o.name.clone()).unwrap_or_else(|| format!("Objeto {obj}"))
}

/// Draws the content of the window inside `r` (the frame is already drawn).
pub fn draw(ui: &mut Ui, game: &mut Game, assets: &mut Assets, r: Rect) {
    let u = ui.u;
    let w = WindowId::Bank;
    inv::begin(ui, game, assets);
    let Some(bank) = game.local.bank.as_ref() else { return };
    let (vault, vgold) = (bank.slots.clone(), bank.gold);
    let inventory = game.local.inventory.clone();
    let act = active(ui, w);
    let m = mouse();
    let mut zones = Vec::new();
    let mut tip: Option<Vec<String>> = None;
    let grid = Grid { cols: 11, rows: 4, cell: 26.0 * u, gap: 2.0 * u };
    let ox = r.x + (r.w - grid.size().x) / 2.0;

    // ---- the vault ----
    text("Bóveda", r.x + 4.0 * u, r.y + 16.0 * u, 19.0 * u, GOLD_TEXT);
    let s = format!("{} oro guardado", fmt_gold(vgold as u64));
    text(&s, r.right() - 4.0 * u - crate::ui::widgets::measure(&s, 17.0 * u).width, r.y + 16.0 * u, 17.0 * u, TEXT);
    let vo = vec2(ox, r.y + 22.0 * u);
    let sel_v = selected_other(w);
    let (h1, z1) = draw_grid(ui, assets, &grid, vo, &GridView { slots: &vault, from: WindowId::Bank, selected: sel_v, locked_from: usize::MAX }, act);
    zones.extend(z1);
    let ay = vo.y + grid.size().y + 4.0 * u;
    let sel_item = vault.get(sel_v.wrapping_sub(1) as usize).copied().flatten();
    let (b1, b2) = (btn_w("Retirar", u), btn_w("Retirar oro", u));
    let name_w = r.w - b1 - b2 - 12.0 * u;
    match sel_item {
        Some(it) => text_fit(&format!("{} x{}", item_name(assets, it.obj), it.amount), r.x + 4.0 * u, ay + 17.0 * u, 16.0 * u, name_w, TEXT),
        None => text_fit("Elige un objeto", r.x + 4.0 * u, ay + 17.0 * u, 16.0 * u, name_w, MUTED),
    }
    let by = Rect::new(r.right() - b2, ay, b2, 24.0 * u);
    if ibtn(by, "Retirar oro", vgold > 0, act, u) {
        open_prompt(w, Pending::WithdrawGold, "Retirar oro", vgold, vgold.min(1), None);
    }
    let bi = Rect::new(by.x - 4.0 * u - b1, ay, b1, 24.0 * u);
    if ibtn(bi, "Retirar", sel_item.is_some(), act, u) {
        if let Some(it) = sel_item {
            let hit = inv::Hit { from: WindowId::Bank, slot: sel_v, obj: it.obj, amount: it.amount };
            inv::start(game, assets, Op::Withdraw, hit, w);
        }
    }

    // ---- the inventory ----
    let y2 = ay + 52.0 * u;
    text("Tu inventario", r.x + 4.0 * u, y2 + 14.0 * u, 19.0 * u, GOLD_TEXT);
    let s = format!("{} oro encima", fmt_gold(game.local.gold as u64));
    text(&s, r.right() - 4.0 * u - crate::ui::widgets::measure(&s, 17.0 * u).width, y2 + 14.0 * u, 17.0 * u, TEXT);
    let io = vec2(ox, y2 + 20.0 * u);
    let sel_i = selected_inv();
    let (h2, z2) = draw_grid(ui, assets, &grid, io, &GridView { slots: &inventory, from: WindowId::Inventory, selected: sel_i, locked_from: usize::MAX }, act);
    zones.extend(z2);
    let ay2 = io.y + grid.size().y + 4.0 * u;
    let sel_inv_item = inventory.get(sel_i.wrapping_sub(1) as usize).copied().flatten();
    let (c1, c2, c3) = (btn_w("Guardar", u), btn_w("Guardar oro", u), btn_w("Cerrar", u));
    let can_store = sel_inv_item.is_some_and(|i| !i.equipped);
    let close = Rect::new(r.right() - c3, ay2, c3, 24.0 * u);
    if ibtn(close, "Cerrar", true, act, u) {
        game.bank_close();
    }
    let gold_b = Rect::new(close.x - 4.0 * u - c2, ay2, c2, 24.0 * u);
    if ibtn(gold_b, "Guardar oro", game.local.gold > 0, act, u) {
        open_prompt(w, Pending::DepositGold, "Guardar oro", game.local.gold, game.local.gold.min(1), None);
    }
    let item_b = Rect::new(gold_b.x - 4.0 * u - c1, ay2, c1, 24.0 * u);
    if ibtn(item_b, "Guardar", can_store, act, u) {
        if let Some(it) = sel_inv_item {
            let hit = inv::Hit { from: WindowId::Inventory, slot: sel_i, obj: it.obj, amount: it.amount };
            inv::start(game, assets, Op::Deposit, hit, w);
        }
    }

    // ---- input ----
    let hit = h1.or(h2);
    if let (Some(h), None) = (hit, &ui.drag) {
        let eq = h.from == WindowId::Inventory && game.local.slot(h.slot).is_some_and(|i| i.equipped);
        let extra = if h.from == WindowId::Inventory { "Doble clic: guardar" } else { "Doble clic: retirar" };
        tip = Some(item_tip(game, assets, h.obj, h.amount, eq, &[extra.to_string()]));
    }
    if let Some(ev) = track(ui, w, act, hit) {
        handle(game, assets, w, ev);
    }
    inv::register(w, r, u, zones);
    inv::menu_ui(ui, game, assets, w);
    inv::prompt_ui(ui, game, w, r);
    if let Some(t) = tip {
        if ui.drag.is_none() && act {
            inv::tooltip_at(&t, m, u);
        }
    }
}

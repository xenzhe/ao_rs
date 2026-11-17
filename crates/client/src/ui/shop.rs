//! Window: the NPC merchant (buy and sell). OWNER: Sonnet 2 (docs/P3_CONTRATO.md).
//!
//! Stock with prices on top (what the server computes for this player, commerce skill
//! included), the inventory below with the sale price. Drag from the stock to the inventory
//! to buy and the other way round to sell, double click, right-click menu or the buttons; a
//! quantity prompt (with the total) asks "how many".
use super::bank::item_name;
use super::inventory::{self as inv, active, btn_w, draw_cell, draw_grid, fmt_gold, handle, ibtn, item_tip, mouse, scroll_of, selected_inv, selected_other, set_scroll, track, Grid, GridView, Hit, Op, Zone};
use crate::assets::Assets;
use crate::game::Game;
use crate::ui::theme::{BAD, GOLD, GOLD_TEXT, MUTED, SLOT, TEXT};
use crate::ui::widgets::{measure, text, text_fit};
use crate::ui::{Ui, WindowId};
use client_core::p3::items::sale_value;
use macroquad::prelude::*;

const ROWS: usize = 4;

/// What the merchant pays per unit of `obj` (0 = does not buy it).
fn sale_unit(assets: &Assets, obj: u16) -> u64 {
    assets.objs.get(obj as u32).map_or(0, |o| sale_value(o, 1))
}

/// Draws the content of the window inside `r` (the frame is already drawn).
pub fn draw(ui: &mut Ui, game: &mut Game, assets: &mut Assets, r: Rect) {
    let u = ui.u;
    let w = WindowId::Shop;
    inv::begin(ui, game, assets);
    let Some(shop) = game.local.social.trade.shop.clone() else { return };
    let inventory = game.local.inventory.clone();
    let act = active(ui, w);
    let m = mouse();
    let mut zones = Vec::new();
    let mut tip: Option<Vec<String>> = None;
    let gold = game.local.gold;

    // ---- header ----
    // "Grom <Herrero Brutal>": the window title already says what he is.
    let who = game.ents.get(&shop.npc).map(|e| e.name.split('<').next().unwrap_or("").trim().to_string()).filter(|n| !n.is_empty()).unwrap_or_else(|| "Comerciante".into());
    let cw = btn_w("Cerrar", u);
    let close = Rect::new(r.right() - cw, r.y - 2.0 * u, cw, 22.0 * u);
    if ibtn(close, "Cerrar", true, act, u) {
        game.shop_close();
    }
    let s = format!("Tu oro: {}", fmt_gold(gold as u64));
    let gx = close.x - 10.0 * u - measure(&s, 17.0 * u).width;
    text(&s, gx, r.y + 16.0 * u, 17.0 * u, TEXT);
    text_fit(&who, r.x + 4.0 * u, r.y + 16.0 * u, 19.0 * u, gx - r.x - 14.0 * u, GOLD_TEXT);

    // ---- stock list ----
    let list = Rect::new(r.x, r.y + 24.0 * u, r.w, ROWS as f32 * 28.0 * u);
    draw_rectangle(list.x, list.y, list.w, list.h, Color::new(0.03, 0.025, 0.02, 0.6));
    let stock: Vec<(u8, client_core::social::trade::ShopItem)> = shop.items.iter().map(|(s, i)| (*s, *i)).collect();
    let mut scroll = scroll_of(w).min(stock.len().saturating_sub(ROWS));
    if act && list.contains(m) {
        let wheel = mouse_wheel().1;
        if wheel > 0.0 {
            scroll = scroll.saturating_sub(1);
        } else if wheel < 0.0 && scroll + ROWS < stock.len() {
            scroll += 1;
        }
    }
    set_scroll(w, scroll);
    let sel = selected_other(w);
    let mut hit: Option<Hit> = None;
    let row_w = list.w - 10.0 * u;
    for (k, (slot, it)) in stock.iter().skip(scroll).take(ROWS).enumerate() {
        let rr = Rect::new(list.x, list.y + k as f32 * 28.0 * u, row_w, 26.0 * u);
        let hover = act && rr.contains(m);
        if *slot == sel {
            draw_rectangle(rr.x, rr.y, rr.w, rr.h, Color::new(0.35, 0.25, 0.08, 0.8));
        } else if hover {
            draw_rectangle(rr.x, rr.y, rr.w, rr.h, Color::new(0.2, 0.15, 0.08, 0.8));
        }
        let icon = Rect::new(rr.x + 2.0 * u, rr.y + 1.0 * u, 24.0 * u, 24.0 * u);
        draw_rectangle(icon.x, icon.y, icon.w, icon.h, SLOT);
        assets.draw_obj(it.obj, Rect::new(icon.x + 1.0, icon.y + 1.0, icon.w - 2.0, icon.h - 2.0));
        let price = format!("{} oro", fmt_gold(it.price as u64));
        let pw = measure(&price, 16.0 * u).width;
        let stock_s = format!("x{}", it.amount);
        let sw = measure(&stock_s, 14.0 * u).width;
        text_fit(&item_name(assets, it.obj), rr.x + 32.0 * u, rr.y + 18.0 * u, 16.0 * u, rr.w - 32.0 * u - pw - sw - 20.0 * u, TEXT);
        text(&stock_s, rr.right() - pw - 10.0 * u - sw, rr.y + 18.0 * u, 14.0 * u, MUTED);
        text(&price, rr.right() - pw - 4.0 * u, rr.y + 18.0 * u, 16.0 * u, if it.price > gold { BAD } else { GOLD_TEXT });
        if hover {
            hit = Some(Hit { from: WindowId::Shop, slot: *slot, obj: it.obj, amount: it.amount });
            if ui.drag.is_none() {
                tip = Some(item_tip(game, assets, it.obj, 1, false, &[format!("Precio: {} oro c/u", fmt_gold(it.price as u64)), "Doble clic: comprar 1".to_string()]));
            }
        }
    }
    if stock.is_empty() {
        text("El comerciante no tiene nada que vender.", list.x + 8.0 * u, list.y + 22.0 * u, 17.0 * u, MUTED);
    }
    if stock.len() > ROWS {
        let bar = Rect::new(list.right() - 6.0 * u, list.y, 6.0 * u, list.h);
        draw_rectangle(bar.x, bar.y, bar.w, bar.h, Color::new(0.0, 0.0, 0.0, 0.5));
        let (th, ty) = (bar.h * ROWS as f32 / stock.len() as f32, bar.h * scroll as f32 / stock.len() as f32);
        draw_rectangle(bar.x, bar.y + ty, bar.w, th, GOLD);
    }
    draw_rectangle_lines(list.x, list.y, list.w, list.h, 1.0, Color::new(0.35, 0.26, 0.14, 1.0));
    zones.push((Zone::StockArea, list));

    // ---- buy strip ----
    let by = list.bottom() + 4.0 * u;
    let sel_stock = shop.items.get(&sel).copied();
    let (b1, b2) = (btn_w("Comprar 1", u), btn_w("Comprar...", u));
    match sel_stock {
        Some(it) => text_fit(&format!("{}: {} oro c/u", item_name(assets, it.obj), fmt_gold(it.price as u64)), r.x + 4.0 * u, by + 13.0 * u, 16.0 * u, r.w - 8.0 * u, TEXT),
        None => text_fit("Elige algo de la lista", r.x + 4.0 * u, by + 13.0 * u, 16.0 * u, r.w - 8.0 * u, MUTED),
    }
    let can_buy = sel_stock.is_some() && game.shop_max_buy(sel) > 0;
    let bm = Rect::new(r.right() - b2, by + 18.0 * u, b2, 24.0 * u);
    if ibtn(bm, "Comprar...", can_buy, act, u) {
        if let Some(it) = sel_stock {
            inv::start(game, assets, Op::Buy, Hit { from: WindowId::Shop, slot: sel, obj: it.obj, amount: it.amount }, w);
        }
    }
    if ibtn(Rect::new(bm.x - 4.0 * u - b1, bm.y, b1, 24.0 * u), "Comprar 1", can_buy, act, u) {
        game.shop_buy(sel, 1);
    }

    // ---- inventory ----
    let y2 = by + 48.0 * u;
    text("Tu inventario", r.x + 4.0 * u, y2 + 13.0 * u, 18.0 * u, GOLD_TEXT);
    let hint = "Paga un tercio del valor";
    text(hint, r.right() - 4.0 * u - measure(hint, 14.0 * u).width, y2 + 13.0 * u, 14.0 * u, MUTED);
    let grid = Grid { cols: 11, rows: 4, cell: 26.0 * u, gap: 2.0 * u };
    let io = vec2(r.x + (r.w - grid.size().x) / 2.0, y2 + 18.0 * u);
    let sel_i = selected_inv();
    let (h2, z2) = draw_grid(ui, assets, &grid, io, &GridView { slots: &inventory, from: WindowId::Inventory, selected: sel_i, locked_from: usize::MAX }, act);
    zones.extend(z2);
    if let (Some(h), None) = (h2, &ui.drag) {
        let eq = game.local.slot(h.slot).is_some_and(|i| i.equipped);
        let unit = sale_unit(assets, h.obj);
        let pay = if unit > 0 { format!("Te pagan: {} oro c/u", fmt_gold(unit)) } else { "El comerciante no paga por esto".to_string() };
        tip = Some(item_tip(game, assets, h.obj, h.amount, eq, &[pay, "Doble clic: vender".to_string()]));
    }

    // ---- sell strip ----
    let sy = io.y + grid.size().y + 4.0 * u;
    let sel_item = inventory.get(sel_i.wrapping_sub(1) as usize).copied().flatten();
    let (s1, s2) = (btn_w("Vender 1", u), btn_w("Vender...", u));
    let can_sell = sel_item.is_some_and(|i| !i.equipped && sale_unit(assets, i.obj) > 0);
    match sel_item {
        Some(it) => {
            let unit = sale_unit(assets, it.obj);
            let line = if unit > 0 { format!("{}: te pagan {} oro c/u", item_name(assets, it.obj), fmt_gold(unit)) } else { format!("{}: no lo compran", item_name(assets, it.obj)) };
            text_fit(&line, r.x + 4.0 * u, sy + 13.0 * u, 16.0 * u, r.w - 8.0 * u, if can_sell { TEXT } else { MUTED });
        }
        None => text_fit("Elige algo de tu inventario", r.x + 4.0 * u, sy + 13.0 * u, 16.0 * u, r.w - 8.0 * u, MUTED),
    }
    let sm = Rect::new(r.right() - s2, sy + 18.0 * u, s2, 24.0 * u);
    if ibtn(sm, "Vender...", can_sell, act, u) {
        if let Some(it) = sel_item {
            inv::start(game, assets, Op::Sell, Hit { from: WindowId::Inventory, slot: sel_i, obj: it.obj, amount: it.amount }, w);
        }
    }
    if ibtn(Rect::new(sm.x - 4.0 * u - s1, sm.y, s1, 24.0 * u), "Vender 1", can_sell, act, u) {
        game.shop_sell(sel_i, 1);
    }

    // ---- input ----
    if let Some(ev) = track(ui, w, act, hit.or(h2)) {
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
    let _ = (draw_cell, GOLD);
}

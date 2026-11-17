//! Window: the NPC merchant (buy and sell). OWNER: Sonnet 2 (docs/P3_CONTRATO.md).
//!
//! On the left the merchant's shelf: one card per object with the price for this player
//! (commerce skill included) and whether the purse reaches. On the right your bag, each
//! object marked with what the merchant pays. Under both, the counter: the object in
//! focus, how many, the total and the one button that does the deal. Drag from the shelf to
//! the bag to buy and back to sell, or double click for one unit. Every deal answers with a
//! toast.
use super::inventory::{self as inv, active, aux, btn_w, cat_color, chip, draw_cell, draw_grid, fmt_gold, glyph, handle, ibtn_kind, item_name, item_tip, mouse, scroll_of, selected_inv, selected_other, set_scroll, settle_tx, track, tray, watch_buy, watch_sell, CellOpts, Ev, Glyph, Grid, GridView, Hit, Zone};
use crate::assets::Assets;
use crate::game::Game;
use crate::ui::fx_ui::{ease, key_of};
use crate::ui::theme::{self, FontKind, BAD, GOLD, GOLD_BRIGHT, GOLD_DIM, GOLD_TEXT, GOOD, TEXT, TEXT_DIM, TEXT_FAINT};
use crate::ui::widgets::{self, measure, text, text_fit, text_with, Kind};
use crate::ui::{Ui, WindowId};
use ao_formats::dat::obj::{ObjData, ObjType};
use client_core::p3::items::{category, parse_qty, sale_value, type_name, usable_slots};
use client_core::ItemSlot;
use macroquad::prelude::*;

const ROW_H: f32 = 54.0;

/// What the merchant pays per unit of `obj` (0 = does not buy it).
fn sale_unit(assets: &Assets, obj: u16) -> u64 {
    assets.objs.get(obj as u32).map_or(0, |o| sale_value(o, 1))
}

/// One line of numbers about an object, for the cards: damage, defence, effect...
fn stat_line(o: &ObjData) -> String {
    use ObjType::*;
    let kind = type_name(o.obj_type);
    match o.obj_type {
        Weapon | Arrows => format!("{kind} · daño {}-{}", o.min_hit, o.max_hit),
        Armor | Shield | Helmet => format!("{kind} · defensa {}-{}", o.min_def, o.max_def),
        Potions if o.max_modifier > 0 => format!("{kind} · efecto {}-{}", o.min_modifier, o.max_modifier),
        UseOnce | Drinks if o.min_hunger > 0 => format!("{kind} · hambre +{}", o.min_hunger),
        UseOnce | Drinks if o.min_thirst > 0 => format!("{kind} · sed +{}", o.min_thirst),
        _ => kind.to_string(),
    }
}

/// The counter's price for `n` units of the focused object.
struct Deal {
    buying: bool,
    slot: u8,
    obj: u16,
    unit: u64,
    /// Most units the deal allows (stock and purse when buying, the stack when selling).
    max: u32,
    ok: bool,
    /// Why it cannot be done (empty when it can).
    why: &'static str,
}

/// Draws the content of the window inside `r` (the frame is already drawn).
pub fn draw(ui: &mut Ui, game: &mut Game, assets: &mut Assets, r: Rect) {
    let u = ui.u;
    let w = WindowId::Shop;
    inv::begin(ui, game, assets);
    settle_tx(game, assets);
    let Some(shop) = game.local.social.trade.shop.clone() else { return };
    let inventory = game.local.inventory.clone();
    let act = active(ui, w);
    let m = mouse();
    let mut zones = Vec::new();
    let mut tip: Option<Vec<String>> = None;
    let gold = game.local.gold;
    let bp = inventory.iter().flatten().any(|i| i.equipped && assets.objs.get(i.obj as u32).is_some_and(|o| o.obj_type == ObjType::Backpack));
    let usable = usable_slots(bp);

    // ---- headings ----
    // "Grom <Herrero Brutal>": the window title already says what he is.
    let who = game.ents.get(&shop.npc).map(|e| e.name.split('<').next().unwrap_or("").trim().to_string()).filter(|n| !n.is_empty()).unwrap_or_else(|| "Comerciante".into());
    let listw = 372.0 * u;
    let (lx, rx) = (r.x, r.x + listw + 26.0 * u);
    let pane_w = r.right() - rx;
    let top = theme::section(&format!("Escaparate de {who}"), lx, r.y - 4.0 * u, listw, u);
    theme::section("Tu bolsa", rx, r.y - 4.0 * u, pane_w, u);
    let gs = fmt_gold(gold as u64);
    let gd = widgets::measure_with(FontKind::Title, &gs, 20.0 * u);
    text_with(FontKind::Title, &gs, r.right() - gd.width, r.y + 16.0 * u, 20.0 * u, GOLD_BRIGHT);
    glyph(Glyph::Coin, vec2(r.right() - gd.width - 14.0 * u, r.y + 9.0 * u), 8.0 * u, GOLD);
    let bar_h = 88.0 * u;
    let body_h = r.bottom() - bar_h - top - 8.0 * u;

    // ---- the shelf ----
    let list = Rect::new(lx + 4.0 * u, top + 4.0 * u, listw - 8.0 * u, body_h - 4.0 * u);
    let hot = ui.drag.as_ref().is_some_and(|d| d.from == WindowId::Inventory) && act && list.contains(m);
    tray(list, ease(key_of(list), hot, 0.1), u);
    let stock: Vec<(u8, client_core::social::trade::ShopItem)> = shop.items.iter().map(|(s, i)| (*s, *i)).collect();
    let rows = ((list.h - 8.0 * u) / (ROW_H * u)).floor().max(1.0) as usize;
    let mut scroll = scroll_of(w).min(stock.len().saturating_sub(rows));
    if act && list.contains(m) {
        let wheel = mouse_wheel().1;
        if wheel > 0.0 {
            scroll = scroll.saturating_sub(1);
        } else if wheel < 0.0 && scroll + rows < stock.len() {
            scroll += 1;
        }
    }
    set_scroll(w, scroll);
    let sel = selected_other(w);
    let mut hit: Option<Hit> = None;
    let sb = if stock.len() > rows { 10.0 * u } else { 0.0 };
    for (k, (slot, it)) in stock.iter().skip(scroll).take(rows).enumerate() {
        let rr = Rect::new(list.x + 4.0 * u, list.y + 4.0 * u + k as f32 * ROW_H * u, list.w - 8.0 * u - sb, (ROW_H - 4.0) * u);
        let hover = act && rr.contains(m);
        let h = ease(key_of(rr), hover, 0.08);
        let afford = it.price <= gold;
        let obj = assets.objs.get(it.obj as u32).cloned();
        let accent = obj.as_ref().map_or(GOLD, |o| cat_color(category(o.obj_type)));
        let sel_row = *slot == sel;
        draw_rectangle(rr.x, rr.y, rr.w, rr.h, Color::new(0.085 + 0.10 * h, 0.06 + 0.07 * h, 0.04 + 0.03 * h, 1.0));
        draw_rectangle(rr.x, rr.y, rr.w, rr.h * 0.45, Color::new(1.0, 0.9, 0.7, 0.03));
        draw_rectangle(rr.x, rr.y, 3.0 * u, rr.h, Color::new(accent.r, accent.g, accent.b, if sel_row { 1.0 } else { 0.55 }));
        draw_rectangle_lines(rr.x, rr.y, rr.w, rr.h, if sel_row { 2.0 * u } else { 1.0 }, if sel_row { GOLD_BRIGHT } else { Color::new(GOLD_DIM.r, GOLD_DIM.g, GOLD_DIM.b, 0.45 + 0.4 * h) });
        let ic = Rect::new(rr.x + 8.0 * u, rr.y + 4.0 * u, rr.h - 8.0 * u, rr.h - 8.0 * u);
        draw_cell(assets, ic, Some(ItemSlot { obj: it.obj, amount: 1, equipped: false }), &CellOpts { hover, no_amount: true, ..Default::default() }, u);
        // Name, numbers, what is left.
        let price_w = 118.0 * u;
        let name = item_name(assets, it.obj);
        text_fit(&name, ic.right() + 10.0 * u, rr.y + 21.0 * u, 18.0 * u, rr.w - ic.w - price_w - 30.0 * u, TEXT);
        let sub = obj.as_ref().map_or(String::new(), stat_line);
        text_fit(&sub, ic.right() + 10.0 * u, rr.y + 39.0 * u, 14.0 * u, rr.w - ic.w - price_w - 30.0 * u, TEXT_DIM);
        // Price: big, in gold; red with what is missing when the purse does not reach.
        let ps = fmt_gold(it.price as u64);
        let pd = widgets::measure_with(FontKind::Title, &ps, 21.0 * u);
        let py = if afford { rr.y + 31.0 * u } else { rr.y + 25.0 * u };
        text_with(FontKind::Title, &ps, rr.right() - 10.0 * u - pd.width, py, 21.0 * u, if afford { GOLD_TEXT } else { BAD });
        glyph(Glyph::Coin, vec2(rr.right() - 10.0 * u - pd.width - 12.0 * u, py - 7.0 * u), 7.5 * u, if afford { GOLD } else { Color::new(0.55, 0.25, 0.2, 1.0) });
        let (sub2, col2) = if afford { (format!("quedan {}", it.amount), TEXT_FAINT) } else { (format!("te faltan {}", fmt_gold((it.price - gold) as u64)), BAD) };
        text(&sub2, rr.right() - 10.0 * u - measure(&sub2, 13.0 * u).width, rr.y + 43.0 * u, 13.0 * u, col2);
        if hover {
            hit = Some(Hit { from: WindowId::Shop, slot: *slot, obj: it.obj, amount: it.amount });
            if ui.drag.is_none() {
                tip = Some(item_tip(game, assets, it.obj, 1, false, &[format!("${} oro cada uno", fmt_gold(it.price as u64)), "Doble clic: comprar 1 · arrastra a tu bolsa para elegir cuántos".to_string()]));
            }
        }
    }
    if stock.is_empty() {
        text("El comerciante no tiene nada que vender.", list.x + 14.0 * u, list.y + 30.0 * u, 17.0 * u, TEXT_FAINT);
    }
    if stock.len() > rows {
        let bar = Rect::new(list.right() - 12.0 * u, list.y + 6.0 * u, 6.0 * u, list.h - 12.0 * u);
        draw_rectangle(bar.x, bar.y, bar.w, bar.h, Color::new(0.0, 0.0, 0.0, 0.5));
        let (th, ty) = (bar.h * rows as f32 / stock.len() as f32, bar.h * scroll as f32 / stock.len() as f32);
        draw_rectangle(bar.x, bar.y + ty, bar.w, th, GOLD);
        draw_rectangle(bar.x, bar.y + ty, bar.w, th * 0.4, Color::new(1.0, 1.0, 1.0, 0.2));
    }
    zones.push((Zone::StockArea, list));

    // ---- your bag, marked with what the merchant pays ----
    let grid = Grid::fit(7, 6, pane_w - 12.0 * u, body_h - 12.0 * u, 3.0 * u, 48.0 * u);
    let io = vec2(rx + 6.0 * u + (pane_w - 12.0 * u - grid.size().x) / 2.0, top + 10.0 * u);
    let sel_i = selected_inv();
    let view = GridView { slots: &inventory, from: WindowId::Inventory, selected: sel_i, locked_from: usable, filter: None, query: "", sale_badges: true };
    let (h2, z2) = draw_grid(ui, assets, &grid, io, &view, act);
    zones.extend(z2);
    if let (Some(h), None) = (h2, &ui.drag) {
        let eq = game.local.slot(h.slot).is_some_and(|i| i.equipped);
        let unit = sale_unit(assets, h.obj);
        let pay = if unit > 0 { format!("${} oro cada uno", fmt_gold(unit)) } else { "!El comerciante no paga por esto".to_string() };
        tip = Some(item_tip(game, assets, h.obj, h.amount, eq, &[pay, "Doble clic: vender".to_string()]));
    }

    // ---- input ----
    if let Some(ev) = track(ui, w, act, hit.or(h2)) {
        if let Ev::Click(h) = &ev {
            aux(|a| a.shop_buying = h.from == WindowId::Shop);
        }
        handle(game, assets, w, ev);
    }

    // ---- the counter ----
    let bar = Rect::new(r.x, r.bottom() - bar_h + 6.0 * u, r.w, bar_h - 6.0 * u);
    theme::divider(r.x + r.w / 2.0, bar.y - 2.0 * u, r.w, u);
    let buying = aux(|a| a.shop_buying);
    let sel_stock = shop.items.get(&sel).copied();
    let sel_item = inventory.get(sel_i.wrapping_sub(1) as usize).copied().flatten();
    let deal: Option<Deal> = if buying {
        sel_stock.map(|it| Deal { buying: true, slot: sel, obj: it.obj, unit: it.price as u64, max: game.shop_max_buy(sel).max(1).min(it.amount.max(1)), ok: game.shop_max_buy(sel) > 0, why: if game.shop_max_buy(sel) > 0 { "" } else { "no te alcanza el oro" } })
    } else {
        sel_item.map(|it| {
            let unit = sale_unit(assets, it.obj);
            Deal { buying: false, slot: sel_i, obj: it.obj, unit, max: it.amount.max(1), ok: unit > 0 && !it.equipped, why: if it.equipped { "quítatelo antes de venderlo" } else if unit == 0 { "no lo compran" } else { "" } }
        })
    };
    match deal {
        None => {
            let msg = "Elige algo del escaparate para comprarlo, o de tu bolsa para venderlo";
            text_with(FontKind::NarrativeItalic, msg, bar.x + (bar.w - widgets::measure_with(FontKind::NarrativeItalic, msg, 18.0 * u).width) / 2.0, bar.y + bar.h / 2.0 + 6.0 * u, 18.0 * u, TEXT_DIM);
        }
        Some(d) => counter(ui, game, assets, bar, &d, act),
    }
    let cl = btn_w("Cerrar", u) + 10.0 * u;
    if ibtn_kind(Rect::new(r.right() - cl, r.bottom() - 26.0 * u, cl, 24.0 * u), "Cerrar", Kind::Ghost, true, act, u) {
        game.shop_close();
    }

    inv::register(w, r, u, zones);
    inv::menu_ui(ui, game, assets, w);
    inv::prompt_ui(ui, game, assets, w, r);
    if let Some(t) = tip {
        if ui.drag.is_none() && act {
            inv::tooltip_at(&t, m, u);
        }
    }
    let _ = (GOOD, ibtn_kind, chip, btn_w);
}

/// The counter itself: object, how many, total, button.
fn counter(ui: &mut Ui, game: &mut Game, assets: &mut Assets, bar: Rect, d: &Deal, act: bool) {
    let u = ui.u;
    let o = assets.objs.get(d.obj as u32).cloned();
    // Reset the quantity when the focus moves to another object.
    let key = (d.buying, d.slot, d.obj);
    if aux(|a| a.qty_key != Some(key)) {
        aux(|a| {
            a.qty_key = Some(key);
            a.qty_text.clear();
        });
    }
    let mut qt = aux(|a| std::mem::take(&mut a.qty_text));
    let n = parse_qty(&qt, d.max).unwrap_or(1).min(d.max).max(1);
    let total: u64 = match (&o, d.buying) {
        (Some(o), true) => {
            let skill = game.local.skills.get(ao_rules::trade::SKILL_COMMERCE).copied().unwrap_or(0);
            ao_rules::trade::buy_price(o.value, skill, n)
        }
        (Some(o), false) => sale_value(o, n),
        _ => d.unit * n as u64,
    };
    // Object card.
    let cy = bar.y + 8.0 * u;
    let ic = Rect::new(bar.x, cy, 56.0 * u, 56.0 * u);
    draw_cell(assets, ic, Some(ItemSlot { obj: d.obj, amount: 1, equipped: false }), &CellOpts { no_amount: true, ..Default::default() }, u);
    let verb = if d.buying { "Comprar" } else { "Vender" };
    text(if d.buying { "Compras" } else { "Vendes" }, ic.right() + 12.0 * u, cy + 14.0 * u, 14.0 * u, if d.buying { GOOD } else { GOLD_TEXT });
    text_fit(&item_name(assets, d.obj), ic.right() + 12.0 * u, cy + 34.0 * u, 19.0 * u, 190.0 * u, TEXT);
    let unit_s = if d.unit > 0 { format!("{} oro cada uno", fmt_gold(d.unit)) } else { "no lo compran".to_string() };
    text(&unit_s, ic.right() + 12.0 * u, cy + 52.0 * u, 14.0 * u, if d.unit > 0 { TEXT_DIM } else { BAD });
    // Quantity: - [field] + Max.
    let qx = bar.x + 300.0 * u;
    let b = 30.0 * u;
    let minus = Rect::new(qx, cy + 12.0 * u, b, b);
    let field = Rect::new(minus.right() + 4.0 * u, cy + 12.0 * u, 74.0 * u, b);
    let plus = Rect::new(field.right() + 4.0 * u, cy + 12.0 * u, b, b);
    let maxb = Rect::new(plus.right() + 8.0 * u, cy + 12.0 * u, 52.0 * u, b);
    if chip(minus, "-", false, act, u) {
        qt = n.saturating_sub(1).max(1).to_string();
    }
    if chip(plus, "+", false, act, u) {
        qt = (n + 1).min(d.max).to_string();
    }
    if chip(maxb, "Máx", false, act, u) {
        qt = d.max.to_string();
    }
    let enter = widgets::text_field(ui, "shop_qty", &mut qt, field, 6, false, |c| c.is_ascii_digit());
    if qt.is_empty() && ui.focus.as_deref() != Some("shop_qty") {
        let s = n.to_string();
        let dm = measure(&s, 18.0 * u);
        text(&s, field.x + (field.w - dm.width) / 2.0, field.y + field.h / 2.0 + 6.0 * u, 18.0 * u, TEXT_DIM);
    }
    text(&format!("de {}", fmt_gold(d.max as u64)), qx, cy + 62.0 * u, 13.0 * u, TEXT_FAINT);
    aux(|a| a.qty_text = qt);
    // Total.
    let tx = bar.x + 520.0 * u;
    text("Total", tx, cy + 18.0 * u, 14.0 * u, TEXT_DIM);
    let ts = fmt_gold(total);
    let afford = !d.buying || total <= game.local.gold as u64;
    text_with(FontKind::Title, &ts, tx + 24.0 * u, cy + 46.0 * u, 26.0 * u, if afford { GOLD_BRIGHT } else { BAD });
    glyph(Glyph::Coin, vec2(tx + 10.0 * u, cy + 38.0 * u), 9.0 * u, if afford { GOLD_BRIGHT } else { BAD });
    if !afford {
        text(&format!("te faltan {}", fmt_gold(total - game.local.gold as u64)), tx, cy + 64.0 * u, 14.0 * u, BAD);
    } else if !d.why.is_empty() {
        text(d.why, tx, cy + 64.0 * u, 14.0 * u, BAD);
    }
    // The deal.
    let bw = 130.0 * u;
    let btn = Rect::new(bar.right() - bw, cy + 6.0 * u, bw, 40.0 * u);
    let can = d.ok && afford;
    if ibtn_kind(btn, verb, Kind::Primary, can, act, u) || (enter && can) {
        if d.buying {
            watch_buy(game, d.obj);
            game.shop_buy(d.slot, n);
        } else {
            watch_sell(game, d.obj);
            game.shop_sell(d.slot, n);
        }
    }
    let _ = GOLD_BRIGHT;
}

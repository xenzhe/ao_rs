//! Window: auction house. OWNER: Opus D (docs/P4_CONTRATO.md).
//!
//! Left: the open lots (icon, seller, best bid, time left); the selected one can be bid on
//! with the minimum filled in. Right: publishing one of your objects (pick it in the small
//! inventory grid, amount, base price and duration). `AO_EVENTDEMO=1` shows sample lots.
use super::events::{demo_active, gold_str};
use crate::assets::Assets;
use crate::game::Game;
use crate::ui::theme::{FontKind, GOLD, GOLD_TEXT, MUTED, SLOT, TEXT};
use crate::ui::widgets::{button, measure, slot, text, text_field, text_fit, text_with, tooltip};
use crate::ui::Ui;
use ao_protocol::p4::event::AuctionLot;
use macroquad::prelude::*;
use std::cell::RefCell;

/// AO20 `CostoPreSubasta` and the 5 % commission (shown, enforced by the server).
const FEE: u32 = 500;

#[derive(Default)]
struct Form {
    lot: Option<u32>,
    bid: String,
    slot: Option<u8>,
    amount: String,
    price: String,
    minutes: u8,
}

thread_local! {
    static FORM: RefCell<Form> = RefCell::new(Form { minutes: 5, amount: "1".into(), ..Default::default() });
}

fn demo_lots() -> Vec<AuctionLot> {
    let lot = |id, seller: &str, obj, amount, base, best, bidder: &str, secs| AuctionLot { id, seller: seller.into(), obj, amount, base_price: base, best, bidder: bidder.into(), min_bid: if best == 0 { base } else { best + (best * 5 / 100).max(100) }, secs_left: secs };
    vec![
        lot(1, "Aldric", 126, 1, 40_000, 52_000, "Morgana", 212),
        lot(2, "Kira", 38, 150, 3_000, 0, "", 274),
        lot(3, "Tordek", 1700, 1, 15_000, 18_500, "Valen", 41),
        lot(4, "Brom", 1987, 1, 120_000, 0, "", 1_650),
        lot(5, "Valen", 1867, 1, 25_000, 0, "", 905),
    ]
}

fn obj_name(assets: &Assets, obj: u16) -> String {
    assets.objs.get(obj as u32).map(|o| o.name.clone()).filter(|n| !n.is_empty()).unwrap_or_else(|| format!("Objeto {obj}"))
}

fn time_str(secs: u32) -> String {
    if secs >= 60 { format!("{}:{:02}", secs / 60, secs % 60) } else { format!("{secs} s") }
}

pub fn draw(ui: &mut Ui, game: &mut Game, assets: &mut Assets, r: Rect) {
    let u = ui.u;
    if demo_active() {
        game.local.p4.event.lots = demo_lots();
        FORM.with(|f| {
            let mut f = f.borrow_mut();
            if f.lot.is_none() {
                f.lot = Some(1);
                f.bid = "54600".into();
            }
        });
    }
    let lots = game.local.p4.event.lots.clone();
    let split = r.x + r.w * 0.58;
    lots_column(ui, game, assets, &lots, Rect::new(r.x, r.y, split - r.x - 10.0 * u, r.h));
    draw_line(split, r.y + 4.0 * u, split, r.bottom() - 4.0 * u, 1.0, Color::new(GOLD.r, GOLD.g, GOLD.b, 0.4));
    publish_column(ui, game, assets, Rect::new(split + 10.0 * u, r.y, r.right() - split - 10.0 * u, r.h));
}

fn lots_column(ui: &mut Ui, game: &mut Game, assets: &mut Assets, lots: &[AuctionLot], r: Rect) {
    let u = ui.u;
    text_with(FontKind::Title, "En subasta", r.x, r.y + 18.0 * u, 20.0 * u, GOLD_TEXT);
    let mut y = r.y + 28.0 * u;
    if lots.is_empty() {
        text_fit("No hay nada en subasta. ¡Sé el primero en publicar!", r.x, y + 20.0 * u, 16.0 * u, r.w, MUTED);
        return;
    }
    let me = game.me().map(|e| e.name.clone()).unwrap_or_default();
    let selected = FORM.with(|f| f.borrow().lot).filter(|id| lots.iter().any(|l| l.id == *id));
    let list_bottom = r.bottom() - 74.0 * u;
    let mut hover = None;
    for l in lots {
        if y + 50.0 * u > list_bottom {
            text(&format!("y {} más...", lots.iter().filter(|x| x.id > l.id).count() + 1), r.x, y + 14.0 * u, 15.0 * u, MUTED);
            break;
        }
        let row = Rect::new(r.x, y, r.w, 48.0 * u);
        let sel = selected == Some(l.id);
        draw_rectangle(row.x, row.y, row.w, row.h, Color::new(0.0, 0.0, 0.0, if sel { 0.55 } else if row.contains(mouse_position().into()) { 0.4 } else { 0.25 }));
        if sel {
            draw_rectangle_lines(row.x, row.y, row.w, row.h, 1.5 * u, GOLD_TEXT);
        }
        let icon = Rect::new(row.x + 4.0 * u, row.y + 4.0 * u, 40.0 * u, 40.0 * u);
        draw_rectangle(icon.x, icon.y, icon.w, icon.h, SLOT);
        assets.draw_obj(l.obj, icon);
        let tx = icon.right() + 8.0 * u;
        let name = if l.amount > 1 { format!("{} (x{})", obj_name(assets, l.obj), l.amount) } else { obj_name(assets, l.obj) };
        let time_w = 54.0 * u;
        text_fit(&name, tx, row.y + 19.0 * u, 17.0 * u, row.right() - tx - time_w, TEXT);
        let price = if l.best > 0 { format!("{} de oro · {}", gold_str(l.best), l.bidder) } else { format!("Base {} de oro · sin ofertas", gold_str(l.base_price)) };
        text_fit(&price, tx, row.y + 39.0 * u, 15.0 * u, row.right() - tx - time_w, GOLD_TEXT);
        let t = time_str(l.secs_left);
        let td = measure(&t, 16.0 * u);
        text(&t, row.right() - td.width - 8.0 * u, row.y + 19.0 * u, 16.0 * u, if l.secs_left < 60 { Color::new(1.0, 0.55, 0.4, 1.0) } else { MUTED });
        if row.contains(mouse_position().into()) {
            hover = Some(l.clone());
            if is_mouse_button_pressed(MouseButton::Left) {
                FORM.with(|f| {
                    let mut f = f.borrow_mut();
                    f.lot = Some(l.id);
                    f.bid = l.min_bid.to_string();
                });
            }
        }
        y += 52.0 * u;
    }
    if let Some(l) = hover {
        tooltip(&[obj_name(assets, l.obj), format!("Vende: {}", l.seller), format!("Oferta mínima: {} de oro", gold_str(l.min_bid)), format!("Termina en {}", time_str(l.secs_left))], u);
    }
    // Bid bar.
    let bar = Rect::new(r.x, list_bottom + 8.0 * u, r.w, 64.0 * u);
    draw_rectangle(bar.x, bar.y, bar.w, bar.h, Color::new(0.0, 0.0, 0.0, 0.3));
    let Some(l) = selected.and_then(|id| lots.iter().find(|l| l.id == id)) else {
        text_fit("Elige un objeto de la lista para pujar.", bar.x + 8.0 * u, bar.y + 36.0 * u, 16.0 * u, bar.w - 16.0 * u, MUTED);
        return;
    };
    let own = l.seller.eq_ignore_ascii_case(&me);
    text_fit(&format!("Pujar por {}: mínimo {} de oro", obj_name(assets, l.obj), gold_str(l.min_bid)), bar.x + 8.0 * u, bar.y + 18.0 * u, 15.0 * u, bar.w - 16.0 * u, MUTED);
    let mut bid = None;
    FORM.with(|f| {
        let mut f = f.borrow_mut();
        text_field(ui, "auc_bid", &mut f.bid, Rect::new(bar.x + 8.0 * u, bar.y + 28.0 * u, bar.w - 130.0 * u, 26.0 * u), 9, false, |c| c.is_ascii_digit());
        let b = Rect::new(bar.right() - 112.0 * u, bar.y + 28.0 * u, 104.0 * u, 26.0 * u);
        if own {
            text("Es tu subasta", b.x, b.y + 18.0 * u, 15.0 * u, MUTED);
        } else if button(b, "Pujar", false, u) {
            bid = Some(f.bid.trim().parse::<u32>().unwrap_or(0));
        }
    });
    if let Some(amount) = bid {
        game.auction_bid(l.id, amount);
    }
}

fn publish_column(ui: &mut Ui, game: &mut Game, assets: &mut Assets, r: Rect) {
    let u = ui.u;
    text_with(FontKind::Title, "Publicar", r.x, r.y + 18.0 * u, 20.0 * u, GOLD_TEXT);
    text_fit("Elige un objeto de tu inventario:", r.x, r.y + 42.0 * u, 15.0 * u, r.w, MUTED);
    let cols = 6usize;
    let cell = (r.w / cols as f32).floor();
    let gy = r.y + 50.0 * u;
    let inv = game.local.inventory.clone();
    let mut sel = FORM.with(|f| f.borrow().slot);
    let mut hover = None;
    for i in 0..24usize {
        let rect = Rect::new(r.x + (i % cols) as f32 * cell, gy + (i / cols) as f32 * cell, cell - 3.0 * u, cell - 3.0 * u);
        let item = inv.get(i).copied().flatten();
        let (h, clicked) = slot(rect, sel == Some(i as u8 + 1), u);
        if let Some(it) = item {
            assets.draw_obj(it.obj, rect);
            if it.amount > 1 {
                text(&it.amount.to_string(), rect.x + 3.0 * u, rect.bottom() - 4.0 * u, 13.0 * u, TEXT);
            }
            if h {
                hover = Some(it.obj);
            }
            if clicked {
                sel = Some(i as u8 + 1);
                FORM.with(|f| {
                    let mut f = f.borrow_mut();
                    f.slot = sel;
                    f.amount = it.amount.to_string();
                });
            }
        }
    }
    if let Some(o) = hover {
        tooltip(&[obj_name(assets, o)], u);
    }
    let mut y = gy + 4.0 * cell + 18.0 * u;
    let chosen = sel.and_then(|s| inv.get(s as usize - 1).copied().flatten());
    text_fit(&chosen.map_or("Ningún objeto elegido".to_string(), |c| obj_name(assets, c.obj)), r.x, y, 17.0 * u, r.w, if chosen.is_some() { TEXT } else { MUTED });
    y += 10.0 * u;
    let mut publish = None;
    FORM.with(|f| {
        let mut f = f.borrow_mut();
        let half = (r.w - 8.0 * u) / 2.0;
        text("Cantidad", r.x, y + 12.0 * u, 14.0 * u, MUTED);
        text("Precio base", r.x + half + 8.0 * u, y + 12.0 * u, 14.0 * u, MUTED);
        y += 16.0 * u;
        text_field(ui, "auc_amount", &mut f.amount, Rect::new(r.x, y, half, 24.0 * u), 5, false, |c| c.is_ascii_digit());
        text_field(ui, "auc_price", &mut f.price, Rect::new(r.x + half + 8.0 * u, y, half, 24.0 * u), 7, false, |c| c.is_ascii_digit());
        y += 32.0 * u;
        text("Duración", r.x, y + 16.0 * u, 14.0 * u, MUTED);
        let dw = (r.w - 70.0 * u) / 3.0;
        for (i, m) in [5u8, 10, 30].into_iter().enumerate() {
            if button(Rect::new(r.x + 66.0 * u + i as f32 * dw, y, dw - 4.0 * u, 24.0 * u), &format!("{m} min"), f.minutes == m, u) {
                f.minutes = m;
            }
        }
        y += 34.0 * u;
        text_fit(&format!("Publicar cuesta {FEE} de oro."), r.x, y + 10.0 * u, 14.0 * u, r.w, MUTED);
        text_fit("El reino cobra el 5 % de la venta.", r.x, y + 26.0 * u, 14.0 * u, r.w, MUTED);
        y += 36.0 * u;
        let b = Rect::new(r.x, y, r.w, 28.0 * u);
        if button(b, "Publicar", false, u) {
            match (f.slot, f.amount.trim().parse::<u32>(), f.price.trim().parse::<u32>()) {
                (Some(s), Ok(a), Ok(p)) => publish = Some((s, a, p, f.minutes)),
                _ => publish = Some((0, 0, 0, 0)),
            }
        }
    });
    match publish {
        Some((0, ..)) => game.say("Elige un objeto, la cantidad y el precio base.", ao_protocol::Tone::Warn),
        Some((s, a, p, m)) => game.auction_publish(s, a, p, m),
        None => {}
    }
    text_fit(&format!("Tu oro: {}", gold_str(game.local.gold)), r.x, r.bottom() - 4.0 * u, 16.0 * u, r.w, GOLD_TEXT);
}

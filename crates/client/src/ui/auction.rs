//! Window: auction house, as a bidding table. Two pages: "Pujar" (the open lots on the left;
//! the chosen one on a table with its price, who leads, a big countdown and the bid) and
//! "Publicar" (pick one of your objects in the bag grid, amount, base price and duration).
//! `AO_EVENTDEMO=1` shows sample lots; `AO_AUCTION_TAB=publicar` opens the second page.
use super::events::{demo_active, gold_str};
use crate::assets::Assets;
use crate::game::Game;
use crate::ui::character::kit::{self, Icon, INK_DIM, WAX};
use crate::ui::theme::{FontKind, BAD, GOLD, GOLD_BRIGHT, GOLD_DIM, GOLD_TEXT, GOOD, TEXT, TEXT_DIM, TEXT_FAINT, WARN};
use crate::ui::widgets::{button_kind, slot, text_field, text_with, tooltip, Kind};
use crate::ui::{Ui, WindowId};
use ao_protocol::p4::event::AuctionLot;
use macroquad::prelude::*;
use std::cell::{Cell, RefCell};

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
    publish_tab: bool,
    scroll: usize,
}

thread_local! {
    static FORM: RefCell<Form> = RefCell::new(Form { minutes: 5, amount: "1".into(), publish_tab: std::env::var("AO_AUCTION_TAB").is_ok_and(|v| v == "publicar"), ..Default::default() });
    static LAST_DRAW: Cell<f64> = const { Cell::new(-10.0) };
    static LOTS_AT: Cell<f64> = const { Cell::new(0.0) };
}

fn demo_lots() -> Vec<AuctionLot> {
    let lot = |id, seller: &str, obj, amount, base, best, bidder: &str, secs| AuctionLot { id, seller: seller.into(), obj, amount, base_price: base, best, bidder: bidder.into(), min_bid: if best == 0 { base } else { best + (best * 5 / 100).max(100) }, secs_left: secs };
    vec![
        // Objects with `Subastable=1` in obj.dat.
        lot(1, "Aldric", 476, 1, 400_000, 520_000, "Morgana", 212),
        lot(2, "Kira", 388, 40, 30_000, 0, "", 274),
        lot(3, "Tordek", 193, 250, 15_000, 18_500, "Valen", 41),
        lot(4, "Brom", 58, 500, 2_000, 0, "", 1_650),
        lot(5, "Valen", 386, 100, 25_000, 0, "", 905),
    ]
}

/// `AO_AUCTION_PUBLISH=slot,amount,price,minutes`: publishes for real once the slot holds
/// something (tests and captures against a real server; pair it with `AO_SAY=/dar ...`).
fn real_publish_once(game: &mut Game) {
    thread_local!(static DONE: Cell<bool> = const { Cell::new(false) });
    let Ok(spec) = std::env::var("AO_AUCTION_PUBLISH") else { return };
    if DONE.get() {
        return;
    }
    let n: Vec<u32> = spec.split(',').filter_map(|s| s.trim().parse().ok()).collect();
    let [slot, amount, price, minutes] = n[..] else { return };
    if game.local.inventory.get(slot as usize - 1).copied().flatten().is_some() && game.local.gold >= FEE {
        DONE.set(true);
        game.auction_publish(slot as u8, amount, price, minutes as u8);
    }
}

fn obj_name(assets: &Assets, obj: u16) -> String {
    assets.objs.get(obj as u32).map(|o| o.name.clone()).filter(|n| !n.is_empty()).unwrap_or_else(|| format!("Objeto {obj}"))
}

fn time_str(secs: u32) -> String {
    if secs >= 3600 {
        format!("{}:{:02}:{:02}", secs / 3600, (secs / 60) % 60, secs % 60)
    } else {
        format!("{}:{:02}", secs / 60, secs % 60)
    }
}

/// Colour of a countdown: calm gold, amber under five minutes, red under one.
fn time_color(secs: u32) -> Color {
    if secs < 60 {
        BAD
    } else if secs < 300 {
        WARN
    } else {
        GOLD_BRIGHT
    }
}

pub fn draw(ui: &mut Ui, game: &mut Game, assets: &mut Assets, r: Rect) {
    let u = ui.u;
    kit::begin(ui, WindowId::Auction);
    if demo_active() {
        game.local.p4.event.lots = demo_lots();
        FORM.with(|f| {
            let mut f = f.borrow_mut();
            if f.lot.is_none() {
                f.lot = Some(1);
                f.bid = "546000".into();
            }
        });
    }
    real_publish_once(game);
    let fresh = std::mem::take(&mut game.local.p4.event.lots_new);
    let elapsed = super::events::sync_window(game, &LAST_DRAW, &LOTS_AT, fresh);
    let mut lots = game.local.p4.event.lots.clone();
    for l in lots.iter_mut() {
        l.secs_left = l.secs_left.saturating_sub(elapsed);
    }
    let r = Rect::new(r.x + 6.0 * u, r.y + 2.0 * u, r.w - 12.0 * u, r.h - 4.0 * u);
    let tab = FORM.with(|f| f.borrow().publish_tab) as usize;
    let labels = [format!("Pujar ({})", lots.len()), "Publicar".to_string()];
    if let Some(i) = kit::tabs(r.x + 6.0 * u, r.y, &labels, tab, u) {
        FORM.with(|f| f.borrow_mut().publish_tab = i == 1);
    }
    // Your gold, always in view.
    let gold = gold_str(game.local.gold);
    let gw = crate::ui::widgets::measure_with(FontKind::Title, &gold, 17.0 * u).width;
    kit::icon(Icon::Coin, r.right() - 4.0 * u - gw - 16.0 * u, r.y + 13.0 * u, 18.0 * u, Color::new(0.95, 0.76, 0.26, 1.0));
    kit::text_right(FontKind::Title, &gold, r.right() - 4.0 * u, r.y + 20.0 * u, 17.0 * u, GOLD_TEXT);
    kit::rule(r.x, r.y + 30.0 * u, r.w, GOLD);
    let body = Rect::new(r.x, r.y + 42.0 * u, r.w, r.h - 42.0 * u);
    if tab == 0 {
        bid_page(ui, game, assets, &lots, body);
    } else {
        publish_page(ui, game, assets, body);
    }
}

fn bid_page(ui: &mut Ui, game: &mut Game, assets: &mut Assets, lots: &[AuctionLot], r: Rect) {
    let u = ui.u;
    let list = Rect::new(r.x, r.y, 306.0 * u, r.h);
    let table = Rect::new(list.right() + 24.0 * u, r.y, r.right() - list.right() - 24.0 * u, r.h);
    draw_line(list.right() + 12.0 * u, r.y + 4.0 * u, list.right() + 12.0 * u, r.bottom() - 4.0 * u, 1.0, kit::alpha(GOLD, 0.35));
    if lots.is_empty() {
        kit::empty(r, "No hay nada en subasta.", "Sé el primero en publicar.", u);
        return;
    }
    let me = game.me().map(|e| e.name.clone()).unwrap_or_default();
    let selected = FORM.with(|f| f.borrow().lot).filter(|id| lots.iter().any(|l| l.id == *id)).or_else(|| lots.first().map(|l| l.id));
    // ---- the lots
    let rh = 62.0 * u;
    let vis = ((list.h / rh).floor() as usize).max(1);
    let scroll = kit::wheel(list, FORM.with(|f| f.borrow().scroll), lots.len(), vis, 1);
    FORM.with(|f| f.borrow_mut().scroll = scroll);
    let mut hover = None;
    for (i, l) in lots.iter().enumerate().skip(scroll).take(vis) {
        let row = Rect::new(list.x, list.y + (i - scroll) as f32 * rh, list.w - 8.0 * u, rh - 4.0 * u);
        let (hov, click) = kit::row(row, selected == Some(l.id), u);
        let icon = Rect::new(row.x + 8.0 * u, row.y + 7.0 * u, 42.0 * u, 42.0 * u);
        kit::well(icon, u);
        assets.draw_obj(l.obj, Rect::new(icon.x + 3.0 * u, icon.y + 3.0 * u, icon.w - 6.0 * u, icon.h - 6.0 * u));
        let tx = icon.right() + 10.0 * u;
        let name = if l.amount > 1 { format!("{} ×{}", obj_name(assets, l.obj), l.amount) } else { obj_name(assets, l.obj) };
        kit::fit(FontKind::Title, &name, tx, row.y + 22.0 * u, 16.0 * u, row.right() - tx - 70.0 * u, if selected == Some(l.id) { GOLD_BRIGHT } else { TEXT });
        let price = if l.best > 0 { gold_str(l.best) } else { format!("base {}", gold_str(l.base_price)) };
        kit::icon(Icon::Coin, tx + 7.0 * u, row.y + 41.0 * u, 13.0 * u, Color::new(0.95, 0.76, 0.26, 1.0));
        text_with(FontKind::Body, &price, tx + 18.0 * u, row.y + 46.0 * u, 14.0 * u, if l.best > 0 { GOLD_TEXT } else { TEXT_DIM });
        let t = time_str(l.secs_left);
        let pulse = if l.secs_left < 60 { 0.6 + 0.4 * (get_time() * 5.0).sin() as f32 } else { 1.0 };
        kit::text_right(FontKind::Title, &t, row.right() - 8.0 * u, row.y + 22.0 * u, 16.0 * u, kit::alpha(time_color(l.secs_left), pulse));
        kit::icon(Icon::Hourglass, row.right() - 8.0 * u - crate::ui::widgets::measure_with(FontKind::Title, &t, 16.0 * u).width - 12.0 * u, row.y + 16.0 * u, 12.0 * u, kit::alpha(time_color(l.secs_left), 0.8));
        if click {
            FORM.with(|f| {
                let mut f = f.borrow_mut();
                f.lot = Some(l.id);
                f.bid = l.min_bid.to_string();
            });
        }
        if hov {
            hover = Some(l.clone());
        }
    }
    kit::scrollbar(list, lots.len(), vis, scroll, u);
    if let Some(l) = hover {
        tooltip(&[obj_name(assets, l.obj), format!("Vende: {}", l.seller), format!("Oferta mínima: {} de oro", gold_str(l.min_bid)), format!("Termina en {}", time_str(l.secs_left))], u);
    }

    // ---- the table
    let Some(l) = selected.and_then(|id| lots.iter().find(|l| l.id == id)) else {
        kit::empty(table, "Elige un objeto de la lista.", "", u);
        return;
    };
    let own = l.seller.eq_ignore_ascii_case(&me);
    // Plinth and name.
    let plinth = Rect::new(table.x, table.y, 92.0 * u, 92.0 * u);
    kit::well(plinth, u);
    kit::glow(plinth.center().x, plinth.center().y, 44.0 * u, Color::new(1.0, 0.75, 0.35, 1.0));
    draw_rectangle_lines(plinth.x, plinth.y, plinth.w, plinth.h, 1.0, kit::alpha(GOLD, 0.6));
    assets.draw_obj(l.obj, Rect::new(plinth.x + 8.0 * u, plinth.y + 8.0 * u, plinth.w - 16.0 * u, plinth.h - 16.0 * u));
    let tx = plinth.right() + 16.0 * u;
    kit::fit(FontKind::Title, &obj_name(assets, l.obj), tx, table.y + 34.0 * u, 24.0 * u, table.right() - tx, GOLD_BRIGHT);
    if l.amount > 1 {
        text_with(FontKind::Body, &format!("{} unidades", l.amount), tx, table.y + 58.0 * u, 16.0 * u, TEXT);
    }
    text_with(FontKind::NarrativeItalic, &format!("Lo vende {}", l.seller), tx, table.y + 80.0 * u, 16.0 * u, TEXT_DIM);

    // The countdown, big.
    let ty = table.y + 112.0 * u;
    let c = time_color(l.secs_left);
    let pulse = if l.secs_left < 60 { 0.7 + 0.3 * (get_time() * 5.0).sin() as f32 } else { 1.0 };
    text_with(FontKind::Title, "TERMINA EN", table.x, ty + 12.0 * u, 12.0 * u, GOLD_DIM);
    kit::icon(Icon::Hourglass, table.x + 16.0 * u, ty + 46.0 * u, 26.0 * u, kit::alpha(c, pulse));
    text_with(FontKind::Body, &time_str(l.secs_left), table.x + 40.0 * u, ty + 62.0 * u, 48.0 * u, kit::alpha(c, pulse));

    // The price and who leads.
    let py = ty + 90.0 * u;
    kit::rule(table.x, py, table.w, GOLD);
    if l.best > 0 {
        text_with(FontKind::Title, "OFERTA MÁS ALTA", table.x, py + 24.0 * u, 12.0 * u, GOLD_DIM);
        kit::icon(Icon::Coin, table.x + 12.0 * u, py + 54.0 * u, 22.0 * u, Color::new(0.95, 0.76, 0.26, 1.0));
        text_with(FontKind::Title, &gold_str(l.best), table.x + 30.0 * u, py + 62.0 * u, 28.0 * u, GOLD_BRIGHT);
        text_with(FontKind::NarrativeItalic, &format!("de {}", l.bidder), table.x + 36.0 * u + crate::ui::widgets::measure_with(FontKind::Title, &gold_str(l.best), 28.0 * u).width, py + 62.0 * u, 17.0 * u, TEXT_DIM);
    } else {
        text_with(FontKind::Title, "PRECIO BASE", table.x, py + 24.0 * u, 12.0 * u, GOLD_DIM);
        kit::icon(Icon::Coin, table.x + 12.0 * u, py + 54.0 * u, 22.0 * u, Color::new(0.95, 0.76, 0.26, 1.0));
        text_with(FontKind::Title, &gold_str(l.base_price), table.x + 30.0 * u, py + 62.0 * u, 28.0 * u, GOLD_BRIGHT);
        text_with(FontKind::NarrativeItalic, "sin ofertas todavía", table.x + 36.0 * u + crate::ui::widgets::measure_with(FontKind::Title, &gold_str(l.base_price), 28.0 * u).width, py + 62.0 * u, 17.0 * u, TEXT_FAINT);
    }

    // The bid.
    let by = py + 86.0 * u;
    if own {
        text_with(FontKind::NarrativeItalic, "Es tu subasta: espera a que alguien puje.", table.x, by + 26.0 * u, 17.0 * u, TEXT_DIM);
        return;
    }
    text_with(FontKind::Body, &format!("Puja mínima: {} de oro", gold_str(l.min_bid)), table.x, by + 4.0 * u, 15.0 * u, TEXT_DIM);
    let mut bid = None;
    FORM.with(|f| {
        let mut f = f.borrow_mut();
        let field = Rect::new(table.x, by + 14.0 * u, table.w - 128.0 * u, 36.0 * u);
        text_field(ui, "auc_bid", &mut f.bid, field, 9, false, |c| c.is_ascii_digit());
        let b = Rect::new(field.right() + 10.0 * u, field.y, 118.0 * u, 36.0 * u);
        if button_kind(b, "Pujar", Kind::Primary, true, u) {
            bid = Some(f.bid.trim().parse::<u32>().unwrap_or(0));
        }
        let q = Rect::new(table.x, field.bottom() + 8.0 * u, 118.0 * u, 24.0 * u);
        if button_kind(q, "Puja mínima", Kind::Ghost, true, u) {
            f.bid = l.min_bid.to_string();
        }
        let q2 = Rect::new(q.right() + 8.0 * u, q.y, 118.0 * u, 24.0 * u);
        if button_kind(q2, "+10 %", Kind::Ghost, true, u) {
            let base: u32 = f.bid.trim().parse().unwrap_or(l.min_bid);
            f.bid = (base + base / 10).max(l.min_bid).to_string();
        }
    });
    if let Some(amount) = bid {
        game.auction_bid(l.id, amount);
    }
}

fn publish_page(ui: &mut Ui, game: &mut Game, assets: &mut Assets, r: Rect) {
    let u = ui.u;
    let left = Rect::new(r.x, r.y, 330.0 * u, r.h);
    let right = Rect::new(left.right() + 24.0 * u, r.y, r.right() - left.right() - 24.0 * u, r.h);
    draw_line(left.right() + 12.0 * u, r.y + 4.0 * u, left.right() + 12.0 * u, r.bottom() - 4.0 * u, 1.0, kit::alpha(GOLD, 0.35));
    // ---- the bag: 24 slots, up to 42 with a backpack
    let gy = kit::section("Tu mochila", left.x, left.y, left.w, u);
    text_with(FontKind::NarrativeItalic, "Elige lo que quieres vender.", left.x, gy + 10.0 * u, 15.0 * u, TEXT_DIM);
    let cols = 6usize;
    let inv = game.local.inventory.clone();
    let last = inv.iter().rposition(|s| s.is_some()).map_or(0, |i| i + 1);
    let shown = last.max(24).div_ceil(cols) * cols;
    let rows = shown / cols;
    let full = (left.w / cols as f32).floor();
    let cell = full.min((left.h - 70.0 * u) / rows as f32).floor();
    let gy = gy + 24.0 * u;
    let mut sel = FORM.with(|f| f.borrow().slot);
    let mut hover = None;
    for i in 0..shown {
        let rect = Rect::new(left.x + (i % cols) as f32 * cell, gy + (i / cols) as f32 * cell, cell - 3.0 * u, cell - 3.0 * u);
        let item = inv.get(i).copied().flatten();
        let (h, clicked) = slot(rect, sel == Some(i as u8 + 1), u);
        if let Some(it) = item {
            assets.draw_obj(it.obj, Rect::new(rect.x + 2.0, rect.y + 2.0, rect.w - 4.0, rect.h - 4.0));
            if it.amount > 1 {
                text_with(FontKind::Body, &it.amount.to_string(), rect.x + 3.0 * u, rect.bottom() - 4.0 * u, 13.0 * u, TEXT);
            }
            if h {
                hover = Some(it.obj);
            }
            if clicked && kit::active() {
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

    // ---- the lot to publish
    let chosen = sel.and_then(|s| inv.get(s as usize - 1).copied().flatten());
    let plinth = Rect::new(right.x, right.y, 88.0 * u, 88.0 * u);
    kit::well(plinth, u);
    if chosen.is_some() {
        kit::glow(plinth.center().x, plinth.center().y, 40.0 * u, Color::new(1.0, 0.75, 0.35, 1.0));
    }
    draw_rectangle_lines(plinth.x, plinth.y, plinth.w, plinth.h, 1.0, kit::alpha(GOLD, 0.5));
    if let Some(c) = chosen {
        assets.draw_obj(c.obj, Rect::new(plinth.x + 8.0 * u, plinth.y + 8.0 * u, plinth.w - 16.0 * u, plinth.h - 16.0 * u));
    }
    let tx = plinth.right() + 16.0 * u;
    match chosen {
        Some(c) => {
            kit::fit(FontKind::Title, &obj_name(assets, c.obj), tx, right.y + 34.0 * u, 22.0 * u, right.right() - tx, GOLD_BRIGHT);
            text_with(FontKind::Body, &format!("En la mochila: {}", c.amount), tx, right.y + 58.0 * u, 15.0 * u, TEXT_DIM);
        }
        None => text_with(FontKind::NarrativeItalic, "Ningún objeto elegido", tx, right.y + 40.0 * u, 18.0 * u, TEXT_FAINT),
    }
    let mut publish = None;
    FORM.with(|f| {
        let mut f = f.borrow_mut();
        let mut y = right.y + 112.0 * u;
        let half = (right.w - 10.0 * u) / 2.0;
        text_with(FontKind::Body, "Cantidad", right.x, y, 13.0 * u, TEXT_DIM);
        text_with(FontKind::Body, "Precio base (oro)", right.x + half + 10.0 * u, y, 13.0 * u, TEXT_DIM);
        y += 6.0 * u;
        text_field(ui, "auc_amount", &mut f.amount, Rect::new(right.x, y, half, 30.0 * u), 5, false, |c| c.is_ascii_digit());
        text_field(ui, "auc_price", &mut f.price, Rect::new(right.x + half + 10.0 * u, y, half, 30.0 * u), 7, false, |c| c.is_ascii_digit());
        y += 52.0 * u;
        text_with(FontKind::Title, "DURACIÓN", right.x, y, 13.0 * u, GOLD_TEXT);
        y += 8.0 * u;
        let dw = (right.w - 16.0 * u) / 3.0;
        for (i, m) in [5u8, 10, 30].into_iter().enumerate() {
            let b = Rect::new(right.x + i as f32 * (dw + 8.0 * u), y, dw, 32.0 * u);
            if button_kind(b, &format!("{m} min"), if f.minutes == m { Kind::Primary } else { Kind::Secondary }, true, u) {
                f.minutes = m;
            }
        }
        y += 50.0 * u;
        kit::icon(Icon::Coin, right.x + 8.0 * u, y + 2.0 * u, 14.0 * u, kit::alpha(GOLD, 0.9));
        text_with(FontKind::Body, &format!("Publicar cuesta {FEE} de oro."), right.x + 22.0 * u, y + 7.0 * u, 14.0 * u, TEXT_DIM);
        text_with(FontKind::NarrativeItalic, "El reino cobra el 5 % de la venta.", right.x + 22.0 * u, y + 27.0 * u, 14.0 * u, TEXT_DIM);
        let b = Rect::new(right.x, right.bottom() - 44.0 * u, right.w, 40.0 * u);
        if button_kind(b, "Publicar", Kind::Primary, true, u) {
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
    let _ = (GOOD, INK_DIM, WAX);
}

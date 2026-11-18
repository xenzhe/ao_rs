//! TSAO progression windows:
//! - Exchange (TSAO `frmCanjes`, `frmDonaciones`, `frmNewTiendaTS`): tabs for tournament
//!   prizes, the TS shop and donation packs; the list on the left, the selected one on the
//!   right with what it gives, the price against your points and the button.
//! - Class bonus (TSAO message `99`): the two options of the tier just reached, as cards.
//! - Poll (TSAO `frmVotacion`): the question and its options; the results once you voted.
use crate::assets::Assets;
use crate::game::Game;
use crate::ui::character::kit::{self, Icon, WAX};
use crate::ui::inventory::{draw_cell, item_name, CellOpts};
use crate::ui::theme::{FontKind, BAD, GOLD, GOLD_BRIGHT, GOLD_TEXT, INK, TEXT, TEXT_DIM};
use crate::ui::widgets::{button_kind, Kind};
use crate::ui::{Ui, WindowId};
use ao_protocol::p4::tsao::TsaoC2S;
use client_core::ItemSlot;
use macroquad::prelude::*;
use std::cell::Cell;

thread_local! {
    static TAB: Cell<usize> = const { Cell::new(0) };
    static SEL: Cell<usize> = const { Cell::new(0) };
    static SCROLL: Cell<usize> = const { Cell::new(0) };
    static UNITS: Cell<u32> = const { Cell::new(1) };
    static TAB_ENV: Cell<bool> = const { Cell::new(false) };
    static UP_SCROLL: Cell<usize> = const { Cell::new(0) };
}

const ROW: f32 = 46.0;
/// Most prize units bought at once (the server's limit).
const MAX_UNITS: u32 = 10_000;
const GOOD_INK: Color = Color::new(0.16, 0.36, 0.12, 1.0);

/// One row of the list: icon object (0 = none), name, price, affordable.
struct Line {
    obj: u16,
    amount: u32,
    name: String,
    price: u32,
    ok: bool,
}

fn points_strip(game: &Game, x: f32, y: f32, w: f32, u: f32) {
    let p = game.local.p4.tsao.points;
    let parts = [("Torneo", p.tournament, Icon::Crown), ("TS", p.ts, Icon::Star), ("Créditos", p.credits, Icon::Coin)];
    let cw = w / 3.0;
    for (i, (label, v, ic)) in parts.iter().enumerate() {
        let cx = x + cw * i as f32;
        kit::icon(*ic, cx + 12.0 * u, y + 10.0 * u, 16.0 * u, Color::new(0.75, 0.52, 0.08, 1.0));
        kit::ink(FontKind::Body, label, cx + 26.0 * u, y + 15.0 * u, 14.0 * u, TEXT_DIM);
        kit::ink(FontKind::Title, &kit::thousands(*v as u64), cx + 26.0 * u + kit::measure_with(FontKind::Body, label, 14.0 * u).width + 8.0 * u, y + 16.0 * u, 17.0 * u, GOLD_TEXT);
    }
}

pub fn draw_exchange(ui: &mut Ui, game: &mut Game, assets: &mut Assets, r: Rect) {
    let u = ui.u;
    kit::begin(ui, WindowId::Exchange);
    let Some(ex) = game.local.p4.tsao.exchange.clone() else { return };
    let pts = game.local.p4.tsao.points;
    points_strip(game, r.x + 6.0 * u, r.y, r.w - 12.0 * u, u);
    let labels = vec!["Premios de torneo".to_string(), "Tienda TS".to_string(), "Donaciones".to_string()];
    // Screenshots: `AO_EXCHANGE_TAB=0|1|2` opens on that tab once.
    if let Some(t) = std::env::var("AO_EXCHANGE_TAB").ok().and_then(|t| t.parse().ok()).filter(|_| !TAB_ENV.with(|d| d.replace(true))) {
        TAB.with(|c| c.set(t));
    }
    let mut tab = TAB.with(Cell::get);
    if let Some(t) = kit::tabs(r.x + 8.0 * u, r.y + 30.0 * u, &labels, tab, u) {
        if t != tab {
            tab = t;
            SEL.with(|s| s.set(0));
            SCROLL.with(|s| s.set(0));
            UNITS.with(|s| s.set(1));
        }
    }
    TAB.with(|t| t.set(tab));
    let lines: Vec<Line> = match tab {
        0 => ex.prizes.iter().map(|z| Line { obj: z.obj, amount: 1, name: z.name.clone(), price: z.price, ok: z.price <= pts.tournament }).collect(),
        1 => ex.ts.iter().map(|t| Line { obj: t.obj, amount: t.amount, name: item_name(assets, t.obj), price: t.price, ok: t.price <= pts.ts }).collect(),
        _ => ex.packs.iter().map(|k| Line { obj: 0, amount: 1, name: k.name.clone(), price: k.price, ok: k.available && k.price <= pts.credits }).collect(),
    };
    let unit = ["pts", "TS", "créditos"][tab.min(2)];
    let top = r.y + 64.0 * u;
    let list = Rect::new(r.x + 6.0 * u, top, (r.w * 0.46).floor(), r.bottom() - top - 4.0 * u);
    let right = Rect::new(list.right() + 18.0 * u, top, r.right() - list.right() - 24.0 * u, list.h);
    draw_line(list.right() + 9.0 * u, top, list.right() + 9.0 * u, r.bottom() - 4.0 * u, 1.0, kit::alpha(GOLD, 0.35));

    // ---- list
    let vis = ((list.h / ((ROW + 4.0) * u)).floor() as usize).max(1);
    let off = kit::wheel(list, SCROLL.with(Cell::get), lines.len(), vis, 1);
    SCROLL.with(|s| s.set(off));
    let mut sel = SEL.with(Cell::get).min(lines.len().saturating_sub(1));
    for (k, (i, l)) in lines.iter().enumerate().skip(off).take(vis).enumerate() {
        let row = Rect::new(list.x, list.y + k as f32 * (ROW + 4.0) * u, list.w - 8.0 * u, ROW * u);
        kit::page(row, u);
        let (_, click) = kit::row_ink(row, i == sel, u);
        if click {
            sel = i;
            UNITS.with(|s| s.set(1));
        }
        let ic = Rect::new(row.x + 6.0 * u, row.y + 5.0 * u, row.h - 10.0 * u, row.h - 10.0 * u);
        if l.obj > 0 {
            draw_cell(assets, ic, Some(ItemSlot { obj: l.obj, amount: l.amount, equipped: false }), &CellOpts { no_amount: l.amount <= 1, ..Default::default() }, u);
        } else {
            kit::icon(Icon::Star, ic.center().x, ic.center().y, 18.0 * u, WAX);
        }
        let price = format!("{} {unit}", kit::thousands(l.price as u64));
        let pw = kit::measure_with(FontKind::Title, &price, 15.0 * u).width;
        kit::ink_fit(FontKind::Body, &l.name, ic.right() + 8.0 * u, row.y + 28.0 * u, 15.0 * u, row.w - ic.w - pw - 30.0 * u, INK);
        kit::text_right(FontKind::Title, &price, row.right() - 10.0 * u, row.y + 28.0 * u, 15.0 * u, if l.ok { GOOD_INK } else { WAX });
    }
    kit::scrollbar(list, lines.len(), vis, off, u);
    SEL.with(|s| s.set(sel));
    let Some(l) = lines.get(sel) else {
        kit::ink(FontKind::NarrativeItalic, "No hay nada para canjear.", right.x, right.y + 24.0 * u, 16.0 * u, TEXT_DIM);
        return;
    };

    // ---- detail
    let mut y = kit::section(&l.name, right.x, right.y, right.w, u);
    let mut detail: Vec<String> = Vec::new();
    match tab {
        0 => {
            let z = &ex.prizes[sel];
            for ((lo, hi), name) in z.stats.iter().zip(["Golpe", "Defensa", "Daño mágico", "Defensa mágica"]) {
                if *hi > 0 {
                    detail.push(format!("{name}: {lo} - {hi}"));
                }
            }
            if !z.desc.is_empty() && !z.desc.starts_with("No hay") {
                detail.push(z.desc.clone());
            }
        }
        1 => detail.push(format!("{} x {}", ex.ts[sel].amount, l.name)),
        _ => {
            let k = &ex.packs[sel];
            detail.extend(k.contents.iter().map(|c| format!("· {c}")));
            if !k.desc.is_empty() && !k.desc.starts_with("No hay") {
                detail.push(k.desc.clone());
            }
            if !k.available {
                detail.push("Este pack no se puede canjear en este servidor.".into());
            }
        }
    }
    if l.obj > 0 {
        let ic = Rect::new(right.x + 4.0 * u, y + 4.0 * u, 52.0 * u, 52.0 * u);
        draw_cell(assets, ic, Some(ItemSlot { obj: l.obj, amount: l.amount, equipped: false }), &CellOpts { no_amount: l.amount <= 1, ..Default::default() }, u);
        y = ic.bottom() + 6.0 * u;
    }
    for d in &detail {
        y = kit::paragraph(FontKind::Body, d, right.x + 4.0 * u, y + 4.0 * u, right.w - 8.0 * u, 15.0 * u, TEXT, 4) + 2.0 * u;
    }

    // ---- price and button
    let by = right.bottom() - 40.0 * u;
    let mut units = UNITS.with(Cell::get);
    if tab == 0 {
        let have = pts.tournament / l.price.max(1);
        let minus = Rect::new(right.x, by - 44.0 * u, 34.0 * u, 30.0 * u);
        let plus = Rect::new(right.x + 110.0 * u, by - 44.0 * u, 34.0 * u, 30.0 * u);
        kit::ink(FontKind::Body, "Unidades", right.x, by - 52.0 * u, 14.0 * u, TEXT_DIM);
        if button_kind(minus, "-", Kind::Secondary, units > 1, u) {
            units -= 1;
        }
        kit::ink_mid(FontKind::Title, &units.to_string(), right.x + 72.0 * u, by - 23.0 * u, 18.0 * u, 60.0 * u, TEXT);
        if button_kind(plus, "+", Kind::Secondary, units < have.clamp(1, MAX_UNITS), u) {
            units += 1;
        }
        UNITS.with(|s| s.set(units));
    } else {
        units = 1;
    }
    let total = l.price as u64 * units as u64;
    let have = [pts.tournament, pts.ts, pts.credits][tab.min(2)] as u64;
    let ok = l.ok && total <= have;
    kit::ink(FontKind::Body, &format!("Total: {} {unit}", kit::thousands(total)), right.x, by + 21.0 * u, 16.0 * u, if ok { GOLD_TEXT } else { BAD });
    if button_kind(Rect::new(right.right() - 150.0 * u, by, 150.0 * u, 32.0 * u), "Canjear", Kind::Primary, ok, u) {
        let index = sel as u16;
        game.tsao_send(match tab {
            0 => TsaoC2S::BuyPrize { index, amount: units },
            1 => TsaoC2S::BuyTs { index },
            _ => TsaoC2S::BuyPack { index },
        });
    }
}


/// The Octarine gem: each object you can upgrade, what it becomes and the button.
pub fn draw_upgrade(ui: &mut Ui, game: &mut Game, assets: &mut Assets, r: Rect) {
    let u = ui.u;
    kit::begin(ui, WindowId::Upgrade);
    let Some(list) = game.local.p4.tsao.upgrades.clone() else { return };
    let mut y = kit::section("Mejorar con la Gema Octarina", r.x + 6.0 * u, r.y, r.w - 12.0 * u, u);
    y += 6.0 * u;
    let row_h = 76.0 * u;
    let vis = (((r.bottom() - y) / (row_h + 6.0 * u)).floor() as usize).max(1);
    let area = Rect::new(r.x, y, r.w, r.bottom() - y);
    let off = kit::wheel(area, UP_SCROLL.with(Cell::get), list.len(), vis, 1);
    UP_SCROLL.with(|s| s.set(off));
    for (index, from, to) in list.iter().skip(off).take(vis) {
        let row = Rect::new(r.x + 6.0 * u, y, r.w - 18.0 * u, row_h);
        kit::page(row, u);
        let a = Rect::new(row.x + 8.0 * u, row.y + 12.0 * u, 52.0 * u, 52.0 * u);
        draw_cell(assets, a, Some(ItemSlot { obj: *from, amount: 1, equipped: false }), &CellOpts { no_amount: true, ..Default::default() }, u);
        kit::icon(Icon::Star, a.right() + 16.0 * u, a.center().y, 14.0 * u, WAX);
        let b = Rect::new(a.right() + 32.0 * u, a.y, 52.0 * u, 52.0 * u);
        draw_cell(assets, b, Some(ItemSlot { obj: to.obj, amount: 1, equipped: false }), &CellOpts { no_amount: true, ..Default::default() }, u);
        let tx = b.right() + 12.0 * u;
        let tw = row.right() - tx - 140.0 * u;
        kit::ink_fit(FontKind::Title, &to.name, tx, row.y + 26.0 * u, 16.0 * u, tw, INK);
        let stats: Vec<String> = to.stats.iter().zip(["Golpe", "Def.", "Daño mág.", "Def. mág."]).filter(|((_, hi), _)| *hi > 0).map(|((lo, hi), n)| format!("{n} {lo}-{hi}")).collect();
        kit::ink_fit(FontKind::Body, &format!("de {} · {}", item_name(assets, *from), stats.join(" · ")), tx, row.y + 50.0 * u, 13.0 * u, tw, INK);
        if button_kind(Rect::new(row.right() - 128.0 * u, row.y + 22.0 * u, 118.0 * u, 32.0 * u), "Mejorar", Kind::Primary, true, u) {
            game.tsao_send(TsaoC2S::Upgrade { index: *index });
        }
        y += row_h + 6.0 * u;
    }
    kit::scrollbar(area, list.len(), vis, off, u);
}

pub fn draw_bonus(ui: &mut Ui, game: &mut Game, _assets: &mut Assets, r: Rect) {
    let u = ui.u;
    kit::begin(ui, WindowId::Bonus);
    let Some((_, level, options)) = game.local.p4.tsao.bonus_offer.clone() else { return };
    let y = kit::section(&format!("Bonificación de nivel {level}"), r.x + 6.0 * u, r.y, r.w - 12.0 * u, u);
    let y = kit::paragraph(FontKind::NarrativeItalic, "Elige una mejora para tu personaje. La elección es para siempre.", r.x + 10.0 * u, y + 4.0 * u, r.w - 20.0 * u, 15.0 * u, TEXT_DIM, 2) + 8.0 * u;
    let cw = (r.w - 30.0 * u) / 2.0;
    let ch = r.bottom() - y - 56.0 * u;
    for (i, text) in options.iter().enumerate() {
        let c = Rect::new(r.x + 10.0 * u + i as f32 * (cw + 10.0 * u), y, cw, ch);
        kit::page(c, u);
        let hov = kit::hover(c);
        if hov {
            draw_rectangle_lines(c.x - 2.0, c.y - 2.0, c.w + 4.0, c.h + 4.0, 2.0 * u, GOLD_BRIGHT);
        }
        kit::icon([Icon::Shield, Icon::Star][i], c.center().x, c.y + 34.0 * u, 30.0 * u, WAX);
        kit::paragraph(FontKind::Body, text, c.x + 12.0 * u, c.y + 64.0 * u, c.w - 24.0 * u, 16.0 * u, INK, 6);
        if button_kind(Rect::new(c.x + 12.0 * u, c.bottom() - 42.0 * u, c.w - 24.0 * u, 30.0 * u), "Elegir", Kind::Primary, true, u) {
            game.bonus_choose(i as u8);
        }
    }
    if button_kind(Rect::new(r.right() - 160.0 * u, r.bottom() - 40.0 * u, 150.0 * u, 30.0 * u), "Más tarde", Kind::Ghost, true, u) {
        game.bonus_later();
    }
    kit::ink(FontKind::Body, "Escribe /bonus para verla otra vez.", r.x + 12.0 * u, r.bottom() - 20.0 * u, 13.0 * u, TEXT_DIM);
}

pub fn draw_poll(ui: &mut Ui, game: &mut Game, _assets: &mut Assets, r: Rect) {
    let u = ui.u;
    kit::begin(ui, WindowId::Poll);
    let Some(p) = game.local.p4.tsao.poll.clone() else { return };
    let mut y = kit::section("Consulta popular", r.x + 6.0 * u, r.y, r.w - 12.0 * u, u);
    y = kit::paragraph(FontKind::Title, &p.question, r.x + 10.0 * u, y + 6.0 * u, r.w - 20.0 * u, 18.0 * u, TEXT, 3) + 10.0 * u;
    let total: u32 = p.results.as_ref().map_or(0, |v| v.iter().sum());
    for (i, o) in p.options.iter().enumerate() {
        let row = Rect::new(r.x + 10.0 * u, y, r.w - 20.0 * u, 40.0 * u);
        kit::page(row, u);
        if let Some(v) = p.results.as_ref().and_then(|v| v.get(i)) {
            let pct = if total == 0 { 0.0 } else { *v as f32 / total as f32 };
            draw_rectangle(row.x + 2.0, row.y + 2.0, (row.w - 4.0) * pct, row.h - 4.0, kit::alpha(GOLD, 0.35));
            kit::ink_fit(FontKind::Body, o, row.x + 12.0 * u, row.y + 26.0 * u, 16.0 * u, row.w - 140.0 * u, INK);
            kit::text_right(FontKind::Title, &format!("{v} · {:.0}%", pct * 100.0), row.right() - 12.0 * u, row.y + 26.0 * u, 15.0 * u, INK);
        } else {
            let (_, click) = kit::row_ink(row, false, u);
            kit::ink_fit(FontKind::Body, o, row.x + 12.0 * u, row.y + 26.0 * u, 16.0 * u, row.w - 120.0 * u, INK);
            kit::text_right(FontKind::Body, "Votar", row.right() - 12.0 * u, row.y + 26.0 * u, 14.0 * u, WAX);
            if click {
                game.tsao_send(TsaoC2S::Vote { option: i as u8 });
            }
        }
        y += 46.0 * u;
    }
    let foot = if p.results.is_some() { format!("{total} votos. Tu cuenta ya votó.") } else { "Un voto por cuenta.".to_string() };
    kit::ink(FontKind::Body, &foot, r.x + 12.0 * u, r.bottom() - 14.0 * u, 13.0 * u, TEXT_DIM);
}

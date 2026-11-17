//! Window: inventory and equipment (drag & drop, use, equip, drop). OWNER: Sonnet 2 (docs/P3_CONTRATO.md).
//!
//! This file also holds the toolkit the four item windows (inventory, vault, merchant, player
//! trade) share: slot grids with click / double click / right click / drag, the context menu,
//! the quantity prompt (`QtyPrompt`), the drop resolution (what a dragged item does where it
//! is released) and the helpers they need. All of it is `pub(crate)` for `bank`, `shop` and
//! `trade`.
use crate::assets::Assets;
use crate::game::Game;
use crate::render::layout::Layout;
use crate::ui::theme::{GOLD, GOLD_TEXT, GOOD, INK, MUTED, SLOT, TEXT, WOOD_LIGHT};
use crate::ui::widgets::{measure, panel, text, text_fit};
use crate::ui::{Drag, Ui, WindowId};
use client_core::p3::items::{equip_category, primary_action, sale_value, tooltip_lines, usable_slots, EquipCat, Primary};
use client_core::ItemSlot;
use macroquad::prelude::*;
use std::cell::RefCell;
use std::collections::HashMap;

// ---------------------------------------------------------------- small helpers

/// Mouse position; tests and screenshots can force it (see `demo`).
pub(crate) fn mouse() -> Vec2 {
    DEMO_MOUSE.with(|m| m.get()).unwrap_or_else(|| mouse_position().into())
}

thread_local! {
    static DEMO_MOUSE: std::cell::Cell<Option<Vec2>> = const { std::cell::Cell::new(None) };
    /// Scripted buttons while a demo drives the mouse: (left down, left pressed this frame,
    /// right pressed this frame). `None`: the real mouse.
    static DEMO_BTN: std::cell::Cell<Option<(bool, bool, bool)>> = const { std::cell::Cell::new(None) };
}

/// Left button held (real, or scripted by the screenshot demo).
pub(crate) fn lmb_down() -> bool {
    DEMO_BTN.with(|b| b.get()).map_or_else(|| is_mouse_button_down(MouseButton::Left), |b| b.0)
}

/// Left button went down this frame.
pub(crate) fn lmb_pressed() -> bool {
    DEMO_BTN.with(|b| b.get()).map_or_else(|| is_mouse_button_pressed(MouseButton::Left), |b| b.1)
}

/// Right button went down this frame.
pub(crate) fn rmb_pressed() -> bool {
    DEMO_BTN.with(|b| b.get()).map_or_else(|| is_mouse_button_pressed(MouseButton::Right), |b| b.2)
}

/// Forces the mouse position seen by the item windows (screenshots only).
pub(crate) fn set_demo_mouse(p: Option<Vec2>) {
    DEMO_MOUSE.with(|m| m.set(p));
}

/// `1234567` -> `1.234.567`.
pub(crate) fn fmt_gold(n: u64) -> String {
    let s = n.to_string();
    let mut out = String::new();
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (s.len() - i).is_multiple_of(3) {
            out.push('.');
        }
        out.push(c);
    }
    out
}

fn fmt_amount(n: u32) -> String {
    if n >= 10_000 {
        format!("{}k", n / 1000)
    } else {
        n.to_string()
    }
}

/// A button in the AO look that can be disabled and only reacts when `act`.
pub(crate) fn ibtn(r: Rect, label: &str, enabled: bool, act: bool, u: f32) -> bool {
    let hover = act && enabled && r.contains(mouse());
    let bg = if !enabled {
        Color::new(0.09, 0.07, 0.05, 1.0)
    } else if hover {
        Color::new(0.34, 0.23, 0.10, 1.0)
    } else {
        WOOD_LIGHT
    };
    draw_rectangle(r.x, r.y, r.w, r.h, bg);
    draw_rectangle_lines(r.x, r.y, r.w, r.h, 1.0, if enabled { GOLD } else { Color::new(0.30, 0.25, 0.18, 1.0) });
    let size = 16.0 * u;
    let d = measure(label, size);
    text(label, r.x + (r.w - d.width) / 2.0, r.y + r.h / 2.0 + d.height / 2.0, size, if !enabled { MUTED } else if hover { WHITE } else { TEXT });
    hover && lmb_pressed()
}

/// Width of a button for `label` at the standard size.
pub(crate) fn btn_w(label: &str, u: f32) -> f32 {
    measure(label, 16.0 * u).width + 18.0 * u
}

/// Tooltip box near `at`, several lines (the first one in gold).
pub(crate) fn tooltip_at(lines: &[String], at: Vec2, u: f32) {
    if lines.is_empty() {
        return;
    }
    let size = 16.0 * u;
    let w = lines.iter().map(|l| measure(l, size).width).fold(0.0, f32::max) + 16.0 * u;
    let h = lines.len() as f32 * 19.0 * u + 10.0 * u;
    let x = (at.x + 18.0 * u).min(screen_width() - w - 4.0).max(2.0);
    let y = (at.y + 18.0 * u).min(screen_height() - h - 4.0).max(2.0);
    draw_rectangle(x, y, w, h, Color::new(0.04, 0.03, 0.02, 0.96));
    draw_rectangle_lines(x, y, w, h, 1.0, GOLD);
    for (i, l) in lines.iter().enumerate() {
        text(l, x + 8.0 * u, y + 20.0 * u + i as f32 * 19.0 * u, size, if i == 0 { GOLD_TEXT } else { TEXT });
    }
}

/// Tooltip lines of an object of the inventory / vault / merchant, with `extra` lines last.
pub(crate) fn item_tip(game: &Game, assets: &Assets, obj: u16, amount: u32, equipped: bool, extra: &[String]) -> Vec<String> {
    let mut v = match assets.objs.get(obj as u32) {
        Some(o) => tooltip_lines(o, amount, equipped, game.stats.level as i32),
        None => vec![format!("Objeto {obj}")],
    };
    v.extend(extra.iter().cloned());
    v
}

fn has_backpack(game: &Game, assets: &Assets) -> bool {
    game.local.inventory.iter().flatten().any(|i| i.equipped && assets.objs.get(i.obj as u32).is_some_and(|o| o.obj_type == ao_formats::dat::obj::ObjType::Backpack))
}

// ---------------------------------------------------------------- shared state

/// A slot under the mouse or being pressed: where it lives, which slot, what and how many.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Hit {
    /// `Drag.from` space: `Inventory`, `Bank` (vault) or `Shop` (merchant stock).
    pub from: WindowId,
    pub slot: u8,
    pub obj: u16,
    pub amount: u32,
}

pub(crate) enum Ev {
    Click(Hit),
    Double(Hit),
    Right(Hit, Vec2),
    Drag,
}

/// Places inside the item windows a dragged item can be released on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Zone {
    InvCell(u8),
    InvArea,
    VaultArea,
    StockArea,
    OfferMine,
    EquipStrip,
}

/// What a quantity prompt does with the number typed.
#[derive(Clone, Copy, Debug)]
pub(crate) enum Pending {
    Drop(u8),
    DropGold,
    Deposit(u8),
    Withdraw(u8),
    DepositGold,
    WithdrawGold,
    Buy(u8),
    Sell(u8),
    Offer(u8),
    OfferGold,
}

struct Prompt {
    pending: Pending,
    q: QtyPrompt,
}

struct Menu {
    win: WindowId,
    hit: Hit,
    pos: Vec2,
    fresh: bool,
}

#[derive(Default)]
struct St {
    /// Selected slot of the inventory (shared by every inventory pane).
    sel_inv: u8,
    /// Selected vault / merchant slot.
    sel_other: HashMap<WindowId, u8>,
    press: Option<(Hit, WindowId, Vec2)>,
    last_click: Option<(Hit, f64)>,
    drag_src: Option<WindowId>,
    menu: Option<Menu>,
    prompts: HashMap<WindowId, Prompt>,
    zones: HashMap<WindowId, (Rect, Vec<(Zone, Rect)>)>,
    scroll: HashMap<WindowId, usize>,
    /// When each window last drew (a prompt or menu of a closed window is dropped).
    last_draw: HashMap<WindowId, f64>,
}

thread_local! {
    static ST: RefCell<St> = RefCell::new(St::default());
}

fn st<R>(f: impl FnOnce(&mut St) -> R) -> R {
    ST.with(|s| f(&mut s.borrow_mut()))
}

const MINE: [WindowId; 4] = [WindowId::Inventory, WindowId::Bank, WindowId::Shop, WindowId::Trade];

/// Registers where the zones of window `w` are this frame (`content` is the rect `ui.window`
/// gave: the frame around it is derived from it).
pub(crate) fn register(w: WindowId, content: Rect, u: f32, zones: Vec<(Zone, Rect)>) {
    let full = Rect::new(content.x - 12.0 * u, content.y - 38.0 * u, content.w + 24.0 * u, content.h + 50.0 * u);
    let now = get_time();
    st(|s| {
        s.zones.insert(w, (full, zones));
        s.last_draw.insert(w, now);
    });
}

/// Topmost item window under `p` and the innermost zone there.
pub(crate) fn zone_at(ui: &Ui, p: Vec2) -> Option<(WindowId, Option<Zone>)> {
    let order = ui.open_windows();
    st(|s| {
        for w in order.iter().rev().filter(|w| MINE.contains(w)) {
            if let Some((full, zones)) = s.zones.get(w) {
                if full.contains(p) {
                    let z = zones.iter().find(|(z, r)| matches!(z, Zone::InvCell(_)) && r.contains(p)).or_else(|| zones.iter().find(|(_, r)| r.contains(p))).map(|(z, _)| *z);
                    return Some((*w, z));
                }
            }
        }
        None
    })
}

fn modal(w: WindowId) -> bool {
    st(|s| s.prompts.contains_key(&w) || s.menu.as_ref().is_some_and(|m| m.win == w))
}

/// Whether the mouse belongs to window `w` this frame (nothing of ours above it, no prompt).
pub(crate) fn active(ui: &Ui, w: WindowId) -> bool {
    zone_at(ui, mouse()).map(|(x, _)| x) == Some(w) && !modal(w)
}

pub(crate) fn selected_inv() -> u8 {
    st(|s| s.sel_inv)
}

pub(crate) fn selected_other(w: WindowId) -> u8 {
    st(|s| s.sel_other.get(&w).copied().unwrap_or(0))
}

pub(crate) fn set_selected_other(w: WindowId, slot: u8) {
    st(|s| {
        s.sel_other.insert(w, slot);
    });
}

pub(crate) fn scroll_of(w: WindowId) -> usize {
    st(|s| s.scroll.get(&w).copied().unwrap_or(0))
}

pub(crate) fn set_scroll(w: WindowId, v: usize) {
    st(|s| {
        s.scroll.insert(w, v);
    });
}

// ---------------------------------------------------------------- slot grid

/// A grid of square slots (all sizes in pixels, already scaled).
#[derive(Clone, Copy)]
pub(crate) struct Grid {
    pub cols: usize,
    pub rows: usize,
    pub cell: f32,
    pub gap: f32,
}

impl Grid {
    pub fn size(&self) -> Vec2 {
        vec2(self.cols as f32 * (self.cell + self.gap) - self.gap, self.rows as f32 * (self.cell + self.gap) - self.gap)
    }

    pub fn rect(&self, o: Vec2, i: usize) -> Rect {
        let (c, r) = (i % self.cols, i / self.cols);
        Rect::new(o.x + c as f32 * (self.cell + self.gap), o.y + r as f32 * (self.cell + self.gap), self.cell, self.cell)
    }

    pub fn area(&self, o: Vec2) -> Rect {
        let s = self.size();
        Rect::new(o.x, o.y, s.x, s.y)
    }

    pub fn at(&self, o: Vec2, p: Vec2) -> Option<usize> {
        (0..self.cols * self.rows).find(|i| self.rect(o, *i).contains(p))
    }
}

/// One slot: frame, icon, amount, equipped tag.
#[allow(clippy::too_many_arguments)]
pub(crate) fn draw_cell(assets: &mut Assets, r: Rect, it: Option<ItemSlot>, selected: bool, hover: bool, locked: bool, drop_hint: bool, u: f32) {
    draw_rectangle(r.x, r.y, r.w, r.h, SLOT);
    let c = if drop_hint && hover {
        GOOD
    } else if selected {
        Color::new(1.0, 0.85, 0.3, 1.0)
    } else if hover {
        Color::new(0.85, 0.68, 0.36, 1.0)
    } else {
        Color::new(0.35, 0.26, 0.14, 1.0)
    };
    draw_rectangle_lines(r.x, r.y, r.w, r.h, if selected || (drop_hint && hover) { 2.0 * u } else { 1.0 }, c);
    if let Some(it) = it {
        let pad = (r.w * 0.08).max(2.0);
        assets.draw_obj(it.obj, Rect::new(r.x + pad, r.y + pad, r.w - 2.0 * pad, r.h - 2.0 * pad));
        if it.amount > 1 {
            let size = (r.w * 0.36).max(11.0);
            let s = fmt_amount(it.amount);
            let d = measure(&s, size);
            text(&s, r.right() - d.width - 2.0, r.bottom() - 3.0, size, WHITE);
        }
        if it.equipped {
            let t = (r.w * 0.34).max(10.0);
            draw_rectangle(r.x + 1.0, r.y + 1.0, t, t, GOLD);
            let d = measure("E", t);
            text("E", r.x + 1.0 + (t - d.width) / 2.0, r.y + 1.0 + t * 0.84, t * 0.9, INK);
        }
    }
    if locked {
        draw_rectangle(r.x, r.y, r.w, r.h, Color::new(0.0, 0.0, 0.0, 0.55));
    }
}

/// What a grid shows.
pub(crate) struct GridView<'a> {
    pub slots: &'a [Option<ItemSlot>],
    pub from: WindowId,
    pub selected: u8,
    /// Slots from this index (0-based) on are drawn as locked.
    pub locked_from: usize,
}

/// Draws a grid. Returns the hit under the mouse (when `act`) and the zones it occupies.
pub(crate) fn draw_grid(ui: &Ui, assets: &mut Assets, g: &Grid, o: Vec2, v: &GridView, act: bool) -> (Option<Hit>, Vec<(Zone, Rect)>) {
    let u = ui.u;
    let m = mouse();
    let hover_i = if act { g.at(o, m) } else { None };
    let dragging = ui.drag.is_some();
    let mut hit = None;
    let mut zones = Vec::new();
    for i in 0..(g.cols * g.rows).min(v.slots.len()) {
        let r = g.rect(o, i);
        let slot = i as u8 + 1;
        let it = v.slots[i];
        let hover = hover_i == Some(i);
        draw_cell(assets, r, it, v.selected == slot && it.is_some(), hover, i >= v.locked_from, dragging, u);
        if v.from == WindowId::Inventory {
            zones.push((Zone::InvCell(slot), r));
        }
        if hover {
            if let Some(it) = it {
                hit = Some(Hit { from: v.from, slot, obj: it.obj, amount: it.amount });
            }
        }
    }
    zones.push((if v.from == WindowId::Inventory { Zone::InvArea } else { Zone::VaultArea }, g.area(o)));
    (hit, zones)
}

/// Mouse bookkeeping of a pane: turns presses and releases into one event per frame
/// (click, double click, right click, start of a drag). `win` is the window the pane is in.
pub(crate) fn track(ui: &mut Ui, win: WindowId, act: bool, hovered: Option<Hit>) -> Option<Ev> {
    let u = ui.u;
    let m = mouse();
    let down = lmb_down();
    if act && lmb_pressed() {
        if let Some(h) = hovered {
            st(|s| s.press = Some((h, win, m)));
        }
    }
    let mut ev = None;
    if let Some((h, pw, origin)) = st(|s| s.press) {
        if pw == win {
            if down {
                if ui.drag.is_none() && (m - origin).length() > 5.0 * u {
                    ui.drag = Some(Drag { from: h.from, index: h.slot, what: h.obj, amount: h.amount });
                    st(|s| {
                        s.drag_src = Some(win);
                        s.press = None;
                    });
                    ev = Some(Ev::Drag);
                }
            } else {
                st(|s| s.press = None);
                if ui.drag.is_none() {
                    let now = get_time();
                    let dbl = st(|s| {
                        let d = s.last_click.is_some_and(|(l, t)| l.from == h.from && l.slot == h.slot && now - t < 0.4);
                        s.last_click = if d { None } else { Some((h, now)) };
                        d
                    });
                    ev = Some(if dbl { Ev::Double(h) } else { Ev::Click(h) });
                }
            }
        }
    }
    if act && rmb_pressed() {
        if let Some(h) = hovered {
            ev = Some(Ev::Right(h, m));
        }
    }
    ev
}

/// Reacts to an event of a pane of window `win`.
pub(crate) fn handle(game: &mut Game, assets: &Assets, win: WindowId, ev: Ev) {
    match ev {
        Ev::Click(h) => match h.from {
            WindowId::Inventory => st(|s| s.sel_inv = h.slot),
            w => set_selected_other(w, h.slot),
        },
        Ev::Double(h) => match (h.from, win) {
            (WindowId::Inventory, WindowId::Bank) => start(game, assets, Op::Deposit, h, win),
            (WindowId::Inventory, WindowId::Shop) => start(game, assets, Op::Sell, h, win),
            (WindowId::Inventory, WindowId::Trade) => start(game, assets, Op::Offer, h, win),
            (WindowId::Inventory, _) => start(game, assets, Op::Primary, h, win),
            (WindowId::Bank, _) => start(game, assets, Op::Withdraw, h, win),
            (WindowId::Shop, _) => {
                game.shop_buy(h.slot, 1);
            }
            _ => {}
        },
        Ev::Right(h, pos) => st(|s| s.menu = Some(Menu { win, hit: h, pos, fresh: true })),
        Ev::Drag => {}
    }
}

// ---------------------------------------------------------------- operations

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Op {
    /// Use or equip, by object type.
    Primary,
    Use,
    Equip,
    Drop,
    /// Dropped on the world: always asks.
    DropWorld,
    Deposit,
    Withdraw,
    Buy,
    Sell,
    Offer,
}

/// Runs `op` on `hit`, asking for a quantity (in window `win`) when it makes sense.
pub(crate) fn start(game: &mut Game, assets: &Assets, op: Op, hit: Hit, win: WindowId) {
    let slot = hit.slot;
    let obj = assets.objs.get(hit.obj as u32);
    match op {
        Op::Primary => match obj.map(|o| primary_action(o.obj_type)) {
            Some(Primary::Equip) => {
                game.inv_equip(slot);
            }
            _ => {
                game.inv_use(slot);
            }
        },
        Op::Use => {
            game.inv_use(slot);
        }
        Op::Equip => {
            game.inv_equip(slot);
        }
        Op::Drop => ask(game, win, Pending::Drop(slot), "Tirar al suelo", hit.amount, false, None),
        Op::DropWorld => ask(game, win, Pending::Drop(slot), "Tirar al suelo", hit.amount, true, None),
        Op::Deposit => ask(game, win, Pending::Deposit(slot), "Guardar en la bóveda", hit.amount, false, None),
        Op::Withdraw => {
            let n = game.bank_item(slot).map_or(0, |i| i.amount);
            ask(game, win, Pending::Withdraw(slot), "Retirar de la bóveda", n, false, None)
        }
        Op::Buy => {
            let max = game.shop_max_buy(slot);
            if max == 0 {
                game.say("No tienes oro suficiente.", ao_protocol::Tone::Warn);
                return;
            }
            let (value, skill) = (obj.map_or(0, |o| o.value), game.local.skills.get(ao_rules::trade::SKILL_COMMERCE).copied().unwrap_or(0));
            let total: Box<dyn Fn(u32) -> u64> = Box::new(move |n| ao_rules::trade::buy_price(value, skill, n));
            ask(game, win, Pending::Buy(slot), "Comprar", max, false, Some(total))
        }
        Op::Sell => {
            let Some(o) = obj else { return };
            if o.no_tradeable_to_npc || o.newbie != 0 {
                game.say("El comerciante no compra eso.", ao_protocol::Tone::Warn);
                return;
            }
            let o = o.clone();
            let total: Box<dyn Fn(u32) -> u64> = Box::new(move |n| sale_value(&o, n));
            ask(game, win, Pending::Sell(slot), "Vender", hit.amount, false, Some(total))
        }
        Op::Offer => {
            let max = game.offerable(slot);
            ask(game, win, Pending::Offer(slot), "Ofrecer", max, false, None)
        }
    }
}

/// Opens a quantity prompt, or acts at once when there is a single unit to move.
fn ask(game: &mut Game, win: WindowId, pending: Pending, title: &str, max: u32, force: bool, total: Option<Box<dyn Fn(u32) -> u64>>) {
    if max == 0 {
        game.say("No hay nada que mover.", ao_protocol::Tone::Warn);
        return;
    }
    if max == 1 && !force {
        return apply(game, pending, 1);
    }
    open_prompt(win, pending, title, max, 1, total);
}

pub(crate) fn open_prompt(win: WindowId, pending: Pending, title: &str, max: u32, default: u32, total: Option<Box<dyn Fn(u32) -> u64>>) {
    st(|s| {
        s.prompts.insert(win, Prompt { pending, q: QtyPrompt { title: title.to_string(), max, text: String::new(), default, total, focused: false } });
    });
}

fn apply(game: &mut Game, p: Pending, n: u32) {
    match p {
        Pending::Drop(s) => game.inv_drop(s, n),
        Pending::DropGold => game.inv_drop_gold(n),
        Pending::Deposit(s) => game.bank_deposit(s, n),
        Pending::Withdraw(s) => game.bank_withdraw(s, n),
        Pending::DepositGold => game.bank_deposit_gold(n),
        Pending::WithdrawGold => game.bank_withdraw_gold(n),
        Pending::Buy(s) => game.shop_buy(s, n),
        Pending::Sell(s) => game.shop_sell(s, n),
        Pending::Offer(s) => game.trade_offer(s, n),
        Pending::OfferGold => game.trade_offer_gold(n),
    };
}

// ---------------------------------------------------------------- quantity prompt

pub(crate) enum QtyEvent {
    Open,
    Ok(u32),
    Cancel,
}

/// Reusable quantity widget: a number field (keyboard: digits, Up/Down, Enter, Esc) with
/// the quick buttons 1, 10, 100 and "Todo", and an optional running total.
pub(crate) struct QtyPrompt {
    pub title: String,
    pub max: u32,
    pub text: String,
    pub default: u32,
    pub total: Option<Box<dyn Fn(u32) -> u64>>,
    focused: bool,
}

impl QtyPrompt {
    pub fn value(&self) -> u32 {
        client_core::p3::items::parse_qty(&self.text, self.max).unwrap_or_else(|| self.default.clamp(1, self.max.max(1)))
    }

    /// Draws the prompt centred in `area` (dimming it) and returns what happened.
    pub fn show(&mut self, ui: &mut Ui, area: Rect, id: &str) -> QtyEvent {
        let u = ui.u;
        draw_rectangle(area.x - 6.0 * u, area.y - 6.0 * u, area.w + 12.0 * u, area.h + 12.0 * u, Color::new(0.0, 0.0, 0.0, 0.5));
        let (bw, bh) = (280.0 * u, 130.0 * u);
        let b = Rect::new((area.x + (area.w - bw) / 2.0).round(), (area.y + (area.h - bh) / 2.0).round(), bw, bh);
        panel(b, u);
        text_fit(&self.title, b.x + 12.0 * u, b.y + 25.0 * u, 18.0 * u, bw - 24.0 * u, GOLD_TEXT);
        // The prompt is modal: the field keeps the keyboard even after a click elsewhere.
        if ui.focus.as_deref() != Some(id) {
            ui.focus = Some(id.to_string());
        }
        self.focused = true;
        ui.keyboard_captured = true;
        ui.mouse_captured = true;
        let field = Rect::new(b.x + 12.0 * u, b.y + 36.0 * u, 84.0 * u, 28.0 * u);
        let mut enter = crate::ui::widgets::text_field(ui, id, &mut self.text, field, 9, false, |c| c.is_ascii_digit());
        if self.text.is_empty() {
            text(&self.default.to_string(), field.x + 8.0 * u, field.y + field.h / 2.0 + 6.0 * u, 18.0 * u, MUTED);
        }
        let mut bx = field.right() + 8.0 * u;
        for (label, v) in [("1", 1u32), ("10", 10), ("100", 100), ("Todo", self.max)] {
            let w = (btn_w(label, u)).max(30.0 * u);
            if ibtn(Rect::new(bx, field.y, w, field.h), label, true, true, u) {
                self.text = v.min(self.max).to_string();
            }
            bx += w + 4.0 * u;
        }
        text(&format!("Máx. {}", fmt_gold(self.max as u64)), b.x + 12.0 * u, b.y + 84.0 * u, 15.0 * u, MUTED);
        if let Some(f) = &self.total {
            let s = format!("Total: {} oro", fmt_gold(f(self.value())));
            let d = measure(&s, 16.0 * u);
            text(&s, b.right() - 12.0 * u - d.width, b.y + 84.0 * u, 16.0 * u, GOLD_TEXT);
        }
        if is_key_pressed(KeyCode::Up) {
            self.text = (self.value() + 1).min(self.max).to_string();
        }
        if is_key_pressed(KeyCode::Down) {
            self.text = self.value().saturating_sub(1).max(1).to_string();
        }
        enter |= is_key_pressed(KeyCode::Enter) || is_key_pressed(KeyCode::KpEnter);
        let (ok_l, no_l) = ("Aceptar", "Cancelar");
        let (ow, nw) = (btn_w(ok_l, u).max(96.0 * u), btn_w(no_l, u).max(96.0 * u));
        let y = b.y + 94.0 * u;
        let ok = ibtn(Rect::new(b.right() - 12.0 * u - nw - 8.0 * u - ow, y, ow, 28.0 * u), ok_l, true, true, u);
        let no = ibtn(Rect::new(b.right() - 12.0 * u - nw, y, nw, 28.0 * u), no_l, true, true, u);
        if ok || enter {
            ui.focus = None;
            return QtyEvent::Ok(self.value());
        }
        if no || is_key_pressed(KeyCode::Escape) {
            ui.focus = None;
            return QtyEvent::Cancel;
        }
        QtyEvent::Open
    }
}

/// Draws and runs the quantity prompt of window `w`, if it has one open.
pub(crate) fn prompt_ui(ui: &mut Ui, game: &mut Game, w: WindowId, area: Rect) {
    let Some(mut p) = st(|s| s.prompts.remove(&w)) else { return };
    match p.q.show(ui, area, &format!("qty_{w:?}")) {
        QtyEvent::Open => st(|s| {
            s.prompts.insert(w, p);
        }),
        QtyEvent::Ok(n) => apply(game, p.pending, n),
        QtyEvent::Cancel => {}
    }
}

// ---------------------------------------------------------------- context menu

fn menu_options(ui: &Ui, game: &Game, assets: &Assets, hit: Hit) -> Vec<(String, Op)> {
    let mut v = Vec::new();
    match hit.from {
        WindowId::Inventory => {
            let equipped = game.local.slot(hit.slot).is_some_and(|i| i.equipped);
            let t = assets.objs.get(hit.obj as u32).map(|o| o.obj_type);
            match t.map(primary_action) {
                Some(Primary::Equip) => v.push((if equipped { "Quitar" } else { "Equipar" }.to_string(), Op::Equip)),
                _ => v.push(("Usar".to_string(), Op::Use)),
            }
            if !equipped {
                v.push(("Tirar...".to_string(), Op::Drop));
                if game.local.bank.is_some() && ui.is_open(WindowId::Bank) {
                    v.push(("Guardar en la bóveda...".to_string(), Op::Deposit));
                }
                if game.local.social.trade.shop.is_some() {
                    v.push(("Vender...".to_string(), Op::Sell));
                }
                if game.local.social.trade.player.is_some() {
                    v.push(("Ofrecer...".to_string(), Op::Offer));
                }
            }
        }
        WindowId::Bank => v.push(("Retirar...".to_string(), Op::Withdraw)),
        WindowId::Shop => v.push(("Comprar...".to_string(), Op::Buy)),
        _ => {}
    }
    v
}

/// Draws the context menu if it belongs to `win` and runs the chosen entry.
pub(crate) fn menu_ui(ui: &mut Ui, game: &mut Game, assets: &Assets, win: WindowId) {
    let u = ui.u;
    let Some(menu) = st(|s| s.menu.take_if(|m| m.win == win)) else { return };
    let opts = menu_options(ui, game, assets, menu.hit);
    if opts.is_empty() {
        return;
    }
    let (w, row) = (190.0 * u, 26.0 * u);
    let h = opts.len() as f32 * row + 8.0 * u;
    let r = Rect::new(menu.pos.x.min(screen_width() - w - 4.0), menu.pos.y.min(screen_height() - h - 4.0), w, h);
    draw_rectangle(r.x + 3.0 * u, r.y + 3.0 * u, r.w, r.h, Color::new(0.0, 0.0, 0.0, 0.4));
    draw_rectangle(r.x, r.y, r.w, r.h, Color::new(0.07, 0.05, 0.03, 0.98));
    draw_rectangle_lines(r.x, r.y, r.w, r.h, 1.5 * u, GOLD);
    ui.capture_mouse_if_over(r);
    let m = mouse();
    let mut chosen = None;
    for (i, (label, op)) in opts.iter().enumerate() {
        let rr = Rect::new(r.x + 4.0 * u, r.y + 4.0 * u + i as f32 * row, r.w - 8.0 * u, row);
        let hover = rr.contains(m);
        if hover {
            draw_rectangle(rr.x, rr.y, rr.w, rr.h, Color::new(0.34, 0.23, 0.10, 1.0));
        }
        text(label, rr.x + 8.0 * u, rr.y + rr.h / 2.0 + 5.5 * u, 17.0 * u, if hover { WHITE } else { TEXT });
        if hover && lmb_pressed() && !menu.fresh {
            chosen = Some(*op);
        }
    }
    let close = !menu.fresh && (is_key_pressed(KeyCode::Escape) || rmb_pressed() || (lmb_pressed() && !r.contains(m)));
    if let Some(op) = chosen {
        start(game, assets, op, menu.hit, win);
    } else if !close {
        st(|s| s.menu = Some(Menu { fresh: false, ..menu }));
    }
}

// ---------------------------------------------------------------- dragging

/// Registers the drop zones of a window and settles a drag that was released this frame.
/// Every item window calls it first.
pub(crate) fn begin(ui: &mut Ui, game: &mut Game, assets: &Assets) {
    demo_tick(ui, game, assets);
    purge(ui);
    let Some(d) = ui.drag.clone() else { return };
    // Spells and other windows share `Drag`: only settle what comes from the item windows.
    if !is_item_drag(&d) || lmb_down() {
        return;
    }
    ui.drag = None;
    st(|s| s.press = None);
    let hit = Hit { from: d.from, slot: d.index, obj: d.what, amount: d.amount };
    let src = st(|s| s.drag_src.take()).unwrap_or(d.from);
    let m = mouse();
    match zone_at(ui, m) {
        Some((w, zone)) => drop_on(game, assets, hit, src, w, zone),
        None => {
            if hit.from == WindowId::Inventory && over_world(m) {
                start(game, assets, Op::DropWorld, hit, src);
            }
        }
    }
}

fn is_item_drag(d: &Drag) -> bool {
    matches!(d.from, WindowId::Inventory | WindowId::Bank | WindowId::Shop)
}

/// Drops the quantity prompt and the menu of a window that is no longer open (the server
/// closed it, the player walked away, the X) and gives back the keyboard they held.
/// Called by every item window; also by `trade::draw_overlay` for the case that none is open.
pub(crate) fn purge(ui: &mut Ui) {
    let now = get_time();
    let stale = |s: &St, w: WindowId| !ui.is_open(w) || now - s.last_draw.get(&w).copied().unwrap_or(-10.0) > 0.25;
    let none_left = st(|s| {
        let dead: Vec<WindowId> = s.prompts.keys().copied().filter(|w| stale(s, *w)).collect();
        for w in dead {
            s.prompts.remove(&w);
        }
        if s.menu.as_ref().is_some_and(|m| stale(s, m.win)) {
            s.menu = None;
        }
        s.prompts.is_empty()
    });
    if none_left && ui.focus.as_deref().is_some_and(|f| f.starts_with("qty_")) {
        ui.focus = None;
    }
}

fn over_world(p: Vec2) -> bool {
    Layout::compute(false).world.contains(p) || Layout::compute(true).world.contains(p)
}

fn drop_on(game: &mut Game, assets: &Assets, hit: Hit, src: WindowId, w: WindowId, zone: Option<Zone>) {
    match hit.from {
        WindowId::Inventory => match (w, zone) {
            (_, Some(Zone::InvCell(b))) => {
                game.inv_swap(hit.slot, b);
            }
            (WindowId::Bank, Some(Zone::VaultArea)) => start(game, assets, Op::Deposit, hit, w),
            (WindowId::Shop, Some(Zone::StockArea)) => start(game, assets, Op::Sell, hit, w),
            (WindowId::Trade, Some(Zone::OfferMine)) => start(game, assets, Op::Offer, hit, w),
            (WindowId::Inventory, Some(Zone::EquipStrip)) => {
                if assets.objs.get(hit.obj as u32).is_some_and(|o| primary_action(o.obj_type) == Primary::Equip) && !game.local.slot(hit.slot).is_some_and(|i| i.equipped) {
                    game.inv_equip(hit.slot);
                }
            }
            _ => {
                let _ = src;
            }
        },
        WindowId::Bank if w == WindowId::Inventory || matches!(zone, Some(Zone::InvArea | Zone::InvCell(_))) => start(game, assets, Op::Withdraw, hit, if w == WindowId::Inventory { WindowId::Bank } else { w }),
        WindowId::Shop if w == WindowId::Inventory || matches!(zone, Some(Zone::InvArea | Zone::InvCell(_))) => start(game, assets, Op::Buy, hit, if w == WindowId::Inventory { WindowId::Shop } else { w }),
        _ => {}
    }
}

/// Icon of what is being dragged, under the cursor (drawn last, over every window).
pub fn draw_drag(ui: &mut Ui, assets: &mut Assets, d: &crate::ui::Drag) {
    let u = ui.u;
    if !is_item_drag(d) {
        return;
    }
    if !lmb_down() {
        // Nobody settled it (every item window closed meanwhile): cancel.
        ui.drag = None;
        return;
    }
    let m = mouse();
    let s = 36.0 * u;
    let r = Rect::new(m.x - s / 2.0, m.y - s / 2.0, s, s);
    draw_rectangle(r.x, r.y, r.w, r.h, Color::new(0.05, 0.04, 0.03, 0.75));
    draw_rectangle_lines(r.x, r.y, r.w, r.h, 1.5 * u, GOLD_TEXT);
    assets.draw_obj(d.what, Rect::new(r.x + 3.0, r.y + 3.0, r.w - 6.0, r.h - 6.0));
    if d.amount > 1 {
        let t = fmt_amount(d.amount);
        let dm = measure(&t, 14.0 * u);
        text(&t, r.right() - dm.width - 2.0, r.bottom() - 3.0, 14.0 * u, WHITE);
    }
    if d.from == WindowId::Inventory && zone_at(ui, m).is_none() && over_world(m) {
        text("Soltar para tirar", r.right() + 6.0 * u, r.y + r.h / 2.0 + 5.0 * u, 16.0 * u, GOLD_TEXT);
    }
}

// ---------------------------------------------------------------- the window

/// Draws the content of the window inside `r` (the frame is already drawn).
pub fn draw(ui: &mut Ui, game: &mut Game, assets: &mut Assets, r: Rect) {
    let u = ui.u;
    let w = WindowId::Inventory;
    begin(ui, game, assets);
    let act = active(ui, w);
    let m = mouse();
    let mut zones: Vec<(Zone, Rect)> = Vec::new();
    let mut tip: Option<Vec<String>> = None;

    // ---- equipment strip: one slot per kind, what is on is shown; click takes it off ----
    let cell = 38.0 * u;
    let gap = 4.0 * u;
    let sx = r.x + (r.w - (EquipCat::ALL.len() as f32 * cell + (EquipCat::ALL.len() - 1) as f32 * gap)) / 2.0;
    let mut worn: [Option<(u8, ItemSlot)>; 7] = [None; 7];
    for (i, it) in game.local.inventory.iter().enumerate() {
        let Some(it) = it.filter(|i| i.equipped) else { continue };
        if let Some(cat) = assets.objs.get(it.obj as u32).and_then(|o| equip_category(o.obj_type)) {
            let k = EquipCat::ALL.iter().position(|c| *c == cat).unwrap();
            worn[k].get_or_insert((i as u8 + 1, it));
        }
    }
    let strip = Rect::new(sx, r.y, EquipCat::ALL.len() as f32 * (cell + gap) - gap, cell);
    zones.push((Zone::EquipStrip, strip));
    for (k, cat) in EquipCat::ALL.iter().enumerate() {
        let cr = Rect::new(sx + k as f32 * (cell + gap), r.y, cell, cell);
        let hover = act && cr.contains(m);
        draw_cell(assets, cr, worn[k].map(|(_, i)| i), false, hover, false, false, u);
        match worn[k] {
            Some((slot, it)) => {
                if hover {
                    tip = Some(item_tip(game, assets, it.obj, 1, true, &["Clic para quitarlo".to_string()]));
                    if lmb_pressed() {
                        game.inv_equip(slot);
                    }
                }
            }
            None => {
                let label = cat.short();
                let size = 12.0 * u;
                let d = measure(label, size);
                text(label, cr.x + (cr.w - d.width) / 2.0, cr.y + cr.h / 2.0 + 4.0 * u, size, Color::new(0.45, 0.40, 0.33, 1.0));
                if hover {
                    tip = Some(vec![format!("{} (nada equipado)", cat.label())]);
                }
            }
        }
    }

    // ---- the 42 slots ----
    let bp = has_backpack(game, assets);
    let grid = Grid { cols: 7, rows: 6, cell: 40.0 * u, gap: 3.0 * u };
    let go = vec2(r.x + (r.w - grid.size().x) / 2.0, strip.bottom() + 8.0 * u);
    let inv = game.local.inventory.clone();
    let view = GridView { slots: &inv, from: WindowId::Inventory, selected: selected_inv(), locked_from: usable_slots(bp) };
    let (hit, z) = draw_grid(ui, assets, &grid, go, &view, act);
    zones.extend(z);
    if let (Some(h), None) = (hit, &ui.drag) {
        let eq = game.local.slot(h.slot).is_some_and(|i| i.equipped);
        tip = Some(item_tip(game, assets, h.obj, h.amount, eq, &[]));
    }
    if let Some(ev) = track(ui, w, act, hit) {
        handle(game, assets, w, ev);
    }

    // ---- gold ----
    let gy = go.y + grid.size().y + 8.0 * u;
    text(&format!("Oro: {}", fmt_gold(game.local.gold as u64)), r.x + 4.0 * u, gy + 18.0 * u, 19.0 * u, GOLD_TEXT);
    let bw = btn_w("Tirar oro", u);
    let gb = Rect::new(r.right() - bw, gy, bw, 26.0 * u);
    if ibtn(gb, "Tirar oro", game.local.gold > 0, act, u) {
        open_prompt(w, Pending::DropGold, "Tirar oro al suelo", game.local.gold, game.local.gold.min(1), None);
    }
    // What lies on the tile the character stands on (an object dropped, gold thrown).
    let pw = btn_w("Recoger", u);
    let here = game.me().is_some_and(|me| game.local.ground.contains_key(&(me.x, me.y)));
    if ibtn(Rect::new(gb.x - 4.0 * u - pw, gy, pw, 26.0 * u), "Recoger", here, act, u) {
        game.inv_pick_up();
    }

    // ---- trade: invitation, or the players close enough to ask ----
    let ty = gy + 34.0 * u;
    let row = Rect::new(r.x, ty, r.w, 26.0 * u);
    crate::ui::trade::inventory_row(ui, game, row, act);

    register(w, r, u, zones);
    menu_ui(ui, game, assets, w);
    prompt_ui(ui, game, w, r);
    if let Some(t) = tip {
        if ui.drag.is_none() && !modal(w) {
            tooltip_at(&t, m, u);
        }
    }
}

// ---------------------------------------------------------------- screenshots

/// Scripted scene for the screenshots (`AO_ITEMS_DEMO=inv|bank|shop|trade|trade_peer`, with
/// `AO_DEMO_TILE=x,y` of the banker / merchant to click). It talks to the real server: gives
/// objects with `/dar`, equips, opens the NPC. Nothing happens without the variable.
fn demo_tick(ui: &mut Ui, game: &mut Game, assets: &Assets) {
    thread_local! {
        static T0: std::cell::Cell<Option<f64>> = const { std::cell::Cell::new(None) };
        static STEP: std::cell::Cell<u32> = const { std::cell::Cell::new(0) };
        static LAST: std::cell::Cell<f64> = const { std::cell::Cell::new(0.0) };
        static DONE_AT: std::cell::Cell<f64> = const { std::cell::Cell::new(-1.0) };
    }
    static MODE: std::sync::OnceLock<Option<String>> = std::sync::OnceLock::new();
    let Some(mode) = MODE.get_or_init(|| std::env::var("AO_ITEMS_DEMO").ok()).clone() else { return };
    let now = get_time();
    // Several windows call this every frame: only the first one of the frame runs it.
    if now - DONE_AT.with(|d| d.get()) < 0.004 {
        return;
    }
    DONE_AT.with(|d| d.set(now));
    run_script(now);
    let t0 = T0.with(|t| {
        if t.get().is_none() {
            t.set(Some(now));
        }
        t.get().unwrap_or(now)
    });
    let t = now - t0;
    let step = STEP.with(|s| s.get());
    let gap = now - LAST.with(|l| l.get());
    let say = |game: &mut Game, s: String| game.send(&ao_protocol::C2S::Say { text: s });
    let advance = || {
        STEP.with(|s| s.set(s.get() + 1));
        LAST.with(|l| l.set(now));
    };
    // Objects to hand out: the first sensible one of each kind in obj.dat.
    let pick = |ty: ao_formats::dat::obj::ObjType, f: &dyn Fn(&ao_formats::dat::obj::ObjData) -> bool| -> Option<u16> {
        let free = |o: &ao_formats::dat::obj::ObjData| o.forbidden_classes.is_empty() && o.forbidden_races.is_empty() && !o.male_only && !o.female_only && o.royal == 0 && o.chaos == 0;
        assets.objs.iter().find(|(_, o)| o.obj_type == ty && o.grh > 0 && !o.name.is_empty() && free(o) && f(o)).map(|(i, _)| i as u16)
    };
    use ao_formats::dat::obj::ObjType as T;
    let gifts: Vec<(u16, u32)> = [
        (pick(T::Weapon, &|o| o.weapon_anim > 0 && o.min_level <= 1 && o.value > 0 && o.two_handed == 0), 1),
        (pick(T::Armor, &|o| o.min_level <= 1 && o.value > 0 && o.max_def > 0), 1),
        (pick(T::Shield, &|o| o.min_level <= 1 && o.value > 0), 1),
        (pick(T::Helmet, &|o| o.min_level <= 1 && o.value > 0), 1),
        (pick(T::Potions, &|o| o.value > 0), 12),
        (pick(T::UseOnce, &|o| o.min_hunger > 0), 8),
        (pick(T::Drinks, &|_| true), 5),
        (pick(T::RingAccessory, &|o| o.value > 0), 1),
        (pick(T::Arrows, &|o| o.value > 0), 60),
        (pick(T::Backpack, &|_| true), 1),
        (Some(12), 10_000),
    ]
    .into_iter()
    .filter_map(|(o, n)| o.map(|o| (o, n)))
    .collect();
    let slot_of = |g: &Game, ty: T| g.local.inventory.iter().position(|i| i.is_some_and(|i| assets.objs.get(i.obj as u32).is_some_and(|o| o.obj_type == ty))).map(|p| p as u8 + 1);
    if mode == "trade_peer" {
        // The other player: gives itself two objects, accepts the invitation, offers.
        match step {
            0 if t > 1.0 => {
                for (o, n) in gifts.iter().take(2) {
                    say(game, format!("/dar {o} {n}"));
                }
                say(game, "/dar 12 5000".into());
                advance();
            }
            1 if game.local.social.trade.invited_by.is_some() => {
                game.trade_accept_invite();
                advance();
            }
            2 if game.local.social.trade.player.is_some() && gap > 1.0 => {
                game.trade_offer(1, 1);
                game.trade_offer_gold(1500);
                advance();
            }
            3 if gap > 3.5 => {
                game.trade_confirm();
                advance();
            }
            _ => {}
        }
        return;
    }
    let n = gifts.len() as u32;
    if step > n && mode != "inv" && [WindowId::Bank, WindowId::Shop, WindowId::Trade].iter().any(|w| ui.is_open(*w)) {
        ui.close(WindowId::Inventory);
    }
    match step {
        s if s < n && gap > 0.35 && t > 1.0 => {
            let (o, c) = gifts[s as usize];
            say(game, format!("/dar {o} {c}"));
            advance();
        }
        s if s == n && gap > 1.0 => {
            for ty in [T::Weapon, T::Armor, T::Shield, T::Helmet, T::RingAccessory, T::Arrows] {
                if let Some(sl) = slot_of(game, ty) {
                    game.inv_equip(sl);
                }
            }
            advance();
        }
        s if s == n + 1 && gap > 1.0 && (mode != "trade" || !game.nearby_players().is_empty()) => {
            if let Ok(v) = std::env::var("AO_DEMO_TILE") {
                let p: Vec<i32> = v.split(',').filter_map(|x| x.trim().parse().ok()).collect();
                if p.len() == 2 && (mode == "bank" || mode == "shop") {
                    game.send(&ao_protocol::C2S::Interact { tx: p[0], ty: p[1] });
                }
            }
            if mode == "trade" {
                if let Some((id, _)) = game.nearby_players().first() {
                    game.trade_propose(*id);
                }
            }
            advance();
        }
        s if s == n + 2 && gap > 1.5 => {
            // Offers for the trade scene, a few items into the vault, the mouse over a slot.
            if mode == "trade" && game.local.social.trade.player.is_some() {
                if let Some(p) = slot_of(game, T::Potions) {
                    game.trade_offer(p, 5);
                }
                game.trade_offer_gold(2000);
            }
            if mode == "bank" && game.local.bank.is_some() {
                for ty in [T::Potions, T::UseOnce, T::Arrows] {
                    if let Some(sl) = slot_of(game, ty) {
                        game.bank_deposit(sl, 4);
                    }
                }
                game.bank_deposit_gold(8000);
            }
            advance();
        }
        s if s == n + 3 && gap > 1.2 => {
            if mode == "trade" {
                game.trade_confirm();
            }
            if mode == "shop" {
                set_selected_other(WindowId::Shop, 2);
                st(|s| s.sel_inv = 5);
            }
            if mode == "bank" {
                set_selected_other(WindowId::Bank, 1);
                st(|s| s.sel_inv = 5);
            }
            advance();
        }
        s if s == n + 4 => {
            // Mouse on a worn object (inv), a vault slot (bank), a stock row (shop).
            if let Ok(v) = std::env::var("AO_DEMO_MOUSE") {
                let p: Vec<f32> = v.split(',').filter_map(|x| x.trim().parse().ok()).collect();
                if p.len() == 2 {
                    set_demo_mouse(Some(vec2(p[0], p[1])));
                }
            }
            advance();
        }
        s if s == n + 5 && gap > 0.5 => {
            // Optional: the quantity prompt or the context menu open on the potions.
            if let Some(it) = game.local.slot(5) {
                if std::env::var("AO_DEMO_PROMPT").is_ok() {
                    let total: Box<dyn Fn(u32) -> u64> = Box::new(|n| n as u64 * 10);
                    open_prompt(WindowId::Inventory, Pending::Drop(5), "Tirar al suelo", it.amount, 1, Some(total));
                    st(|s| s.prompts.get_mut(&WindowId::Inventory).map(|p| p.q.text = "7".into()));
                }
                if std::env::var("AO_DEMO_MENU").is_ok() {
                    let hit = Hit { from: WindowId::Inventory, slot: 5, obj: it.obj, amount: it.amount };
                    st(|s| s.menu = Some(Menu { win: WindowId::Inventory, hit, pos: mouse(), fresh: false }));
                }
            }
            advance();
        }
        s if s == n + 6 && gap > 0.5 => {
            // Scripted mouse through the real input path: `swap` (drag slot to slot), `world`
            // (drag to the world), `dbl` (double click), `menu` (right click, pick an entry).
            if let Ok(kind) = std::env::var("AO_DEMO_INPUT") {
                let c = |slot: u8| cell_center(WindowId::Inventory, slot);
                let (u, t) = (ui.u, now);
                let script: Vec<(f64, Act)> = match (kind.as_str(), c(5)) {
                    ("swap", Some(a)) => match c(12) {
                        Some(b) => vec![(t, Act::Move(a)), (t + 0.1, Act::Down), (t + 0.2, Act::Move((a + b) / 2.0)), (t + 0.3, Act::Move(b)), (t + 0.5, Act::Up)],
                        None => vec![],
                    },
                    ("world", Some(a)) => {
                        let wr = Layout::compute(false).world;
                        let w = vec2(wr.x + 70.0 * u, wr.y + wr.h / 2.0);
                        vec![(t, Act::Move(a)), (t + 0.1, Act::Down), (t + 0.2, Act::Move((a + w) / 2.0)), (t + 0.3, Act::Move(w)), (t + 0.5, Act::Up)]
                    }
                    ("dbl", Some(a)) => vec![(t, Act::Move(a)), (t + 0.1, Act::Down), (t + 0.15, Act::Up), (t + 0.3, Act::Down), (t + 0.35, Act::Up)],
                    ("menu", Some(a)) => vec![(t, Act::Move(a)), (t + 0.1, Act::RDown), (t + 0.4, Act::Move(a + vec2(30.0, 43.0) * u)), (t + 0.6, Act::Down), (t + 0.65, Act::Up)],
                    _ => vec![],
                };
                SCRIPT.with(|s| *s.borrow_mut() = script);
            }
            advance();
        }
        _ => {}
    }
}

#[derive(Clone, Copy)]
enum Act {
    Move(Vec2),
    Down,
    Up,
    RDown,
}

thread_local! {
    static SCRIPT: RefCell<Vec<(f64, Act)>> = const { RefCell::new(Vec::new()) };
}

/// Plays the scripted mouse actions that are due, once per frame.
fn run_script(now: f64) {
    let running = DEMO_BTN.with(|b| b.get()).is_some();
    if !running && SCRIPT.with(|s| s.borrow().is_empty()) {
        return;
    }
    let mut btn = DEMO_BTN.with(|b| b.get()).unwrap_or((false, false, false));
    (btn.1, btn.2) = (false, false);
    SCRIPT.with(|s| {
        let mut s = s.borrow_mut();
        while s.first().is_some_and(|(t, _)| *t <= now) {
            match s.remove(0).1 {
                Act::Move(p) => set_demo_mouse(Some(p)),
                Act::Down => (btn.0, btn.1) = (true, true),
                Act::Up => btn.0 = false,
                Act::RDown => btn.2 = true,
            }
        }
    });
    let more = SCRIPT.with(|s| !s.borrow().is_empty());
    DEMO_BTN.with(|b| b.set((more || btn.0).then_some(btn)));
}

/// Centre of inventory slot `slot` in window `w` (from the zones it drew last frame).
fn cell_center(w: WindowId, slot: u8) -> Option<Vec2> {
    st(|s| s.zones.get(&w).and_then(|(_, z)| z.iter().find(|(zz, _)| *zz == Zone::InvCell(slot)).map(|(_, r)| vec2(r.x + r.w / 2.0, r.y + r.h / 2.0))))
}

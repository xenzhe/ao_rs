//! Window: inventory and equipment, and the toolkit of the four item windows (inventory,
//! vault, merchant, player trade). OWNER: Sonnet 2 (docs/P3_CONTRATO.md); Numic AO look.
//!
//! The inventory is a bag, not a table: sunken slots with big icons, a frame coloured by the
//! kind of object, a doll with the silhouette of every equipment slot, filter tabs, a search
//! that dims what does not match, and "Ordenar" / "Juntar" that reorder the bag with the
//! fewest `SwapSlots` (plan in `client_core::p3::items`).
//!
//! The toolkit (all `pub(crate)` for `bank`, `shop`, `trade`): the glyph art drawn with
//! primitives, the slot (`draw_cell`) and the grids, rich tooltips, buttons, the context
//! menu, the quantity prompt (`QtyPrompt`), drag and drop (what a dragged object does where
//! it is released) and the tracker that turns the server's answer into a toast.
use crate::assets::Assets;
use crate::game::Game;
use crate::render::layout::Layout;
use crate::ui::fx_ui::{ease, key_of, toast, ToastKind};
use crate::ui::theme::{self, FontKind, Surface, BAD, GOLD, GOLD_BRIGHT, GOLD_DIM, GOLD_TEXT, GOOD, INK, MUTED, TEXT, TEXT_DIM, TEXT_FAINT};
use crate::ui::widgets::{self, measure, measure_with, text, text_with, wrap_lines, Kind};
use crate::ui::{Drag, Ui, WindowId};
use client_core::p3::items::{category, equip_slot, primary_action, rarity, sale_value, sort_key, usable_slots, Category, EquipSlot, Primary, Rarity, SortMode};
use client_core::ItemSlot;
use macroquad::prelude::*;
use std::cell::RefCell;
use std::collections::HashMap;

// ---------------------------------------------------------------- sizes

/// Size (in UI units) each item window needs. `ui/frame.rs` (integrator) has the table:
/// `default_size` should return these for Inventory, Bank, Shop and Trade.
#[allow(dead_code)] // until the integrator calls it from `WindowId::default_size` (see the report)
pub fn wanted_size(w: WindowId) -> Option<Vec2> {
    match w {
        WindowId::Inventory => Some(vec2(680.0, 520.0)),
        WindowId::Bank => Some(vec2(780.0, 520.0)),
        WindowId::Shop => Some(vec2(780.0, 520.0)),
        WindowId::Trade => Some(vec2(800.0, 580.0)),
        _ => None,
    }
}

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

fn alpha(c: Color, a: f32) -> Color {
    Color::new(c.r, c.g, c.b, a)
}

fn mix(a: Color, b: Color, t: f32) -> Color {
    Color::new(a.r + (b.r - a.r) * t, a.g + (b.g - a.g) * t, a.b + (b.b - a.b) * t, a.a + (b.a - a.a) * t)
}

pub(crate) fn item_name(assets: &Assets, obj: u16) -> String {
    assets.objs.get(obj as u32).map(|o| o.name.clone()).unwrap_or_else(|| format!("Objeto {obj}"))
}

fn has_backpack(game: &Game, assets: &Assets) -> bool {
    game.local.inventory.iter().flatten().any(|i| i.equipped && assets.objs.get(i.obj as u32).is_some_and(|o| o.obj_type == ao_formats::dat::obj::ObjType::Backpack))
}

fn obj_category(assets: &Assets, obj: u16) -> Option<Category> {
    assets.objs.get(obj as u32).map(|o| category(o.obj_type))
}

// ---------------------------------------------------------------- colours

/// Frame colour of a kind of object.
pub(crate) fn cat_color(c: Category) -> Color {
    match c {
        Category::Weapon => Color::new(0.92, 0.44, 0.35, 1.0),
        Category::Armor => Color::new(0.48, 0.68, 0.94, 1.0),
        Category::Potion => Color::new(0.40, 0.84, 0.60, 1.0),
        Category::Food => Color::new(0.94, 0.68, 0.30, 1.0),
        Category::Material => Color::new(0.80, 0.64, 0.44, 1.0),
        Category::Magic => Color::new(0.74, 0.54, 0.96, 1.0),
        Category::Tool => Color::new(0.84, 0.82, 0.58, 1.0),
        Category::Other => Color::new(0.68, 0.66, 0.62, 1.0),
    }
}

fn cat_glyph(c: Category) -> Glyph {
    match c {
        Category::Weapon => Glyph::Sword,
        Category::Armor => Glyph::Shield,
        Category::Potion => Glyph::Flask,
        Category::Food => Glyph::Apple,
        Category::Material => Glyph::Ore,
        Category::Magic => Glyph::Star,
        Category::Tool => Glyph::Hammer,
        Category::Other => Glyph::Dots,
    }
}

fn rarity_color(r: Rarity) -> Option<Color> {
    match r {
        Rarity::Common => None,
        Rarity::Fine => Some(Color::new(0.46, 0.86, 0.46, 1.0)),
        Rarity::Rare => Some(Color::new(0.36, 0.64, 1.0, 1.0)),
        Rarity::Epic => Some(Color::new(0.78, 0.46, 1.0, 1.0)),
        Rarity::Legendary => Some(Color::new(1.0, 0.64, 0.2, 1.0)),
    }
}

fn slot_glyph(s: EquipSlot) -> Glyph {
    match s {
        EquipSlot::Helmet => Glyph::Helmet,
        EquipSlot::Amulet => Glyph::Amulet,
        EquipSlot::Armor => Glyph::Armor,
        EquipSlot::Ring => Glyph::Ring,
        EquipSlot::Weapon => Glyph::Sword,
        EquipSlot::Shield => Glyph::Shield,
        EquipSlot::Ammo => Glyph::Arrow,
        EquipSlot::Tool => Glyph::Hammer,
        EquipSlot::Pack => Glyph::Pack,
    }
}

// ---------------------------------------------------------------- glyph art (primitives)

/// Small pictures drawn with lines and polygons: kinds of objects, equipment slots, locks...
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Glyph {
    All,
    Sword,
    Shield,
    Helmet,
    Armor,
    Ring,
    Amulet,
    Arrow,
    Hammer,
    Pack,
    Flask,
    Apple,
    Ore,
    Star,
    Dots,
    Coin,
    Lock,
    Unlock,
    Search,
    Check,
    Cross,
}

/// Draws `g` centred on `c`, `s` = half of its size.
pub(crate) fn glyph(g: Glyph, c: Vec2, s: f32, col: Color) {
    let dark = Color::new(0.0, 0.0, 0.0, col.a * 0.45);
    let th = (s * 0.16).max(1.0);
    match g {
        Glyph::All => {
            for (dx, dy) in [(-0.5, -0.5), (0.5, -0.5), (-0.5, 0.5), (0.5, 0.5)] {
                draw_rectangle(c.x + dx * s - s * 0.38, c.y + dy * s - s * 0.38, s * 0.76, s * 0.76, col);
            }
        }
        Glyph::Sword => {
            let t = s * 0.26;
            draw_rectangle(c.x - t / 2.0, c.y - s * 0.68, t, s * 1.15, col);
            draw_triangle(vec2(c.x - t / 2.0, c.y - s * 0.68), vec2(c.x + t / 2.0, c.y - s * 0.68), vec2(c.x, c.y - s), col);
            draw_rectangle(c.x - s * 0.5, c.y + s * 0.4, s, s * 0.17, col);
            draw_rectangle(c.x - t * 0.3, c.y + s * 0.57, t * 0.6, s * 0.3, col);
            draw_circle(c.x, c.y + s * 0.95, s * 0.13, col);
            draw_line(c.x, c.y - s * 0.62, c.x, c.y + s * 0.3, 1.0, dark);
        }
        Glyph::Shield => {
            draw_rectangle(c.x - s * 0.72, c.y - s * 0.82, s * 1.44, s * 0.95, col);
            draw_triangle(vec2(c.x - s * 0.72, c.y + s * 0.13), vec2(c.x + s * 0.72, c.y + s * 0.13), vec2(c.x, c.y + s), col);
            draw_line(c.x, c.y - s * 0.7, c.x, c.y + s * 0.7, th, dark);
            draw_line(c.x - s * 0.55, c.y - s * 0.15, c.x + s * 0.55, c.y - s * 0.15, th, dark);
        }
        Glyph::Helmet => {
            draw_poly(c.x, c.y + s * 0.15, 20, s * 0.85, 0.0, col);
            draw_rectangle(c.x - s * 0.95, c.y + s * 0.15, s * 1.9, s * 0.78, Color::new(0.0, 0.0, 0.0, 0.0));
            draw_rectangle(c.x - s * 0.9, c.y + s * 0.15, s * 1.8, s * 0.3, col);
            draw_rectangle(c.x - s * 0.1, c.y + s * 0.15, s * 0.2, s * 0.6, col);
            draw_line(c.x - s * 0.5, c.y - s * 0.25, c.x + s * 0.5, c.y - s * 0.25, th, dark);
        }
        Glyph::Armor => {
            draw_rectangle(c.x - s * 0.5, c.y - s * 0.55, s, s * 1.5, col);
            draw_rectangle(c.x - s * 0.98, c.y - s * 0.62, s * 0.5, s * 0.7, col);
            draw_rectangle(c.x + s * 0.48, c.y - s * 0.62, s * 0.5, s * 0.7, col);
            draw_circle(c.x, c.y - s * 0.55, s * 0.22, Color::new(0.04, 0.03, 0.02, 0.95));
            draw_line(c.x, c.y - s * 0.3, c.x, c.y + s * 0.9, th, dark);
        }
        Glyph::Ring => {
            draw_circle_lines(c.x, c.y + s * 0.15, s * 0.55, s * 0.24, col);
            draw_triangle(vec2(c.x - s * 0.28, c.y - s * 0.45), vec2(c.x + s * 0.28, c.y - s * 0.45), vec2(c.x, c.y - s * 0.95), col);
            draw_triangle(vec2(c.x - s * 0.28, c.y - s * 0.45), vec2(c.x + s * 0.28, c.y - s * 0.45), vec2(c.x, c.y - s * 0.2), col);
        }
        Glyph::Amulet => {
            draw_line(c.x - s * 0.8, c.y - s * 0.95, c.x, c.y + s * 0.05, th, col);
            draw_line(c.x + s * 0.8, c.y - s * 0.95, c.x, c.y + s * 0.05, th, col);
            draw_circle(c.x, c.y + s * 0.45, s * 0.46, col);
            draw_circle(c.x, c.y + s * 0.45, s * 0.2, Color::new(0.04, 0.03, 0.02, 0.9));
        }
        Glyph::Arrow => {
            draw_line(c.x - s * 0.8, c.y + s * 0.8, c.x + s * 0.5, c.y - s * 0.5, th * 1.2, col);
            draw_triangle(vec2(c.x + s * 0.95, c.y - s * 0.95), vec2(c.x + s * 0.3, c.y - s * 0.62), vec2(c.x + s * 0.62, c.y - s * 0.3), col);
            draw_line(c.x - s * 0.8, c.y + s * 0.8, c.x - s * 0.8, c.y + s * 0.35, th, col);
            draw_line(c.x - s * 0.8, c.y + s * 0.8, c.x - s * 0.35, c.y + s * 0.8, th, col);
        }
        Glyph::Hammer => {
            draw_line(c.x - s * 0.7, c.y + s * 0.85, c.x + s * 0.35, c.y - s * 0.3, th * 1.5, col);
            draw_triangle(vec2(c.x - s * 0.15, c.y - s * 0.7), vec2(c.x + s * 0.95, c.y + s * 0.15), vec2(c.x + s * 0.55, c.y - s * 0.95), col);
            draw_triangle(vec2(c.x - s * 0.15, c.y - s * 0.7), vec2(c.x + s * 0.95, c.y + s * 0.15), vec2(c.x + s * 0.1, c.y + s * 0.15), col);
        }
        Glyph::Pack => {
            draw_rectangle(c.x - s * 0.7, c.y - s * 0.15, s * 1.4, s * 1.05, col);
            draw_poly(c.x, c.y - s * 0.15, 16, s * 0.7, 0.0, col);
            draw_rectangle(c.x - s * 0.7, c.y - s * 0.15, s * 1.4, s * 0.0, col);
            draw_rectangle(c.x - s * 0.2, c.y + s * 0.2, s * 0.4, s * 0.3, dark);
            draw_line(c.x - s * 0.7, c.y + s * 0.05, c.x + s * 0.7, c.y + s * 0.05, th, dark);
        }
        Glyph::Flask => {
            draw_rectangle(c.x - s * 0.2, c.y - s * 0.95, s * 0.4, s * 0.5, col);
            draw_triangle(vec2(c.x - s * 0.2, c.y - s * 0.5), vec2(c.x + s * 0.2, c.y - s * 0.5), vec2(c.x - s * 0.62, c.y + s * 0.15), col);
            draw_triangle(vec2(c.x + s * 0.2, c.y - s * 0.5), vec2(c.x + s * 0.62, c.y + s * 0.15), vec2(c.x - s * 0.62, c.y + s * 0.15), col);
            draw_circle(c.x, c.y + s * 0.35, s * 0.62, col);
            draw_line(c.x - s * 0.34, c.y + s * 0.25, c.x + s * 0.34, c.y + s * 0.25, th, dark);
        }
        Glyph::Apple => {
            draw_circle(c.x - s * 0.25, c.y + s * 0.2, s * 0.58, col);
            draw_circle(c.x + s * 0.25, c.y + s * 0.2, s * 0.58, col);
            draw_line(c.x, c.y - s * 0.3, c.x + s * 0.15, c.y - s * 0.85, th, col);
            draw_triangle(vec2(c.x + s * 0.18, c.y - s * 0.7), vec2(c.x + s * 0.8, c.y - s * 0.75), vec2(c.x + s * 0.4, c.y - s * 0.3), col);
        }
        Glyph::Ore => {
            draw_poly(c.x, c.y + s * 0.05, 6, s * 0.9, 0.0, col);
            draw_line(c.x - s * 0.75, c.y + s * 0.05, c.x, c.y - s * 0.1, th, dark);
            draw_line(c.x + s * 0.75, c.y + s * 0.05, c.x, c.y - s * 0.1, th, dark);
            draw_line(c.x, c.y - s * 0.1, c.x, c.y + s * 0.9, th, dark);
        }
        Glyph::Star => {
            draw_triangle(vec2(c.x, c.y - s), vec2(c.x - s * 0.28, c.y), vec2(c.x + s * 0.28, c.y), col);
            draw_triangle(vec2(c.x, c.y + s), vec2(c.x - s * 0.28, c.y), vec2(c.x + s * 0.28, c.y), col);
            draw_triangle(vec2(c.x - s, c.y), vec2(c.x, c.y - s * 0.28), vec2(c.x, c.y + s * 0.28), col);
            draw_triangle(vec2(c.x + s, c.y), vec2(c.x, c.y - s * 0.28), vec2(c.x, c.y + s * 0.28), col);
        }
        Glyph::Dots => {
            for dx in [-0.6, 0.0, 0.6] {
                draw_circle(c.x + dx * s, c.y, s * 0.2, col);
            }
        }
        Glyph::Coin => {
            draw_circle(c.x, c.y, s, mix(col, Color::new(0.0, 0.0, 0.0, col.a), 0.35));
            draw_circle(c.x, c.y, s * 0.86, col);
            draw_circle_lines(c.x, c.y, s * 0.6, (s * 0.1).max(1.0), Color::new(0.45, 0.30, 0.08, col.a * 0.8));
            draw_circle(c.x - s * 0.3, c.y - s * 0.32, s * 0.2, Color::new(1.0, 1.0, 0.85, col.a * 0.55));
        }
        Glyph::Lock | Glyph::Unlock => {
            let open = g == Glyph::Unlock;
            let w = s * 0.62;
            draw_rectangle(c.x - w, c.y - s * 0.05, w * 2.0, s * 0.95, col);
            let sx = if open { c.x + w * 0.55 } else { c.x };
            draw_circle_lines(sx, c.y - s * 0.3 - if open { s * 0.16 } else { 0.0 }, w * 0.72, th * 1.3, col);
            draw_circle(c.x, c.y + s * 0.38, s * 0.14, Color::new(0.05, 0.03, 0.02, 0.9));
        }
        Glyph::Search => {
            draw_circle_lines(c.x - s * 0.2, c.y - s * 0.2, s * 0.58, th * 1.4, col);
            draw_line(c.x + s * 0.22, c.y + s * 0.22, c.x + s * 0.9, c.y + s * 0.9, th * 1.8, col);
        }
        Glyph::Check => {
            draw_line(c.x - s * 0.7, c.y + s * 0.05, c.x - s * 0.2, c.y + s * 0.55, th * 1.8, col);
            draw_line(c.x - s * 0.2, c.y + s * 0.55, c.x + s * 0.75, c.y - s * 0.55, th * 1.8, col);
        }
        Glyph::Cross => {
            draw_line(c.x - s * 0.6, c.y - s * 0.6, c.x + s * 0.6, c.y + s * 0.6, th * 1.8, col);
            draw_line(c.x + s * 0.6, c.y - s * 0.6, c.x - s * 0.6, c.y + s * 0.6, th * 1.8, col);
        }
    }
}

/// A tray (chest): sunken dark leather with a wooden edge and four rivets.
pub(crate) fn tray(r: Rect, hot: f32, u: f32) {
    draw_rectangle(r.x - 3.0 * u, r.y - 3.0 * u, r.w + 6.0 * u, r.h + 6.0 * u, Color::new(0.20, 0.13, 0.07, 1.0));
    draw_rectangle(r.x - 3.0 * u, r.y - 3.0 * u, r.w + 6.0 * u, 2.0 * u, Color::new(0.36, 0.25, 0.13, 1.0));
    draw_rectangle(r.x, r.y, r.w, r.h, Color::new(0.045, 0.032, 0.022, 1.0));
    theme::inner_vignette(r, 14.0 * u, 0.5);
    for (i, x) in [r.x - 1.5 * u, r.right() + 1.5 * u].into_iter().enumerate() {
        for y in [r.y - 1.5 * u, r.bottom() + 1.5 * u] {
            draw_circle(x, y, 2.4 * u, GOLD_DIM);
            draw_circle(x - 0.5 * u, y - 0.5 * u - i as f32 * 0.0, 1.0 * u, Color::new(1.0, 0.9, 0.6, 0.55));
        }
    }
    draw_rectangle_lines(r.x - 3.0 * u, r.y - 3.0 * u, r.w + 6.0 * u, r.h + 6.0 * u, 1.0, Color::new(0.0, 0.0, 0.0, 0.8));
    if hot > 0.0 {
        draw_rectangle_lines(r.x - 1.0 * u, r.y - 1.0 * u, r.w + 2.0 * u, r.h + 2.0 * u, 2.0 * u, alpha(GOOD, 0.35 + 0.4 * hot));
        draw_rectangle(r.x, r.y, r.w, r.h, alpha(GOOD, 0.05 * hot));
    }
}

// ---------------------------------------------------------------- buttons and chips

/// A button in the Numic look that can be disabled and only reacts when `act` (the mouse is
/// over this window and nothing covers it).
pub(crate) fn ibtn_kind(r: Rect, label: &str, kind: Kind, enabled: bool, act: bool, u: f32) -> bool {
    widgets::button_kind(r, label, kind, enabled, u);
    act && enabled && r.contains(mouse()) && lmb_pressed()
}

/// Secondary button (see `ibtn_kind`).
pub(crate) fn ibtn(r: Rect, label: &str, enabled: bool, act: bool, u: f32) -> bool {
    ibtn_kind(r, label, Kind::Secondary, enabled, act, u)
}

/// Width of a button for `label` at the standard size.
pub(crate) fn btn_w(label: &str, u: f32) -> f32 {
    measure(label, 17.0 * u).width + 24.0 * u
}

/// A small toggle / segment: `on` draws it lit. Returns true when clicked.
pub(crate) fn chip(r: Rect, label: &str, on: bool, act: bool, u: f32) -> bool {
    let hover = act && r.contains(mouse());
    let h = ease(key_of(r), hover || on, 0.08);
    let press = if hover && lmb_down() { u } else { 0.0 };
    let rr = Rect::new(r.x, r.y + press, r.w, r.h);
    draw_rectangle(rr.x, rr.y, rr.w, rr.h, mix(Color::new(0.10, 0.07, 0.045, 1.0), Color::new(0.30, 0.21, 0.09, 1.0), h));
    draw_rectangle(rr.x, rr.y, rr.w, rr.h * 0.5, Color::new(1.0, 0.85, 0.55, 0.04 + 0.05 * h));
    draw_rectangle_lines(rr.x, rr.y, rr.w, rr.h, 1.0, alpha(if on { GOLD_BRIGHT } else { GOLD }, 0.35 + 0.5 * h));
    let size = 15.0 * u;
    let d = measure(label, size);
    text(label, rr.x + (rr.w - d.width) / 2.0, rr.y + rr.h / 2.0 + d.height * 0.42, size, if on { GOLD_BRIGHT } else { mix(TEXT_DIM, TEXT, h) });
    hover && lmb_pressed()
}

// ---------------------------------------------------------------- the slot

/// How a slot is drawn.
#[derive(Clone, Copy, Default)]
pub(crate) struct CellOpts {
    pub selected: bool,
    pub hover: bool,
    /// Does not match the filter / search: shown faded.
    pub dim: bool,
    /// Matches the search: a soft glow.
    pub glow: bool,
    /// Locked slot (no backpack).
    pub locked: bool,
    /// This slot is the one being dragged: shown as a ghost.
    pub ghost: bool,
    /// A dragged object can be dropped here: lit green under the mouse.
    pub drop_ok: bool,
    /// A price badge at the bottom-left (unit sale price; 0: "no compra").
    pub badge: Option<u64>,
    /// Hide the amount (the caller shows it elsewhere).
    pub no_amount: bool,
}

/// One slot: sunken, frame coloured by kind, big icon, amount, equipped seal, rarity gem.
pub(crate) fn draw_cell(assets: &mut Assets, r: Rect, it: Option<ItemSlot>, o: &CellOpts, u: f32) {
    let key = key_of(r);
    let h = ease(key, (o.hover || o.drop_ok && o.hover) && !o.locked, 0.08);
    let (cat, rar, name_ok) = match it.and_then(|i| assets.objs.get(i.obj as u32)) {
        Some(ob) => (Some(category(ob.obj_type)), rarity(ob), true),
        None => (None, Rarity::Common, false),
    };
    theme::surface(r, Surface::Recess, u);
    let accent = cat.map(cat_color);
    if let Some(a) = accent {
        // A tint from the top and a thin lit edge below: it reads as a coloured glass.
        draw_rectangle(r.x, r.y, r.w, r.h * 0.6, alpha(a, 0.09 + 0.07 * h));
        draw_rectangle(r.x, r.bottom() - 2.0 * u, r.w, 2.0 * u, alpha(a, 0.22));
    } else if it.is_some() && !name_ok {
        draw_rectangle(r.x, r.y, r.w, r.h, alpha(BAD, 0.12));
    }
    if let (Some(c), true) = (rarity_color(rar), it.is_some()) {
        let pulse = if rar == Rarity::Legendary { 0.5 + 0.5 * (get_time() as f32 * 2.0).sin() } else { 0.6 };
        draw_rectangle(r.x, r.y, r.w, r.h, alpha(c, 0.07 + 0.06 * pulse));
    }
    if o.glow {
        draw_rectangle(r.x - 2.0 * u, r.y - 2.0 * u, r.w + 4.0 * u, r.h + 4.0 * u, alpha(GOLD_BRIGHT, 0.20));
    }
    // The icon lifts a little under the mouse and sinks when pressed.
    if let Some(i) = it {
        let pad = r.w * 0.10;
        let lift = h * 1.5 * u - if o.hover && lmb_down() { 2.0 * u } else { 0.0 };
        let ir = Rect::new(r.x + pad - lift, r.y + pad - lift, r.w - 2.0 * pad + 2.0 * lift, r.h - 2.0 * pad + 2.0 * lift);
        // Contact shadow: a thin flattened ellipse under the icon, not a disc.
        draw_ellipse(r.x + r.w / 2.0, r.bottom() - r.h * 0.13, r.w * 0.26, r.h * 0.05, 0.0, Color::new(0.0, 0.0, 0.0, 0.35));
        assets.draw_obj(i.obj, ir);
    }
    // Border: the kind's colour (or gold when selected / hovered / receiving a drop).
    let base = accent.map_or(alpha(GOLD_DIM, 0.55), |a| alpha(a, 0.62));
    let hot = if o.drop_ok && o.hover { GOOD } else { GOLD_BRIGHT };
    let c = if o.selected || (o.drop_ok && o.hover) { hot } else { mix(base, alpha(GOLD_BRIGHT, 0.95), h) };
    let t = if o.selected || (o.drop_ok && o.hover) { 2.0 * u } else { 1.0 + 0.5 * h };
    draw_rectangle_lines(r.x, r.y, r.w, r.h, t, c);
    if o.drop_ok && o.hover {
        draw_rectangle(r.x, r.y, r.w, r.h, alpha(GOOD, 0.20));
    }
    if let Some(i) = it {
        if let Some(gc) = rarity_color(rar) {
            let g = Vec2::new(r.right() - 6.0 * u, r.y + 6.0 * u);
            let d = 4.0 * u;
            draw_triangle(g + vec2(0.0, -d), g + vec2(d, 0.0), g + vec2(0.0, d), gc);
            draw_triangle(g + vec2(0.0, -d), g + vec2(-d, 0.0), g + vec2(0.0, d), gc);
            draw_circle(g.x - 0.8 * u, g.y - 0.8 * u, 1.0 * u, Color::new(1.0, 1.0, 1.0, 0.6));
        }
        if i.amount > 1 && !o.no_amount {
            let size = (r.w * 0.27).max(13.0 * u);
            let s = fmt_amount(i.amount);
            let d = measure(&s, size);
            let pw = d.width + 7.0 * u;
            draw_rectangle(r.right() - pw - 2.0 * u, r.bottom() - size - 4.0 * u, pw, size + 2.0 * u, Color::new(0.0, 0.0, 0.0, 0.62));
            text(&s, r.right() - pw - 2.0 * u + 3.5 * u, r.bottom() - 5.0 * u, size, TEXT);
        }
        if i.equipped {
            // A wax seal at the top-left corner: round, gold, with the check cut in it.
            let (cx, cy, rad) = (r.x + 9.0 * u, r.y + 9.0 * u, 7.5 * u);
            draw_circle(cx + 1.0 * u, cy + 1.5 * u, rad, Color::new(0.0, 0.0, 0.0, 0.5));
            draw_circle(cx, cy, rad, Color::new(0.62, 0.44, 0.14, 1.0));
            draw_circle(cx, cy, rad * 0.78, Color::new(0.88, 0.68, 0.28, 1.0));
            glyph(Glyph::Check, vec2(cx, cy), rad * 0.62, Color::new(0.24, 0.14, 0.04, 1.0));
        }
    }
    if let Some(p) = o.badge {
        let (label, col) = if p > 0 { (fmt_amount(p.min(u32::MAX as u64) as u32), GOLD_TEXT) } else { ("—".to_string(), TEXT_FAINT) };
        let size = 12.0 * u;
        let d = measure(&label, size);
        draw_rectangle(r.x + 2.0 * u, r.bottom() - size - 4.0 * u, d.width + 14.0 * u, size + 2.0 * u, Color::new(0.0, 0.0, 0.0, 0.6));
        if p > 0 {
            glyph(Glyph::Coin, vec2(r.x + 8.0 * u, r.bottom() - size * 0.5 - 3.0 * u), 3.4 * u, GOLD);
        }
        text(&label, r.x + 13.0 * u, r.bottom() - 5.0 * u, size, col);
    }
    if o.dim {
        draw_rectangle(r.x, r.y, r.w, r.h, Color::new(0.02, 0.015, 0.01, 0.66));
    }
    if o.ghost {
        draw_rectangle(r.x, r.y, r.w, r.h, Color::new(0.02, 0.015, 0.01, 0.55));
        draw_rectangle_lines(r.x + 2.0 * u, r.y + 2.0 * u, r.w - 4.0 * u, r.h - 4.0 * u, 1.0, alpha(GOLD, 0.6));
    }
    if o.locked {
        draw_rectangle(r.x, r.y, r.w, r.h, Color::new(0.0, 0.0, 0.0, 0.5));
        glyph(Glyph::Lock, vec2(r.x + r.w / 2.0, r.y + r.h / 2.0), r.w * 0.17, alpha(TEXT_FAINT, 0.75));
    }
}

// ---------------------------------------------------------------- rich tooltips

/// Tooltip lines of an object of the inventory / vault / merchant, `extra` hints last. The
/// first line carries the kind (a control char and a digit) so `tooltip_at` paints the name
/// in the colour of its kind.
pub(crate) fn item_tip(game: &Game, assets: &Assets, obj: u16, amount: u32, equipped: bool, extra: &[String]) -> Vec<String> {
    let mut v = match assets.objs.get(obj as u32) {
        Some(o) => {
            let mut t = client_core::p3::items::rich_tooltip(o, amount, equipped, &game.viewer());
            let k = Category::ALL.iter().position(|c| *c == category(o.obj_type)).unwrap_or(7);
            t[0] = format!("\u{1}{k}{}", t[0]);
            t
        }
        None => vec![format!("Objeto {obj}")],
    };
    for e in extra {
        v.push(if e.starts_with(['~', '!', '$', '?']) { e.clone() } else { format!("?{e}") });
    }
    v
}

enum Row {
    Gap(f32),
    Rule,
    Kind(String),
    Flavour(String),
    Data(String, String),
    Plain(String),
    Warn(String),
    Price(String),
    Hint(String),
}

/// Tooltip box near `at`. Conventions of the lines (see `p3::items::rich_tooltip`): the
/// first one is the name (in the colour of its kind when it carries it); `~` flavour text in
/// italics; `!` an unmet requirement (red); `$` the price; `?` a hint; `Label: value` is
/// shown in two columns; anything else is data. Lines without conventions work too.
pub(crate) fn tooltip_at(lines: &[String], at: Vec2, u: f32) {
    if lines.is_empty() {
        return;
    }
    let (cat, title) = match lines[0].strip_prefix('\u{1}') {
        Some(rest) => {
            let idx = rest.chars().next().and_then(|c| c.to_digit(10)).map(|d| d as usize);
            (idx.and_then(|i| Category::ALL.get(i).copied()), rest.get(1..).unwrap_or(""))
        }
        None => (None, lines[0].as_str()),
    };
    let accent = cat.map_or(GOLD_BRIGHT, cat_color);
    let pad = 13.0 * u;
    let max_w = 330.0 * u;
    let icon_w = if cat.is_some() { 34.0 * u } else { 0.0 };
    let (tsz, ksz, fsz, dsz) = (20.0 * u, 15.0 * u, 16.0 * u, 16.0 * u);
    // Gather rows.
    let title_lines = wrap_lines(FontKind::Title, title, tsz, max_w - icon_w);
    let mut rows: Vec<Row> = Vec::new();
    let mut hints: Vec<Row> = Vec::new();
    for (i, l) in lines.iter().enumerate().skip(1) {
        if let Some(t) = l.strip_prefix('~') {
            for w in wrap_lines(FontKind::NarrativeItalic, t, fsz, max_w) {
                rows.push(Row::Flavour(w));
            }
        } else if let Some(t) = l.strip_prefix('!') {
            rows.push(Row::Warn(t.to_string()));
        } else if let Some(t) = l.strip_prefix('$') {
            rows.push(Row::Price(t.to_string()));
        } else if let Some(t) = l.strip_prefix('?') {
            hints.push(Row::Hint(t.to_string()));
        } else if i == 1 && cat.is_some() {
            rows.push(Row::Kind(l.clone()));
        } else if let Some((a, b)) = l.split_once(": ") {
            rows.push(Row::Data(a.to_string(), b.to_string()));
        } else {
            rows.push(Row::Plain(l.clone()));
        }
    }
    // Group: kind, flavour, then a rule and the data, then price, then hints.
    let mut ordered: Vec<Row> = Vec::new();
    let (mut kind, mut flav, mut data, mut warn, mut price) = (Vec::new(), Vec::new(), Vec::new(), Vec::new(), Vec::new());
    for r in rows {
        match r {
            Row::Kind(_) => kind.push(r),
            Row::Flavour(_) => flav.push(r),
            Row::Warn(_) => warn.push(r),
            Row::Price(_) => price.push(r),
            r => data.push(r),
        }
    }
    ordered.extend(kind);
    ordered.extend(flav);
    let any_data = !data.is_empty() || !warn.is_empty();
    if any_data && !ordered.is_empty() {
        ordered.push(Row::Gap(4.0 * u));
        ordered.push(Row::Rule);
        ordered.push(Row::Gap(5.0 * u));
    }
    ordered.extend(data);
    ordered.extend(warn);
    if !price.is_empty() {
        ordered.push(Row::Gap(3.0 * u));
        ordered.extend(price);
    }
    if !hints.is_empty() {
        ordered.push(Row::Gap(5.0 * u));
        ordered.push(Row::Rule);
        ordered.push(Row::Gap(5.0 * u));
        ordered.extend(hints);
    }
    let row_h = |r: &Row| match r {
        Row::Gap(h) => *h,
        Row::Rule => 1.0,
        Row::Kind(_) => ksz * 1.3,
        Row::Flavour(_) => fsz * 1.22,
        Row::Hint(_) => 14.0 * u * 1.3,
        _ => dsz * 1.32,
    };
    // Width of the box.
    let mut w = 210.0 * u;
    for l in &title_lines {
        w = w.max(measure_with(FontKind::Title, l, tsz).width + icon_w + 2.0 * pad);
    }
    for r in &ordered {
        let tw = match r {
            Row::Kind(t) => measure(t, ksz).width,
            Row::Flavour(t) => measure_with(FontKind::NarrativeItalic, t, fsz).width,
            Row::Data(a, b) => measure(a, dsz).width + measure(b, dsz).width + 22.0 * u,
            Row::Plain(t) => measure(t, dsz).width,
            Row::Warn(t) => measure(t, dsz).width + 20.0 * u,
            Row::Price(t) => measure(t, dsz).width + 22.0 * u,
            Row::Hint(t) => measure(t, 14.0 * u).width,
            _ => 0.0,
        };
        w = w.max(tw + 2.0 * pad);
    }
    let w = w.min(max_w + 2.0 * pad);
    let title_h = (title_lines.len() as f32 * tsz * 1.2).max(icon_w) + 6.0 * u;
    let h = pad + title_h + ordered.iter().map(row_h).sum::<f32>() + pad * 0.8;
    let x = (at.x + 20.0 * u).min(screen_width() - w - 6.0).max(4.0);
    let y = (at.y + 20.0 * u).min(screen_height() - h - 6.0).max(4.0);
    let r = Rect::new(x, y, w, h);
    // Box: shadow, dark body with a tint of the kind at the top, metal edge and a kind bar.
    draw_rectangle(r.x + 4.0, r.y + 6.0, r.w, r.h, Color::new(0.0, 0.0, 0.0, 0.4));
    draw_rectangle(r.x, r.y, r.w, r.h, Color::new(0.04, 0.03, 0.022, 0.975));
    draw_rectangle(r.x, r.y, r.w, title_h + pad * 0.6, alpha(accent, 0.10));
    theme::metal_border(r, 1.5 * u);
    draw_rectangle(r.x + 3.0 * u, r.y + 3.0 * u, 3.0 * u, title_h + pad * 0.6 - 3.0 * u, alpha(accent, 0.9));
    let mut cy = r.y + pad;
    if let Some(c) = cat {
        let gc = vec2(r.x + pad + 14.0 * u, cy + 14.0 * u);
        draw_circle(gc.x, gc.y, 15.0 * u, Color::new(0.0, 0.0, 0.0, 0.5));
        draw_circle_lines(gc.x, gc.y, 15.0 * u, 1.5 * u, alpha(accent, 0.8));
        glyph(cat_glyph(c), gc, 8.5 * u, accent);
    }
    for l in &title_lines {
        cy += tsz * 1.2;
        text_with(FontKind::Title, l, r.x + pad + icon_w, cy - tsz * 0.2, tsz, accent);
    }
    cy = r.y + pad + title_h;
    for row in &ordered {
        let rh = row_h(row);
        let base = cy + rh * 0.76;
        match row {
            Row::Gap(_) => {}
            Row::Rule => draw_line(r.x + pad, cy + 0.5, r.right() - pad, cy + 0.5, 1.0, alpha(GOLD, 0.28)),
            Row::Kind(t) => text(t, r.x + pad, base, ksz, alpha(accent, 0.95)),
            Row::Flavour(t) => text_with(FontKind::NarrativeItalic, t, r.x + pad, base, fsz, TEXT_DIM),
            Row::Data(a, b) => {
                text(a, r.x + pad, base, dsz, TEXT_DIM);
                text(b, r.right() - pad - measure(b, dsz).width, base, dsz, TEXT);
            }
            Row::Plain(t) => text(t, r.x + pad, base, dsz, TEXT),
            Row::Warn(t) => {
                let c = vec2(r.x + pad + 6.0 * u, base - dsz * 0.32);
                draw_triangle(c + vec2(0.0, -6.0 * u), c + vec2(6.5 * u, 5.0 * u), c + vec2(-6.5 * u, 5.0 * u), theme::BAD);
                draw_rectangle(c.x - 0.8 * u, c.y - 2.5 * u, 1.6 * u, 4.0 * u, Color::new(0.2, 0.0, 0.0, 1.0));
                text(t, r.x + pad + 18.0 * u, base, dsz, theme::BAD);
            }
            Row::Price(t) => {
                glyph(Glyph::Coin, vec2(r.x + pad + 6.0 * u, base - dsz * 0.32), 6.0 * u, GOLD);
                text(t, r.x + pad + 18.0 * u, base, dsz, GOLD_TEXT);
            }
            Row::Hint(t) => text(t, r.x + pad, base, 14.0 * u, TEXT_FAINT),
        }
        cy += rh;
    }
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

/// Places inside the item windows a dragged object can be released on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Zone {
    InvCell(u8),
    InvArea,
    VaultArea,
    StockArea,
    OfferMine,
    Doll,
}

/// What a quantity prompt does with the number typed.
#[derive(Clone, Copy, Debug)]
pub(crate) enum Pending {
    Drop(u8),
    DropGold,
    Deposit(u8),
    Withdraw(u8),
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

/// A transaction waiting for the server's answer: the window compares gold and object counts
/// with what they were and says the result in a toast (or that nothing happened).
#[derive(Clone, Copy, Debug)]
struct Tx {
    kind: TxKind,
    obj: u16,
    gold0: u32,
    vault0: u32,
    held0: u32,
    at: f64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TxKind {
    Buy,
    Sell,
    Deposit,
    Withdraw,
    DepositGold,
    WithdrawGold,
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
    /// Inventory: kind filter and search text.
    filter: Option<Category>,
    search: String,
    /// Several `SwapSlots` are in flight: the bag in view is old until they arrive.
    sort_until: f64,
    tx: Option<Tx>,
}

/// State of the other three windows that lives between frames (fields they type into, what
/// they compare against).
#[derive(Default)]
pub(crate) struct Aux {
    /// Shop counter: quantity typed, what it was typed for, and which side is in focus.
    pub qty_text: String,
    pub qty_key: Option<(bool, u8, u16)>,
    pub shop_buying: bool,
    /// Bank gold field.
    pub gold_text: String,
    /// Trade: the offers as seen last frame, to say what changed, and the log of changes.
    pub trade_seen: Option<TradeSeen>,
    pub trade_log: Vec<(f64, TradeChange, bool)>,
    /// Trade: when each table last changed (mine, theirs), and when the confirmations were
    /// last wiped by a change.
    pub trade_flash: [f64; 2],
    pub trade_banner: f64,
}

/// One change of an offer, as the log of the trade window tells it (`true` = theirs).
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum TradeChange {
    Added(u16, u32),
    Removed(u16),
    Changed(u16, u32, u32),
    Gold(u32, u32),
    Wiped,
}

/// What the trade window saw last frame.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct TradeSeen {
    pub rev: u32,
    pub mine: client_core::social::trade::Offer,
    pub theirs: client_core::social::trade::Offer,
    pub i_confirmed: bool,
    pub they_confirmed: bool,
    pub i_accepted: bool,
    pub they_accepted: bool,
    pub name: String,
}

thread_local! {
    static ST: RefCell<St> = RefCell::new(St::default());
    static AUX: RefCell<Aux> = RefCell::new(Aux::default());
}

fn st<R>(f: impl FnOnce(&mut St) -> R) -> R {
    ST.with(|s| f(&mut s.borrow_mut()))
}

pub(crate) fn aux<R>(f: impl FnOnce(&mut Aux) -> R) -> R {
    AUX.with(|s| f(&mut s.borrow_mut()))
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

    /// The biggest grid of `cols` x `rows` that fits `w` x `h`, cells at most `max_cell`.
    pub fn fit(cols: usize, rows: usize, w: f32, h: f32, gap: f32, max_cell: f32) -> Grid {
        let cw = (w - (cols as f32 - 1.0) * gap) / cols as f32;
        let ch = (h - (rows as f32 - 1.0) * gap) / rows as f32;
        Grid { cols, rows, cell: cw.min(ch).min(max_cell).floor(), gap }
    }
}

/// What a grid shows.
pub(crate) struct GridView<'a> {
    pub slots: &'a [Option<ItemSlot>],
    pub from: WindowId,
    pub selected: u8,
    /// Slots from this index (0-based) on are drawn as locked.
    pub locked_from: usize,
    /// Only this kind of object is lit (the rest fades).
    pub filter: Option<Category>,
    /// Lowercase search text (empty: none).
    pub query: &'a str,
    /// Show the unit sale price of every object (the merchant window).
    pub sale_badges: bool,
}

impl<'a> GridView<'a> {
    pub fn plain(slots: &'a [Option<ItemSlot>], from: WindowId, selected: u8, locked_from: usize) -> GridView<'a> {
        GridView { slots, from, selected, locked_from, filter: None, query: "", sale_badges: false }
    }
}

/// Whether a dragged object can be released on a pane of `from` (a hint, the real check is in
/// `drop_on`): the bag takes anything from the bag (swap), the vault and the stock.
fn drag_fits(ui: &Ui, pane: WindowId) -> bool {
    match &ui.drag {
        Some(d) => match pane {
            WindowId::Inventory => matches!(d.from, WindowId::Inventory | WindowId::Bank | WindowId::Shop),
            WindowId::Bank => d.from == WindowId::Inventory,
            WindowId::Shop => d.from == WindowId::Inventory,
            _ => false,
        },
        None => false,
    }
}

/// Draws a grid inside a tray. Returns the hit under the mouse (when `act`) and the zones
/// it occupies.
pub(crate) fn draw_grid(ui: &Ui, assets: &mut Assets, g: &Grid, o: Vec2, v: &GridView, act: bool) -> (Option<Hit>, Vec<(Zone, Rect)>) {
    let u = ui.u;
    let m = mouse();
    let hover_i = if act { g.at(o, m) } else { None };
    let pane = if v.from == WindowId::Inventory { WindowId::Inventory } else { v.from };
    let fits = drag_fits(ui, pane);
    let area = g.area(o);
    let in_area = act && fits && area.contains(m);
    tray(Rect::new(area.x - 6.0 * u, area.y - 6.0 * u, area.w + 12.0 * u, area.h + 12.0 * u), ease(key_of(area), in_area, 0.1), u);
    let drag_src = ui.drag.as_ref().filter(|d| d.from == v.from).map(|d| d.index);
    let mut hit = None;
    let mut zones = Vec::new();
    for i in 0..(g.cols * g.rows).min(v.slots.len()) {
        let r = g.rect(o, i);
        let slot = i as u8 + 1;
        let it = v.slots[i];
        let hover = hover_i == Some(i);
        let (mut dim, mut glow) = (false, false);
        if let (Some(item), true) = (it, v.filter.is_some() || !v.query.is_empty()) {
            // Only while filtering or searching: nothing is allocated otherwise.
            let obj = assets.objs.get(item.obj as u32);
            let ok_cat = v.filter.is_none_or(|f| obj.is_some_and(|ob| category(ob.obj_type) == f));
            let ok_q = v.query.is_empty() || obj.is_some_and(|ob| ob.name.to_lowercase().contains(v.query));
            dim = !(ok_cat && ok_q);
            glow = !v.query.is_empty() && ok_q && ok_cat;
        }
        let badge = if v.sale_badges && it.is_some() { it.map(|i| if i.equipped { 0 } else { assets.objs.get(i.obj as u32).map_or(0, |ob| sale_value(ob, 1)) }) } else { None };
        let opts = CellOpts { selected: v.selected == slot && it.is_some(), hover, dim, glow, locked: i >= v.locked_from, ghost: drag_src == Some(slot) && ui.drag.is_some(), drop_ok: fits && i < v.locked_from, badge, no_amount: false };
        draw_cell(assets, r, it, &opts, u);
        if v.from == WindowId::Inventory {
            zones.push((Zone::InvCell(slot), r));
        }
        if hover {
            if let Some(it) = it {
                hit = Some(Hit { from: v.from, slot, obj: it.obj, amount: it.amount });
            }
        }
    }
    zones.push((if v.from == WindowId::Inventory { Zone::InvArea } else { Zone::VaultArea }, area));
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
                begin_tx(game, TxKind::Buy, h.obj);
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

/// "Use": an equipped working tool or an ore stack asks for a world tile (work, P4);
/// anything else goes to the server as `UseItem`.
fn use_item(game: &mut Game, obj: Option<&ao_formats::dat::obj::ObjData>, hit: Hit) {
    use ao_formats::dat::obj::ObjType;
    use client_core::p4::work::{targeting_for_tool, Targeting};
    let equipped = game.local.slot(hit.slot).is_some_and(|s| s.equipped);
    let target = match obj {
        Some(o) if o.obj_type == ObjType::WorkingTools && equipped => targeting_for_tool(o),
        Some(o) if o.obj_type == ObjType::Minerals => Some(Targeting::Smelt { slot: hit.slot }),
        _ => None,
    };
    match target {
        Some(t) => {
            game.local.p4.work.pending = Some(t);
            game.say("Haz clic en el lugar donde quieres trabajar.", ao_protocol::Tone::Info);
        }
        None => {
            game.inv_use(hit.slot);
        }
    }
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
            _ => use_item(game, obj, hit),
        },
        Op::Use => use_item(game, obj, hit),
        Op::Equip => {
            game.inv_equip(slot);
        }
        Op::Drop => ask(game, win, Pending::Drop(slot), "Tirar al suelo", hit.obj, hit.amount, false, None),
        Op::DropWorld => ask(game, win, Pending::Drop(slot), "Tirar al suelo", hit.obj, hit.amount, true, None),
        Op::Deposit => ask(game, win, Pending::Deposit(slot), "Guardar en la bóveda", hit.obj, hit.amount, false, None),
        Op::Withdraw => {
            let n = game.bank_item(slot).map_or(0, |i| i.amount);
            ask(game, win, Pending::Withdraw(slot), "Retirar de la bóveda", hit.obj, n, false, None)
        }
        Op::Buy => {
            let max = game.shop_max_buy(slot);
            if max == 0 {
                game.say("No tienes oro suficiente.", ao_protocol::Tone::Warn);
                toast("No te alcanza el oro", ToastKind::Bad);
                return;
            }
            let (value, skill) = (obj.map_or(0, |o| o.value), game.local.skills.get(ao_rules::trade::SKILL_COMMERCE).copied().unwrap_or(0));
            let total: Box<dyn Fn(u32) -> u64> = Box::new(move |n| ao_rules::trade::buy_price(value, skill, n));
            ask(game, win, Pending::Buy(slot), "Comprar", hit.obj, max, false, Some(total))
        }
        Op::Sell => {
            let Some(o) = obj else { return };
            if o.no_tradeable_to_npc || o.newbie != 0 {
                game.say("El comerciante no compra eso.", ao_protocol::Tone::Warn);
                toast("El comerciante no compra eso", ToastKind::Warn);
                return;
            }
            let o = o.clone();
            let total: Box<dyn Fn(u32) -> u64> = Box::new(move |n| sale_value(&o, n));
            ask(game, win, Pending::Sell(slot), "Vender", hit.obj, hit.amount, false, Some(total))
        }
        Op::Offer => {
            let max = game.offerable(slot);
            ask(game, win, Pending::Offer(slot), "Ofrecer", hit.obj, max, false, None)
        }
    }
}

/// Opens a quantity prompt, or acts at once when there is a single unit to move.
#[allow(clippy::too_many_arguments)]
fn ask(game: &mut Game, win: WindowId, pending: Pending, title: &str, obj: u16, max: u32, force: bool, total: Option<Box<dyn Fn(u32) -> u64>>) {
    if max == 0 {
        game.say("No hay nada que mover.", ao_protocol::Tone::Warn);
        return;
    }
    if max == 1 && !force {
        return apply(game, pending, 1);
    }
    open_prompt(win, pending, title, max, 1, total);
    st(|s| {
        if let Some(p) = s.prompts.get_mut(&win) {
            p.q.obj = Some(obj);
        }
    });
}

pub(crate) fn open_prompt(win: WindowId, pending: Pending, title: &str, max: u32, default: u32, total: Option<Box<dyn Fn(u32) -> u64>>) {
    st(|s| {
        s.prompts.insert(win, Prompt { pending, q: QtyPrompt { title: title.to_string(), max, text: String::new(), default, total, obj: None, focused: false } });
    });
}

/// Starts watching a transaction: what changes next is its answer.
fn begin_tx(game: &Game, kind: TxKind, obj: u16) {
    let tx = Tx { kind, obj, gold0: game.local.gold, vault0: game.local.bank.as_ref().map_or(0, |b| b.gold), held0: game.local.count_of(obj), at: get_time() };
    st(|s| s.tx = Some(tx));
}

fn apply(game: &mut Game, p: Pending, n: u32) {
    let obj_of_bag = |g: &Game, s: u8| g.local.slot(s).map_or(0, |i| i.obj);
    match p {
        Pending::Deposit(s) => begin_tx(game, TxKind::Deposit, obj_of_bag(game, s)),
        Pending::Withdraw(s) => begin_tx(game, TxKind::Withdraw, game.bank_item(s).map_or(0, |i| i.obj)),
        Pending::Buy(s) => begin_tx(game, TxKind::Buy, game.local.social.trade.shop.as_ref().and_then(|sh| sh.items.get(&s)).map_or(0, |i| i.obj)),
        Pending::Sell(s) => begin_tx(game, TxKind::Sell, obj_of_bag(game, s)),
        _ => {}
    }
    match p {
        Pending::Drop(s) => game.inv_drop(s, n),
        Pending::DropGold => game.inv_drop_gold(n),
        Pending::Deposit(s) => game.bank_deposit(s, n),
        Pending::Withdraw(s) => game.bank_withdraw(s, n),
        Pending::Buy(s) => game.shop_buy(s, n),
        Pending::Sell(s) => game.shop_sell(s, n),
        Pending::Offer(s) => game.trade_offer(s, n),
        Pending::OfferGold => game.trade_offer_gold(n),
    };
}

/// Starts watching a transaction made by a window with its own buttons (shop stepper, vault
/// gold field): same as the prompts do.
pub(crate) fn watch_buy(game: &Game, obj: u16) {
    begin_tx(game, TxKind::Buy, obj);
}

pub(crate) fn watch_sell(game: &Game, obj: u16) {
    begin_tx(game, TxKind::Sell, obj);
}

pub(crate) fn watch_gold(game: &Game, deposit: bool) {
    begin_tx(game, if deposit { TxKind::DepositGold } else { TxKind::WithdrawGold }, 0);
}

/// Turns the answer of the server to the last transaction into a toast: it compares gold and
/// counts with those of the start. Silence for 1.5 s means the server refused it.
pub(crate) fn settle_tx(game: &Game, assets: &Assets) {
    let Some(tx) = st(|s| s.tx) else { return };
    let gold = game.local.gold;
    let held = game.local.count_of(tx.obj);
    let vault = game.local.bank.as_ref().map_or(tx.vault0, |b| b.gold);
    let name = item_name(assets, tx.obj);
    let more = |a: u32, b: u32| a.abs_diff(b);
    let done: Option<(String, ToastKind)> = match tx.kind {
        TxKind::Buy if held > tx.held0 && gold < tx.gold0 => Some((format!("Compraste {}× {} por {} oro", more(held, tx.held0), name, fmt_gold(more(gold, tx.gold0) as u64)), ToastKind::Good)),
        TxKind::Sell if held < tx.held0 && gold > tx.gold0 => Some((format!("Vendiste {}× {} por {} oro", more(held, tx.held0), name, fmt_gold(more(gold, tx.gold0) as u64)), ToastKind::Good)),
        TxKind::Deposit if held < tx.held0 => Some((format!("Guardaste {}× {} en la bóveda", more(held, tx.held0), name), ToastKind::Good)),
        TxKind::Withdraw if held > tx.held0 => Some((format!("Retiraste {}× {}", more(held, tx.held0), name), ToastKind::Good)),
        TxKind::DepositGold if vault > tx.vault0 => Some((format!("Guardaste {} oro", fmt_gold(more(vault, tx.vault0) as u64)), ToastKind::Good)),
        TxKind::WithdrawGold if gold > tx.gold0 => Some((format!("Retiraste {} oro", fmt_gold(more(gold, tx.gold0) as u64)), ToastKind::Good)),
        _ if get_time() - tx.at > 1.5 => Some((
            match tx.kind {
                TxKind::Buy => "El comerciante no pudo venderte eso".to_string(),
                TxKind::Sell => "El comerciante no quiso comprarlo".to_string(),
                _ => "La bóveda no aceptó el movimiento".to_string(),
            },
            ToastKind::Bad,
        )),
        _ => None,
    };
    if let Some((text, kind)) = done {
        st(|s| s.tx = None);
        toast(text, kind);
    }
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
    /// Object the prompt is about (its icon is drawn in the box).
    pub obj: Option<u16>,
    focused: bool,
}

impl QtyPrompt {
    pub fn value(&self) -> u32 {
        client_core::p3::items::parse_qty(&self.text, self.max).unwrap_or_else(|| self.default.clamp(1, self.max.max(1)))
    }

    /// Draws the prompt centred in `area` (dimming it) and returns what happened.
    pub fn show(&mut self, ui: &mut Ui, assets: &mut Assets, area: Rect, id: &str) -> QtyEvent {
        let u = ui.u;
        draw_rectangle(area.x - 6.0 * u, area.y - 6.0 * u, area.w + 12.0 * u, area.h + 12.0 * u, Color::new(0.0, 0.0, 0.0, 0.55));
        let (bw, bh) = (360.0 * u, 184.0 * u);
        let b = Rect::new((area.x + (area.w - bw) / 2.0).round(), (area.y + (area.h - bh) / 2.0).round(), bw, bh);
        widgets::panel(b, u);
        // Icon of the object and the title.
        let ic = Rect::new(b.x + 16.0 * u, b.y + 16.0 * u, 48.0 * u, 48.0 * u);
        draw_cell(assets, ic, self.obj.map(|o| ItemSlot { obj: o, amount: 1, equipped: false }), &CellOpts::default(), u);
        widgets::text_with(FontKind::Title, &self.title, ic.right() + 14.0 * u, b.y + 38.0 * u, 22.0 * u, GOLD_BRIGHT);
        if let Some(o) = self.obj {
            widgets::text_fit(&item_name(assets, o), ic.right() + 14.0 * u, b.y + 58.0 * u, 16.0 * u, b.w - 100.0 * u, TEXT_DIM);
        }
        // The prompt is modal: the field keeps the keyboard even after a click elsewhere.
        if ui.focus.as_deref() != Some(id) {
            ui.focus = Some(id.to_string());
        }
        self.focused = true;
        ui.keyboard_captured = true;
        ui.mouse_captured = true;
        let field = Rect::new(b.x + 16.0 * u, b.y + 76.0 * u, 110.0 * u, 34.0 * u);
        let mut enter = widgets::text_field(ui, id, &mut self.text, field, 9, false, |c| c.is_ascii_digit());
        if self.text.is_empty() {
            text(&self.default.to_string(), field.x + 10.0 * u, field.y + field.h / 2.0 + 7.0 * u, 20.0 * u, TEXT_FAINT);
        }
        let mut bx = field.right() + 10.0 * u;
        for (label, v) in [("1", 1u32), ("10", 10), ("100", 100), ("Todo", self.max)] {
            let w = measure(label, 15.0 * u).width.max(20.0 * u) + 18.0 * u;
            if chip(Rect::new(bx, field.y + 3.0 * u, w, field.h - 6.0 * u), label, false, true, u) {
                self.text = v.min(self.max).to_string();
            }
            bx += w + 5.0 * u;
        }
        text(&format!("Máximo {}", fmt_gold(self.max as u64)), b.x + 16.0 * u, b.y + 130.0 * u, 15.0 * u, TEXT_FAINT);
        if let Some(f) = &self.total {
            let s = fmt_gold(f(self.value()));
            let d = widgets::measure_with(FontKind::Title, &s, 24.0 * u);
            widgets::text_with(FontKind::Title, &s, b.right() - 16.0 * u - d.width, b.y + 134.0 * u, 24.0 * u, GOLD_TEXT);
            glyph(Glyph::Coin, vec2(b.right() - 16.0 * u - d.width - 14.0 * u, b.y + 127.0 * u), 8.0 * u, GOLD);
        }
        if is_key_pressed(KeyCode::Up) {
            self.text = (self.value() + 1).min(self.max).to_string();
        }
        if is_key_pressed(KeyCode::Down) {
            self.text = self.value().saturating_sub(1).max(1).to_string();
        }
        enter |= is_key_pressed(KeyCode::Enter) || is_key_pressed(KeyCode::KpEnter);
        let (ow, nw) = (110.0 * u, 100.0 * u);
        let y = b.bottom() - 40.0 * u;
        let ok = ibtn_kind(Rect::new(b.right() - 16.0 * u - ow, y, ow, 30.0 * u), "Aceptar", Kind::Primary, true, true, u);
        let no = ibtn_kind(Rect::new(b.right() - 16.0 * u - ow - 8.0 * u - nw, y, nw, 30.0 * u), "Cancelar", Kind::Ghost, true, true, u);
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
pub(crate) fn prompt_ui(ui: &mut Ui, game: &mut Game, assets: &mut Assets, w: WindowId, area: Rect) {
    let Some(mut p) = st(|s| s.prompts.remove(&w)) else { return };
    match p.q.show(ui, assets, area, &format!("qty_{w:?}")) {
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
    let (w, row) = (210.0 * u, 30.0 * u);
    let h = opts.len() as f32 * row + 10.0 * u;
    let r = Rect::new(menu.pos.x.min(screen_width() - w - 4.0), menu.pos.y.min(screen_height() - h - 4.0), w, h);
    draw_rectangle(r.x + 4.0 * u, r.y + 5.0 * u, r.w, r.h, Color::new(0.0, 0.0, 0.0, 0.45));
    draw_rectangle(r.x, r.y, r.w, r.h, Color::new(0.06, 0.045, 0.03, 0.985));
    theme::metal_border(r, 1.5 * u);
    ui.capture_mouse_if_over(r);
    let m = mouse();
    let mut chosen = None;
    for (i, (label, op)) in opts.iter().enumerate() {
        let rr = Rect::new(r.x + 5.0 * u, r.y + 5.0 * u + i as f32 * row, r.w - 10.0 * u, row);
        let hover = rr.contains(m);
        let hv = ease(key_of(rr), hover, 0.07);
        if hv > 0.0 {
            draw_rectangle(rr.x, rr.y, rr.w, rr.h, Color::new(0.34, 0.23, 0.10, hv));
            draw_rectangle(rr.x, rr.y, 3.0 * u, rr.h, alpha(GOLD_BRIGHT, hv));
        }
        text(label, rr.x + 12.0 * u, rr.y + rr.h / 2.0 + 5.5 * u, 17.0 * u, mix(TEXT, WHITE, hv));
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
    let open: Vec<WindowId> = MINE.iter().copied().filter(|w| ui.is_open(*w)).collect();
    let stale = |s: &St, w: WindowId| !open.contains(&w) || now - s.last_draw.get(&w).copied().unwrap_or(-10.0) > 0.25;
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
    // The text fields of the item windows give the keyboard back when their window is gone.
    let gone = |w: WindowId| st(|s| stale(s, w));
    if ui.focus.as_deref() == Some("inv_search") && gone(WindowId::Inventory) {
        ui.focus = None;
    }
    if ui.focus.as_deref() == Some("bank_gold") && gone(WindowId::Bank) {
        ui.focus = None;
    }
    if ui.focus.as_deref() == Some("shop_qty") && gone(WindowId::Shop) {
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
            (WindowId::Inventory, Some(Zone::Doll)) => {
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
    let s = 54.0 * u;
    // Held by its lower right corner, so the slot under the mouse stays in view.
    let r = Rect::new(m.x - s * 0.72, m.y - s * 0.78, s, s);
    // A soft shadow below it says it is lifted.
    draw_circle(r.x + s * 0.62, r.y + s * 1.12, s * 0.42, Color::new(0.0, 0.0, 0.0, 0.28));
    let cat = obj_category(assets, d.what).map(cat_color).unwrap_or(GOLD_TEXT);
    draw_rectangle(r.x, r.y, r.w, r.h, Color::new(0.06, 0.045, 0.03, 0.88));
    draw_rectangle(r.x, r.y, r.w, r.h * 0.6, alpha(cat, 0.13));
    draw_rectangle_lines(r.x, r.y, r.w, r.h, 2.0 * u, alpha(cat, 0.95));
    assets.draw_obj(d.what, Rect::new(r.x + 4.0 * u, r.y + 4.0 * u, r.w - 8.0 * u, r.h - 8.0 * u));
    if d.amount > 1 {
        let t = fmt_amount(d.amount);
        let dm = measure(&t, 14.0 * u);
        draw_rectangle(r.right() - dm.width - 9.0 * u, r.bottom() - 18.0 * u, dm.width + 7.0 * u, 16.0 * u, Color::new(0.0, 0.0, 0.0, 0.65));
        text(&t, r.right() - dm.width - 5.5 * u, r.bottom() - 5.0 * u, 14.0 * u, TEXT);
    }
    if d.from == WindowId::Inventory && zone_at(ui, m).is_none() && over_world(m) {
        let label = "Soltar para tirar";
        let dm = widgets::measure_with(FontKind::Title, label, 16.0 * u);
        let lr = Rect::new(r.right() + 8.0 * u, r.y + r.h / 2.0 - 13.0 * u, dm.width + 18.0 * u, 26.0 * u);
        draw_rectangle(lr.x, lr.y, lr.w, lr.h, Color::new(0.07, 0.05, 0.03, 0.92));
        draw_rectangle_lines(lr.x, lr.y, lr.w, lr.h, 1.0, alpha(GOLD, 0.8));
        widgets::text_with(FontKind::Title, label, lr.x + 9.0 * u, lr.y + lr.h / 2.0 + 5.5 * u, 16.0 * u, GOLD_TEXT);
    }
}

// ---------------------------------------------------------------- the window

/// Slot positions of the doll: (slot, column 0|1, row).
const DOLL: [(EquipSlot, usize, usize); 8] = [
    (EquipSlot::Helmet, 0, 0),
    (EquipSlot::Amulet, 0, 1),
    (EquipSlot::Armor, 0, 2),
    (EquipSlot::Ring, 0, 3),
    (EquipSlot::Weapon, 1, 0),
    (EquipSlot::Shield, 1, 1),
    (EquipSlot::Ammo, 1, 2),
    (EquipSlot::Tool, 1, 3),
];

/// The silhouette of a character behind the doll slots: a rune circle, a body in the dark.
/// The player's own character (real body, head and worn equipment, the same composition as
/// in the world) standing on a lit pedestal in `area`, turning slowly through its four
/// headings. Drawn through a camera so the 32 px sprites scale up crisply.
fn draw_figure(game: &Game, assets: &mut Assets, area: Rect, u: f32) {
    let t = get_time() as f32;
    let foot = vec2(area.x + area.w / 2.0, area.bottom() - 18.0 * u);
    // Pedestal: warm light from above and a soft pool on the floor.
    for i in 0..6 {
        let k = i as f32 / 6.0;
        draw_ellipse(foot.x, foot.y, area.w * (0.46 - k * 0.3), 16.0 * u * (1.0 - k * 0.5), 0.0, Color::new(0.95, 0.70, 0.35, 0.035));
    }
    let beam_top = area.y + 8.0 * u;
    for i in 0..10 {
        let k = i as f32 / 10.0;
        let w = area.w * (0.18 + 0.32 * k);
        draw_rectangle(foot.x - w / 2.0, beam_top + (foot.y - beam_top) * k, w, (foot.y - beam_top) / 10.0, Color::new(1.0, 0.82, 0.5, 0.012 + 0.012 * k));
    }
    draw_ellipse(foot.x, foot.y, area.w * 0.30, 9.0 * u, 0.0, Color::new(0.0, 0.0, 0.0, 0.45));
    draw_ellipse_lines(foot.x, foot.y, area.w * 0.36, 11.0 * u, 0.0, 1.2 * u, alpha(GOLD, 0.35));

    let Some(me) = game.me() else { return };
    let mut e = me.clone();
    // South, West, North, East: a slow turn, holding each side 2 s.
    e.dir = [ao_rules::Dir::Down, ao_rules::Dir::Left, ao_rules::Dir::Up, ao_rules::Dir::Right][((t / 2.0) as usize) % 4];
    // The character is drawn at 1:1 into a small off-screen target (the same technique as
    // the world renderer), then that texture is scaled up with whole pixels onto the stand.
    // Room for a character with a tall helmet and a wide weapon, little else.
    const RT_W: f32 = 72.0;
    const RT_H: f32 = 66.0;
    thread_local! {
        static RT: std::cell::RefCell<Option<RenderTarget>> = const { std::cell::RefCell::new(None) };
    }
    let rt = RT.with(|c| {
        c.borrow_mut()
            .get_or_insert_with(|| {
                let rt = render_target(RT_W as u32, RT_H as u32);
                rt.texture.set_filter(FilterMode::Nearest);
                rt
            })
            .clone()
    });
    // In the target, the character's tile sits so its feet land at (RT_W/2, RT_H - 8).
    let tl = e.tile_tl();
    let tile_at = vec2(RT_W / 2.0 - 16.0, RT_H - 8.0 - 30.0);
    let origin = vec2(tl.x, tl.y) - tile_at;
    set_camera(&Camera2D { zoom: vec2(2.0 / RT_W, 2.0 / RT_H), target: vec2(RT_W / 2.0, RT_H / 2.0), render_target: Some(rt.clone()), ..Default::default() });
    clear_background(Color::new(0.0, 0.0, 0.0, 0.0));
    crate::render::characters::draw_character(assets, &e, origin, t * 1000.0);
    set_default_camera();
    // Whole-pixel scale that fits the area; feet on the stand, a slow breath.
    let scale = ((area.h - 24.0 * u) / RT_H).floor().max(1.0);
    let (w, h) = (RT_W * scale, RT_H * scale);
    let breath = ((t * 1.6).sin() * 0.5 + 0.5) * scale * 0.5;
    let dest = vec2((foot.x - w / 2.0).round(), (foot.y + 8.0 * scale - h - breath).round());
    draw_texture_ex(&rt.texture, dest.x, dest.y, WHITE, DrawTextureParams { dest_size: Some(vec2(w, h)), ..Default::default() });
}

/// Draws the content of the window inside `r` (the frame is already drawn).
pub fn draw(ui: &mut Ui, game: &mut Game, assets: &mut Assets, r: Rect) {
    let u = ui.u;
    let w = WindowId::Inventory;
    begin(ui, game, assets);
    settle_tx(game, assets);
    let act = active(ui, w);
    let m = mouse();
    let mut zones: Vec<(Zone, Rect)> = Vec::new();
    let mut tip: Option<Vec<String>> = None;
    let bp = has_backpack(game, assets);
    let usable = usable_slots(bp);
    let inv = game.local.inventory.clone();
    let left_w = 236.0 * u;
    let rx = r.x + left_w + 18.0 * u;
    let rw = r.right() - rx;

    // ================= left: the doll, gold, capacity =================
    let cell = 50.0 * u;
    let rowh = 56.0 * u;
    let doll = Rect::new(r.x, r.y, left_w, 4.0 * rowh + cell + 4.0 * u);
    zones.push((Zone::Doll, doll));
    // Your character, between the two columns of equipment slots.
    let fig = Rect::new(r.x + cell + 8.0 * u, r.y, left_w - 2.0 * cell - 16.0 * u, 4.0 * rowh);
    draw_figure(game, assets, fig, u);
    let mut worn: HashMap<EquipSlot, (u8, ItemSlot)> = HashMap::new();
    for (i, it) in inv.iter().enumerate() {
        let Some(it) = it.filter(|i| i.equipped) else { continue };
        if let Some(sl) = assets.objs.get(it.obj as u32).and_then(|o| equip_slot(o.obj_type)) {
            worn.entry(sl).or_insert((i as u8 + 1, it));
        }
    }
    let dragged_slot = ui.drag.as_ref().filter(|d| d.from == WindowId::Inventory).and_then(|d| assets.objs.get(d.what as u32).and_then(|o| equip_slot(o.obj_type)));
    let mut cells: Vec<(EquipSlot, Rect)> = DOLL.iter().map(|(s, c, row)| (*s, Rect::new(if *c == 0 { r.x + 2.0 * u } else { r.x + left_w - cell - 2.0 * u }, r.y + *row as f32 * rowh, cell, cell))).collect();
    cells.push((EquipSlot::Pack, Rect::new(r.x + (left_w - cell) / 2.0, r.y + 4.0 * rowh + 4.0 * u, cell, cell)));
    for (sl, cr) in &cells {
        let hover = act && cr.contains(m);
        let item = worn.get(sl).map(|(_, i)| *i);
        let want = dragged_slot == Some(*sl);
        let opts = CellOpts { hover, drop_ok: want, dim: dragged_slot.is_some() && !want && item.is_none(), ..Default::default() };
        draw_cell(assets, *cr, item, &opts, u);
        if item.is_none() {
            glyph(slot_glyph(*sl), vec2(cr.x + cr.w / 2.0, cr.y + cr.h / 2.0), cr.w * 0.26, alpha(GOLD, if want { 0.9 } else { 0.30 }));
        }
        if hover {
            match worn.get(sl) {
                Some((slot, it)) => {
                    tip = Some(item_tip(game, assets, it.obj, 1, true, &["Clic: quitarlo".to_string()]));
                    if lmb_pressed() {
                        game.inv_equip(*slot);
                    }
                }
                None => tip = Some(vec![sl.label().to_string(), "?Nada equipado".to_string()]),
            }
        }
    }
    // Gold: the protagonist of the left column.
    let gy = doll.bottom() + 16.0 * u;
    theme::divider(r.x + left_w / 2.0, gy - 8.0 * u, left_w, u);
    glyph(Glyph::Coin, vec2(r.x + 18.0 * u, gy + 22.0 * u), 15.0 * u, GOLD_BRIGHT);
    let gs = fmt_gold(game.local.gold as u64);
    widgets::text_with(FontKind::Title, &gs, r.x + 42.0 * u, gy + 31.0 * u, 28.0 * u, GOLD_BRIGHT);
    let bw = (left_w - 4.0 * u) / 2.0;
    let by = gy + 46.0 * u;
    if ibtn(Rect::new(r.x, by, bw, 28.0 * u), "Tirar oro", game.local.gold > 0, act, u) {
        open_prompt(w, Pending::DropGold, "Tirar oro al suelo", game.local.gold, game.local.gold.min(1), None);
    }
    let here = game.me().is_some_and(|me| game.local.ground.contains_key(&(me.x, me.y)));
    if ibtn(Rect::new(r.x + bw + 4.0 * u, by, bw, 28.0 * u), "Recoger", here, act, u) {
        game.inv_pick_up();
    }
    // Capacity.
    let used = inv.iter().take(usable).filter(|i| i.is_some()).count();
    let cy = by + 42.0 * u;
    glyph(Glyph::Pack, vec2(r.x + 14.0 * u, cy + 6.0 * u), 10.0 * u, GOLD);
    let full = used >= usable;
    widgets::text_with(FontKind::Title, &format!("{used} / {usable}"), r.x + 32.0 * u, cy + 12.0 * u, 18.0 * u, if full { theme::BAD } else { TEXT });
    let hint = if full { "Bolsa llena" } else if bp { "Mochila grande" } else { "Bolsa pequeña" };
    let hd = measure(hint, 14.0 * u);
    text(hint, r.x + left_w - hd.width, cy + 12.0 * u, 14.0 * u, TEXT_FAINT);
    let trk = Rect::new(r.x, cy + 20.0 * u, left_w, 6.0 * u);
    draw_rectangle(trk.x, trk.y, trk.w, trk.h, Color::new(0.0, 0.0, 0.0, 0.6));
    let fill = used as f32 / usable.max(1) as f32;
    let col = if full { theme::BAD } else if fill > 0.8 { theme::WARN } else { GOLD };
    draw_rectangle(trk.x, trk.y, trk.w * fill, trk.h, col);
    draw_rectangle(trk.x, trk.y, trk.w * fill, trk.h * 0.45, Color::new(1.0, 1.0, 1.0, 0.18));
    draw_rectangle_lines(trk.x, trk.y, trk.w, trk.h, 1.0, alpha(GOLD_DIM, 0.6));

    // ================= right: tabs, search, sort, the 42 slots =================
    let filter = st(|s| s.filter);
    let tab = 32.0 * u;
    let tab_gap = 4.0 * u;
    let mut counts = [0usize; 8];
    for i in inv.iter().take(INV_MAX).flatten() {
        if let Some(c) = obj_category(assets, i.obj) {
            counts[Category::ALL.iter().position(|x| *x == c).unwrap_or(7)] += 1;
        }
    }
    for k in 0..=Category::ALL.len() {
        let cat = k.checked_sub(1).map(|i| Category::ALL[i]);
        let tr = Rect::new(rx + k as f32 * (tab + tab_gap), r.y, tab, tab);
        let on = filter == cat;
        let hover = act && tr.contains(m);
        let h = ease(key_of(tr), hover || on, 0.08);
        draw_rectangle(tr.x, tr.y, tr.w, tr.h, mix(Color::new(0.09, 0.065, 0.04, 1.0), Color::new(0.28, 0.19, 0.08, 1.0), h));
        draw_rectangle(tr.x, tr.y, tr.w, tr.h * 0.5, Color::new(1.0, 0.85, 0.55, 0.03 + 0.05 * h));
        let tint = cat.map_or(GOLD_TEXT, cat_color);
        draw_rectangle_lines(tr.x, tr.y, tr.w, tr.h, 1.0, alpha(if on { tint } else { GOLD_DIM }, 0.4 + 0.5 * h));
        if on {
            draw_rectangle(tr.x + 3.0 * u, tr.bottom() - 3.0 * u, tr.w - 6.0 * u, 2.0 * u, tint);
        }
        let press = if hover && lmb_down() { u } else { 0.0 };
        glyph(cat.map_or(Glyph::All, cat_glyph), vec2(tr.x + tr.w / 2.0, tr.y + tr.h / 2.0 - 1.0 * u + press), tr.w * 0.27, alpha(tint, 0.55 + 0.45 * h));
        if let Some(c) = cat {
            let n = counts[Category::ALL.iter().position(|x| *x == c).unwrap_or(7)];
            if n > 0 {
                text(&n.to_string(), tr.right() - 10.0 * u, tr.y + 11.0 * u, 11.0 * u, alpha(tint, 0.9));
            }
        }
        if hover {
            tip = Some(match cat {
                Some(c) => vec![c.label().to_string(), format!("{} en la bolsa", counts[Category::ALL.iter().position(|x| *x == c).unwrap_or(7)])],
                None => vec!["Todo".to_string(), format!("{used} objetos")],
            });
            if lmb_pressed() {
                st(|s| s.filter = cat);
            }
        }
    }
    // Search field + sort buttons.
    let ry = r.y + tab + 8.0 * u;
    let rowh2 = 30.0 * u;
    let labels = [("Tipo", SortMode::Type), ("Nombre", SortMode::Name), ("Valor", SortMode::Value)];
    let mut x = r.right();
    let merge_label = "Juntar";
    let mut btns: Vec<(Rect, &str, Option<SortMode>)> = Vec::new();
    let mw = btn_w(merge_label, u);
    x -= mw;
    btns.push((Rect::new(x, ry, mw, rowh2), merge_label, None));
    for (lab, mode) in labels.iter().rev() {
        let bw = btn_w(lab, u) - 4.0 * u;
        x -= bw + 4.0 * u;
        btns.push((Rect::new(x, ry, bw, rowh2), lab, Some(*mode)));
    }
    let sort_lab = "Ordenar";
    let sl_w = measure(sort_lab, 15.0 * u).width;
    let sl_x = x - 8.0 * u - sl_w;
    text(sort_lab, sl_x, ry + rowh2 / 2.0 + 5.0 * u, 15.0 * u, TEXT_FAINT);
    let search_r = Rect::new(rx + 22.0 * u, ry, (sl_x - 12.0 * u) - (rx + 22.0 * u), rowh2);
    glyph(Glyph::Search, vec2(rx + 8.0 * u, ry + rowh2 / 2.0), 7.0 * u, alpha(GOLD, 0.8));
    let mut q = st(|s| std::mem::take(&mut s.search));
    let enter = widgets::text_field(ui, "inv_search", &mut q, search_r, 24, false, |c| !c.is_control());
    if q.is_empty() && ui.focus.as_deref() != Some("inv_search") {
        widgets::placeholder(search_r, "Buscar…", u);
    }
    // The field gives the keyboard back (so the character can walk) on Enter, Esc, and a
    // click anywhere else in the window.
    let clicked_elsewhere = lmb_pressed() && !search_r.contains(m) && ui.focus.as_deref() == Some("inv_search");
    if ui.focus.as_deref() == Some("inv_search") && (enter || is_key_pressed(KeyCode::Escape) || clicked_elsewhere) {
        ui.focus = None;
    }
    if !q.is_empty() {
        let xr = Rect::new(search_r.right() - 22.0 * u, search_r.y + 4.0 * u, 18.0 * u, search_r.h - 8.0 * u);
        let hv = act && xr.contains(m);
        glyph(Glyph::Cross, vec2(xr.x + xr.w / 2.0, xr.y + xr.h / 2.0), 5.0 * u, if hv { GOLD_BRIGHT } else { TEXT_DIM });
        if hv && lmb_pressed() {
            q.clear();
            ui.focus = None;
        }
    }
    st(|s| s.search = q.clone());
    let locked_out = get_time() < st(|s| s.sort_until);
    let mut sent: Option<(usize, bool)> = None;
    for (rect, lab, mode) in &btns {
        let hover_hint = act && rect.contains(m);
        if hover_hint {
            tip = Some(match mode {
                Some(SortMode::Type) => vec!["Ordenar por tipo".to_string(), "?Armas, armaduras, joyas, pociones, comida y materiales, juntando las pilas iguales.".to_string()],
                Some(SortMode::Name) => vec!["Ordenar por nombre".to_string(), "?De la A a la Z, juntando las pilas iguales.".to_string()],
                Some(SortMode::Value) => vec!["Ordenar por valor".to_string(), "?Lo más valioso primero, juntando las pilas iguales.".to_string()],
                None => vec!["Juntar pilas".to_string(), "?Une las pilas del mismo objeto sin mover lo demás.".to_string()],
            });
        }
        if chip(*rect, lab, false, act && !locked_out, u) {
            let key = |o: u16| sort_key(o, assets.objs.get(o as u32));
            let n = match mode {
                Some(md) => game.inv_organize(usable, *md, &key, true),
                None => game.inv_merge(),
            };
            sent = Some((n, mode.is_none()));
        }
    }
    if let Some((n, merge_only)) = sent {
        if n == 0 {
            toast(if merge_only { "No hay pilas que juntar" } else { "Tu bolsa ya está ordenada" }, ToastKind::Info);
        } else {
            toast(if merge_only { format!("Pilas juntadas ({n} movimientos)") } else { format!("Bolsa ordenada ({n} movimientos)") }, ToastKind::Good);
            st(|s| s.sort_until = get_time() + 0.6);
        }
    }
    // The grid.
    let gtop = ry + rowh2 + 14.0 * u;
    let avail_h = r.bottom() - 34.0 * u - gtop - 12.0 * u - if bp { 0.0 } else { 22.0 * u };
    // Only the usable slots: 6 per row (24 = 4 rows with the small bag, 42 = 7 with a
    // backpack). The locked ones are a single hint line, not a wall of padlocks.
    let grid = Grid::fit(6, usable.div_ceil(6), rw - 12.0 * u, avail_h, 5.0 * u, 60.0 * u);
    let go = vec2(rx + 6.0 * u + (rw - 12.0 * u - grid.size().x) / 2.0, gtop + 6.0 * u);
    let query = q.to_lowercase();
    let view = GridView { slots: &inv, from: WindowId::Inventory, selected: selected_inv(), locked_from: usable, filter, query: &query, sale_badges: false };
    let (hit, z) = draw_grid(ui, assets, &grid, go, &view, act);
    zones.extend(z);
    if !bp {
        let hint = "Equipa una mochila para llevar 18 objetos más";
        let y = go.y + grid.size().y + 22.0 * u;
        let hw = widgets::measure_with(FontKind::NarrativeItalic, hint, 15.0 * u).width;
        glyph(Glyph::Pack, vec2(rx + (rw - hw) / 2.0 - 12.0 * u, y - 5.0 * u), 8.0 * u, alpha(GOLD, 0.55));
        widgets::text_with(FontKind::NarrativeItalic, hint, rx + (rw - hw) / 2.0, y, 15.0 * u, TEXT_FAINT);
    }
    if let (Some(h), None) = (hit, &ui.drag) {
        let eq = game.local.slot(h.slot).is_some_and(|i| i.equipped);
        let mut hints = Vec::new();
        hints.push(if assets.objs.get(h.obj as u32).is_some_and(|o| primary_action(o.obj_type) == Primary::Equip) { if eq { "Doble clic: quitarlo" } else { "Doble clic: equiparlo" } } else { "Doble clic: usarlo" }.to_string());
        hints.push("Arrastra para mover · clic derecho: más".to_string());
        tip = Some(item_tip(game, assets, h.obj, h.amount, eq, &hints));
    }
    if let Some(ev) = track(ui, w, act, hit) {
        handle(game, assets, w, ev);
    }

    // ---- trade: invitation, or the players close enough to ask ----
    let row = Rect::new(rx, r.bottom() - 26.0 * u, rw, 26.0 * u);
    crate::ui::trade::inventory_row(ui, game, row, act);

    register(w, r, u, zones);
    menu_ui(ui, game, assets, w);
    prompt_ui(ui, game, assets, w, r);
    if let Some(t) = tip {
        if ui.drag.is_none() && !modal(w) {
            tooltip_at(&t, m, u);
        }
    }
    let _ = (GOLD_DIM, INK, MUTED);
}

const INV_MAX: usize = client_core::local::INVENTORY_SLOTS;

// ---------------------------------------------------------------- screenshots

/// Scripted scene for the screenshots (`AO_ITEMS_DEMO=inv|bank|shop|trade|trade_peer`, with
/// `AO_DEMO_TILE=x,y` of the banker / merchant to click). It talks to the real server: gives
/// objects with `/dar`, equips, opens the NPC. Nothing happens without the variable.
/// Extras: `AO_DEMO_SORT=tipo|nombre|valor|juntar` presses "Ordenar" at the end,
/// `AO_DEMO_FILTER=armas|...` and `AO_DEMO_SEARCH=text` set the filter / search.
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
    // Objects to hand out: sensible ones of each kind in obj.dat (`nth` skips the first ones).
    let pick = |ty: ao_formats::dat::obj::ObjType, nth: usize, f: &dyn Fn(&ao_formats::dat::obj::ObjData) -> bool| -> Option<u16> {
        let free = |o: &ao_formats::dat::obj::ObjData| o.forbidden_classes.is_empty() && o.forbidden_races.is_empty() && !o.male_only && !o.female_only && o.royal == 0 && o.chaos == 0;
        assets.objs.iter().filter(|(_, o)| o.obj_type == ty && o.grh > 0 && !o.name.is_empty() && free(o) && f(o)).nth(nth).map(|(i, _)| i as u16)
    };
    use ao_formats::dat::obj::ObjType as T;
    let free_w = |o: &ao_formats::dat::obj::ObjData| o.weapon_anim > 0 && o.min_level <= 1 && o.value > 0;
    let free_a = |o: &ao_formats::dat::obj::ObjData| o.min_level <= 1 && o.value > 0 && o.max_def > 0;
    // The first twelve are enough for the other windows (the chat limits how fast `/dar` goes).
    let gifts: Vec<(u16, u32)> = [
        (pick(T::Weapon, 0, &|o| free_w(o) && o.two_handed == 0), 1),
        (pick(T::Potions, 0, &|o| o.value > 0), 42),
        (pick(T::Armor, 0, &free_a), 1),
        (pick(T::UseOnce, 0, &|o| o.min_hunger > 0), 8),
        (pick(T::Shield, 0, &|o| o.min_level <= 1 && o.value > 0), 1),
        (pick(T::Helmet, 0, &|o| o.min_level <= 1 && o.value > 0), 1),
        (pick(T::Drinks, 0, &|_| true), 5),
        (pick(T::Arrows, 0, &|o| o.value > 0), 60),
        (pick(T::Minerals, 0, &|_| true), 14),
        (pick(T::Wood, 0, &|_| true), 20),
        (pick(T::RingAccessory, 0, &|o| o.value > 0), 1),
        (pick(T::Amulets, 0, &|o| o.value > 0), 1),
        (pick(T::WorkingTools, 0, &|_| true), 1),
        (pick(T::Weapon, 6, &|o| free_w(o) && o.min_level > 5 && o.value > 1500), 1),
        (pick(T::Armor, 5, &|o| o.value > 2000), 1),
        (pick(T::Weapon, 1, &free_w), 1),
        (pick(T::Armor, 1, &free_a), 1),
        (pick(T::Potions, 3, &|o| o.value > 0), 6),
        (pick(T::Potions, 1, &|o| o.value > 0), 9),
        (pick(T::Weapon, 4, &|o| o.weapon_anim > 0 && o.value > 5000), 1),
        (pick(T::BlacksmithMaterial, 0, &|_| true), 6),
        (Some(12), 10_000),
    ]
    .into_iter()
    .filter_map(|(o, n)| o.map(|o| (o, n)))
    .collect();
    let gifts: Vec<(u16, u32)> = if mode == "inv" { gifts } else { gifts.into_iter().take(7).chain(std::iter::once((12u16, 10_000u32))).collect() };
    let slot_of = |g: &Game, ty: T| g.local.inventory.iter().position(|i| i.is_some_and(|i| assets.objs.get(i.obj as u32).is_some_and(|o| o.obj_type == ty))).map(|p| p as u8 + 1);
    let when = |k: &str| std::env::var(k).ok().and_then(|v| v.parse::<f64>().ok());
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
            // `AO_DEMO_WIPE=<t>`: changes the gold after both confirmed (wipes the seals).
            4 if when("AO_DEMO_WIPE").is_some_and(|w| t > w) && game.local.social.trade.player.is_some() => {
                game.trade_offer_gold(1700);
                advance();
            }
            // `AO_DEMO_ACCEPT=1`: confirms again and accepts, to see the deal close.
            4 if std::env::var("AO_DEMO_ACCEPT").is_ok() && game.plan_accept().is_ok() => {
                game.trade_accept();
                advance();
            }
            _ => {}
        }
        return;
    }
    let n = gifts.len() as u32;
    // A deal done for the toast, at the second given: `AO_DEMO_BUY=14` buys one of the first
    // stock row, `AO_DEMO_SELL1=14` sells one potion.
    thread_local! {
        static DEALT: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    }
    if !DEALT.with(|d| d.get()) && game.local.social.trade.shop.is_some() {
        if when("AO_DEMO_BUY").is_some_and(|b| t > b) {
            DEALT.with(|d| d.set(true));            if let Some((slot, it)) = game.local.social.trade.shop.as_ref().and_then(|s| s.items.iter().next().map(|(k, v)| (*k, *v))) {
                watch_buy(game, it.obj);
                game.shop_buy(slot, 1);
            }
        }
        if when("AO_DEMO_SELL1").is_some_and(|b| t > b) {
            DEALT.with(|d| d.set(true));
            if let Some(sl) = slot_of(game, T::Potions) {
                let obj = game.local.slot(sl).map_or(0, |i| i.obj);
                watch_sell(game, obj);
                game.shop_sell(sl, 1);
            }
        }
    }
    if step > n && mode != "inv" && [WindowId::Bank, WindowId::Shop, WindowId::Trade].iter().any(|w| ui.is_open(*w)) {
        ui.close(WindowId::Inventory);
    }
    match step {
        s if s < n && gap > 0.62 && t > 1.0 + std::env::var("AO_DEMO_DELAY").ok().and_then(|d| d.parse::<f64>().ok()).unwrap_or(0.0) => {
            let (o, c) = gifts[s as usize];
            say(game, format!("/dar {o} {c}"));
            advance();
        }
        s if s == n && gap > 1.0 => {
            for ty in [T::Weapon, T::Armor, T::Shield, T::Helmet, T::RingAccessory, T::Arrows, T::Amulets] {
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
                for ty in [T::Potions, T::UseOnce, T::Arrows, T::Minerals, T::Wood, T::Drinks] {
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
            let loose = slot_of(game, T::Potions).unwrap_or(5);
            if mode == "shop" {
                set_selected_other(WindowId::Shop, 2);
                st(|s| s.sel_inv = loose);
                aux(|a| a.shop_buying = std::env::var("AO_DEMO_SELL").is_err());
            }
            if mode == "bank" {
                set_selected_other(WindowId::Bank, 1);
                st(|s| s.sel_inv = loose);
                if let Ok(g) = std::env::var("AO_DEMO_GOLD") {
                    aux(|a| a.gold_text = g);
                }
            }
            advance();
        }
        s if s == n + 4 => {
            // Filter, search and sort, if asked.
            if let Ok(f) = std::env::var("AO_DEMO_FILTER") {
                let cat = Category::ALL.iter().copied().find(|c| c.label().to_lowercase().starts_with(&f.to_lowercase()));
                st(|s| s.filter = cat);
            }
            if let Ok(q) = std::env::var("AO_DEMO_SEARCH") {
                st(|s| s.search = q);
            }
            if let Ok(kind) = std::env::var("AO_DEMO_SORT") {
                let usable = usable_slots(has_backpack(game, assets));
                let key = |o: u16| sort_key(o, assets.objs.get(o as u32));
                match kind.as_str() {
                    "tipo" => game.inv_organize(usable, SortMode::Type, &key, true),
                    "nombre" => game.inv_organize(usable, SortMode::Name, &key, true),
                    "valor" => game.inv_organize(usable, SortMode::Value, &key, true),
                    _ => game.inv_merge(),
                };
            }
            advance();
        }
        s if s == n + 5 && gap > 2.0 => {
            if std::env::var("AO_DEMO_DUMP").is_ok() {
                for (i, it) in game.local.inventory.iter().enumerate() {
                    if let Some(it) = it {
                        let o = assets.objs.get(it.obj as u32);
                        eprintln!("slot {:2}: {:>5} x{:<5} {:?} {} {}", i + 1, it.obj, it.amount, o.map(|o| o.obj_type), o.map_or("?", |o| o.name.as_str()), if it.equipped { "(E)" } else { "" });
                    }
                }
            }
            // Mouse on a worn object (inv), a vault slot (bank), a stock row (shop).
            if let Some(c) = std::env::var("AO_DEMO_HOVER").ok().and_then(|s| s.parse::<u8>().ok()).and_then(|s| cell_center(WindowId::Inventory, s)) {
                set_demo_mouse(Some(c));
            }
            if let Ok(v) = std::env::var("AO_DEMO_MOUSE") {
                let p: Vec<f32> = v.split(',').filter_map(|x| x.trim().parse().ok()).collect();
                if p.len() == 2 {
                    set_demo_mouse(Some(vec2(p[0], p[1])));
                }
            }
            advance();
        }
        s if s == n + 6 && gap > 0.5 => {
            // Optional: the quantity prompt or the context menu open on the potions.
            if let Some(it) = game.local.slot(5) {
                if std::env::var("AO_DEMO_PROMPT").is_ok() {
                    let total: Box<dyn Fn(u32) -> u64> = Box::new(|n| n as u64 * 10);
                    open_prompt(WindowId::Inventory, Pending::Drop(5), "Tirar al suelo", it.amount, 1, Some(total));
                    st(|s| {
                        s.prompts.get_mut(&WindowId::Inventory).map(|p| {
                            p.q.text = "7".into();
                            p.q.obj = Some(it.obj);
                        })
                    });
                }
                if std::env::var("AO_DEMO_MENU").is_ok() {
                    let hit = Hit { from: WindowId::Inventory, slot: 5, obj: it.obj, amount: it.amount };
                    st(|s| s.menu = Some(Menu { win: WindowId::Inventory, hit, pos: mouse(), fresh: false }));
                }
            }
            advance();
        }
        s if s == n + 8 && gap > 1.0 && std::env::var("AO_DEMO_ACCEPT").is_ok() && game.plan_accept().is_ok() => {
            game.trade_accept();
            advance();
        }
        s if s == n + 7 && gap > 0.5 => {
            // Scripted mouse through the real input path: `swap` (drag slot to slot), `world`
            // (drag to the world), `dbl` (double click), `menu` (right click, pick an entry),
            // `dragging` (a drag held over a slot, for the drop preview).
            if let Ok(kind) = std::env::var("AO_DEMO_INPUT") {
                let c = |slot: u8| cell_center(WindowId::Inventory, slot);
                let (u, t) = (ui.u, now);
                let script: Vec<(f64, Act)> = match (kind.as_str(), c(5)) {
                    ("swap", Some(a)) => match c(12) {
                        Some(b) => vec![(t, Act::Move(a)), (t + 0.1, Act::Down), (t + 0.2, Act::Move((a + b) / 2.0)), (t + 0.3, Act::Move(b)), (t + 0.5, Act::Up)],
                        None => vec![],
                    },
                    ("dragging", Some(a)) => match c(19) {
                        Some(b) => vec![(t, Act::Move(a)), (t + 0.1, Act::Down), (t + 0.2, Act::Move((a + b) / 2.0)), (t + 0.3, Act::Move(b))],
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

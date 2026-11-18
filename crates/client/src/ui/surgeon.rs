//! Window: the surgeon (TSAO `FrmCirujia`). The faces of your race and gender, front view,
//! on parchment tiles; the current one sealed, the chosen one framed in gold.
use crate::assets::Assets;
use crate::game::Game;
use crate::ui::character::kit::{self, Icon, INK_DIM, WAX};
use crate::ui::theme::{FontKind, GOLD_BRIGHT};
use crate::ui::widgets::{button_kind, Kind};
use crate::ui::{Ui, WindowId};
use macroquad::prelude::*;
use std::cell::Cell;

thread_local! {
    static PICK: Cell<u16> = const { Cell::new(0) };
}

pub fn draw(ui: &mut Ui, game: &mut Game, assets: &mut Assets, r: Rect) {
    let u = ui.u;
    kit::begin(ui, WindowId::Surgeon);
    let Some((_, heads, price, current)) = game.local.p4.surgeon.open.clone() else { return };
    // Only faces with graphics in this client.
    let heads: Vec<u16> = heads.into_iter().filter(|h| assets.db.heads.contains_key(h)).collect();
    let y0 = kit::section("Elige tu nueva cara", r.x + 6.0 * u, r.y, r.w - 12.0 * u, u);
    let cell = 64.0 * u;
    let cols = ((r.w - 12.0 * u) / (cell + 8.0 * u)).floor().max(1.0) as usize;
    let mut pick = PICK.with(Cell::get);
    for (i, h) in heads.iter().enumerate() {
        let (cx, cy) = (i % cols, i / cols);
        let c = Rect::new(r.x + 6.0 * u + cx as f32 * (cell + 8.0 * u), y0 + 6.0 * u + cy as f32 * (cell + 8.0 * u), cell, cell);
        if c.bottom() > r.bottom() - 50.0 * u {
            break;
        }
        kit::page(c, u);
        if let Some(g) = assets.db.heads.get(h).map(|v| v[3]) {
            assets.draw(g, c.x + (cell - 32.0) / 2.0, c.y + (cell - 32.0) / 2.0 - 4.0 * u, false, 0.0, WHITE);
        }
        if *h == current {
            kit::ink(FontKind::Body, "actual", c.x + 6.0 * u, c.bottom() - 5.0 * u, 12.0 * u, WAX);
        }
        if *h == pick {
            draw_rectangle_lines(c.x - 2.0, c.y - 2.0, c.w + 4.0, c.h + 4.0, 2.0 * u, GOLD_BRIGHT);
        }
        if c.contains(mouse_position().into()) && is_mouse_button_pressed(MouseButton::Left) {
            pick = *h;
        }
    }
    PICK.with(|p| p.set(pick));
    let by = r.bottom() - 38.0 * u;
    kit::icon(Icon::Coin, r.x + 18.0 * u, by + 16.0 * u, 16.0 * u, Color::new(0.75, 0.52, 0.08, 1.0));
    kit::ink(FontKind::Body, &format!("{} de oro", kit::thousands(price as u64)), r.x + 32.0 * u, by + 21.0 * u, 15.0 * u, INK_DIM);
    let ok = pick != 0 && pick != current && game.local.gold >= price;
    if button_kind(Rect::new(r.right() - 160.0 * u, by, 150.0 * u, 32.0 * u), "Operar", Kind::Primary, ok, u) {
        game.surgeon_change(pick);
    }
}

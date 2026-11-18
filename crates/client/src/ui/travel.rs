//! Window: a transporter's trips (TSAO `frmViajar`). One parchment card per destination
//! with its price and level; the ones you cannot take yet say why instead of a button.
use crate::assets::Assets;
use crate::game::Game;
use crate::ui::character::kit::{self, Icon, INK_DIM, WAX};
use crate::ui::theme::{FontKind, INK};
use crate::ui::widgets::{button_kind, Kind};
use crate::ui::{Ui, WindowId};
use macroquad::prelude::*;

pub fn draw(ui: &mut Ui, game: &mut Game, assets: &mut Assets, r: Rect) {
    let _ = assets;
    let u = ui.u;
    kit::begin(ui, WindowId::Travel);
    let Some((_, name, dests)) = game.local.p4.travel.open.clone() else { return };
    let (level, gold) = (game.stats.level, game.local.gold);
    let mut y = kit::section(&name, r.x + 6.0 * u, r.y, r.w - 12.0 * u, u);
    let card_h = 58.0 * u;
    for (i, d) in dests.iter().enumerate() {
        let card = Rect::new(r.x + 6.0 * u, y, r.w - 12.0 * u, card_h);
        kit::page(card, u);
        kit::ink_fit(FontKind::Title, &d.name, card.x + 16.0 * u, card.y + 26.0 * u, 19.0 * u, card.w - 150.0 * u, WAX);
        kit::icon(Icon::Coin, card.x + 24.0 * u, card.y + 42.0 * u, 14.0 * u, Color::new(0.75, 0.52, 0.08, 1.0));
        let lvl = if d.min_level > 0 { format!(" · desde nivel {}", d.min_level) } else { String::new() };
        kit::ink(FontKind::Body, &format!("{} de oro{lvl}", kit::thousands(d.price as u64)), card.x + 38.0 * u, card.y + 47.0 * u, 14.0 * u, INK_DIM);
        let b = Rect::new(card.right() - 120.0 * u, card.y + 13.0 * u, 106.0 * u, 32.0 * u);
        if level < d.min_level {
            kit::ink(FontKind::Body, &format!("Nivel {}", d.min_level), b.x + 18.0 * u, b.y + 21.0 * u, 15.0 * u, INK);
        } else if gold < d.price {
            kit::ink(FontKind::Body, "Sin oro", b.x + 24.0 * u, b.y + 21.0 * u, 15.0 * u, INK);
        } else if button_kind(b, "Viajar", Kind::Primary, true, u) {
            game.travel_go(i as u8);
        }
        y += card_h + 8.0 * u;
    }
}

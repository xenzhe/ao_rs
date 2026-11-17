//! Window: events: duels, tournaments, invasions, castles. OWNER: Opus D (docs/P4_CONTRATO.md).
use crate::assets::Assets;
use crate::game::Game;
use crate::ui::Ui;
use macroquad::prelude::*;

pub fn draw(ui: &mut Ui, game: &mut Game, assets: &mut Assets, r: Rect) {
    let _ = (game, assets);
    crate::ui::widgets::text("(en construcción)", r.x, r.y + 20.0 * ui.u, 18.0 * ui.u, crate::ui::theme::MUTED);
}

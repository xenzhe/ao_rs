//! Window: inventory and equipment (drag & drop, use, equip, drop). OWNER: Sonnet 2 (docs/P3_CONTRATO.md).
use crate::assets::Assets;
use crate::game::Game;
use crate::ui::Ui;
use macroquad::prelude::*;

/// Draws the content of the window inside `r` (the frame is already drawn).
pub fn draw(ui: &mut Ui, game: &mut Game, assets: &mut Assets, r: Rect) {
    let _ = (game, assets);
    crate::ui::widgets::text("(en construcción)", r.x, r.y + 20.0 * ui.u, 18.0 * ui.u, crate::ui::theme::MUTED);
}

/// Icon of what is being dragged, under the cursor (drawn last, over every window).
pub fn draw_drag(ui: &mut Ui, assets: &mut Assets, d: &crate::ui::Drag) {
    let _ = (ui, assets, d);
}

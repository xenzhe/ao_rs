//! User interface (docs/P3_CONTRATO.md). The base (`frame`: window manager and shared UI
//! state, `theme`, `widgets`) is kept by the integrator; each window and screen is a file
//! with one owner.
pub mod frame;
pub mod theme;
pub mod widgets;

// Screens
pub mod account;
pub mod login;
// HUD pieces
pub mod console;
pub mod hud;
// Windows
pub mod inventory;
pub mod bank;
pub mod shop;
pub mod trade;
pub mod spells;
pub mod character;
pub mod guild;
pub mod party;
pub mod quests;
pub mod worldmap;
pub mod options;

pub use frame::{Drag, Ui, WindowId};

use crate::assets::Assets;
use crate::game::Game;
use crate::render::layout::Layout;

/// In-game UI for this frame, before the world input: HUD, console, then the open windows
/// back to front, and the drag icon on top.
pub fn frame(ui: &mut Ui, game: &mut Game, assets: &mut Assets, lay: &Layout, hour: f32, pixel_perfect: bool) {
    ui.begin_frame(lay.u);
    auto_windows(ui, game);
    hud::draw(ui, game, assets, lay, hour, pixel_perfect);
    console::draw(ui, game, lay);
    for w in ui.open_windows() {
        let Some(r) = ui.window(w) else { continue };
        match w {
            WindowId::Inventory => inventory::draw(ui, game, assets, r),
            WindowId::Bank => bank::draw(ui, game, assets, r),
            WindowId::Shop => shop::draw(ui, game, assets, r),
            WindowId::Trade => trade::draw(ui, game, assets, r),
            WindowId::Spells => spells::draw(ui, game, assets, r),
            WindowId::Character => character::draw(ui, game, assets, r),
            WindowId::Guild => guild::draw(ui, game, assets, r),
            WindowId::Party => party::draw(ui, game, assets, r),
            WindowId::Quests => quests::draw(ui, game, assets, r),
            WindowId::WorldMap => worldmap::draw(ui, game, assets, r),
            WindowId::Options => options::draw(ui, game, assets, r),
        }
    }
    if let Some(d) = ui.drag.clone() {
        inventory::draw_drag(ui, assets, &d);
    }
}

/// Windows the server opens and closes (banker, merchant, player trade).
fn auto_windows(ui: &mut Ui, game: &Game) {
    let l = &game.local;
    for (w, on) in [(WindowId::Bank, l.bank.is_some()), (WindowId::Shop, l.social.trade.shop.is_some()), (WindowId::Trade, l.social.trade.player.is_some())] {
        if on && !ui.is_open(w) {
            ui.open(w);
        } else if !on && ui.is_open(w) {
            ui.close(w);
        }
    }
}

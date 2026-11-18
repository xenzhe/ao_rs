//! Argentum-style client on the real AO20 maps and graphics, at modern resolutions.
//!
//! Layout:
//! This crate is only the frontend; the game logic (state, server messages, prediction,
//! network, obfuscated stats) lives in `client-core`, shared with the test bots.
//! - [`game`]: glue with `client-core` (colours, core events to particles/textures)
//! - [`render`]: world, lighting, effects, screen layout
//! - [`ui`]: HUD and login screens
//! - [`input`], [`assets`], [`config`]
//! - [`devtools`]: env-driven automation for tests and screenshots
mod assets;
mod audio;
mod secret;
mod config;
mod devtools;
mod game;
mod input;
mod keys;
mod render;
mod ui;

use assets::Assets;
use config::ClientConfig;
use devtools::DevTools;
use game::{Game, Screen};
use macroquad::prelude::*;
use render::layout::Layout;
use render::Renderer;

fn window_conf() -> Conf {
    let cfg = ClientConfig::load();
    Conf { window_title: "Numic AO".into(), window_width: cfg.video.width, window_height: cfg.video.height, window_resizable: true, ..Default::default() }
}

#[macroquad::main(window_conf)]
async fn main() {
    let cfg = ClientConfig::load();
    clear_background(BLACK);
    draw_text("Numic AO - cargando...", 40.0, 60.0, 30.0, Color::new(0.92, 0.76, 0.44, 1.0));
    next_frame().await;

    let mut assets = Assets::load(cfg.recursos_path());
    let mut dev = DevTools::from_env();
    let fixed_hour = std::env::var("AO_HOUR").is_ok();
    let hour = std::env::var("AO_HOUR").ok().and_then(|h| h.parse().ok()).unwrap_or(12.0);
    let mut renderer = Renderer::new(cfg.video.pixel_perfect, hour);
    let mut ui = ui::Ui::new(ui::theme::Theme::load(&assets.root));
    let mut game = Game::new(DevTools::default_name().unwrap_or_default(), cfg.server.clone(), assets.root.clone());
    if dev.auto_login {
        if game.name_input.is_empty() {
            game.name_input = "Tester".into();
        }
        game.connect();
    }

    loop {
        let dt = get_frame_time().min(0.1);
        let now = get_time();
        let t_ms = (now * 1000.0) as f32;
        let lay = Layout::compute(renderer.pixel_perfect);
        clear_background(Color::new(0.05, 0.04, 0.03, 1.0));

        game.now = now;
        if !fixed_hour {
            renderer.world_clock = game.world_time;
        }
        game::poll(&mut game, &mut assets, &mut renderer.fx);
        // Window hotkeys only in the world and never while typing in the chat.
        ui.in_game = game.screen == Screen::Game && game.chat.is_none();
        match &game.screen {
            // Account screens: scenes of the world (ui/scene.rs) under the account UI.
            Screen::Login => {
                ui.begin_frame(lay.u);
                ui::login::draw(&mut ui, &mut game, &mut assets, &mut renderer, dt);
            }
            Screen::Characters => {
                ui.begin_frame(lay.u);
                ui::account::characters(&mut ui, &mut game, &mut assets, &mut renderer, dt);
            }
            Screen::Create => {
                ui.begin_frame(lay.u);
                ui::account::create(&mut ui, &mut game, &mut assets, &mut renderer, dt);
            }
            // Kicked before entering the world (bad password, old client): the login flow
            // takes it back to the login screen with the reason.
            Screen::Kicked(_) if !game.entered_world() => {
                ui.begin_frame(lay.u);
                ui::login::draw(&mut ui, &mut game, &mut assets, &mut renderer, dt);
            }
            Screen::Kicked(reason) => {
                ui::hud::kicked(&reason.clone(), lay.u);
                if dev.shot.is_some() {
                    break;
                }
            }
            Screen::Game => {
                game.check_integrity();
                game.update(dt);
                renderer.fx.update(dt);
                if game.map.is_some() {
                    renderer.draw_world(&game, &mut assets, &lay, t_ms, dt);
                }
                // UI on top of the world, then the world input with what the UI left.
                let hour = renderer.hour();
                ui::frame(&mut ui, &mut game, &mut assets, &lay, hour, renderer.pixel_perfect);
                input::update(&mut game, &mut renderer, &lay, now, &ui, &assets);
                dev.update(&mut game, &mut renderer, &mut ui, now);
                ui::fx_ui::draw_toasts(lay.u);
                if dev.maybe_capture(now) {
                    break;
                }
            }
        }
        if game.screen != Screen::Game {
            ui::fx_ui::draw_toasts(lay.u);
        }
        next_frame().await;
    }
}

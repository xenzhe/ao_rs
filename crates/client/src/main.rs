//! Argentum-style client on the real AO20 maps and graphics, at modern resolutions.
//!
//! Layout:
//! - [`game`]: client state, server messages, player actions (mirroring server rules)
//! - [`render`]: world, lighting, effects, screen layout
//! - [`ui`]: HUD and login screens
//! - [`input`], [`net`], [`assets`], [`config`], [`guard`] (obfuscated stats)
//! - [`devtools`]: env-driven automation for tests and screenshots
mod assets;
mod config;
mod devtools;
mod game;
mod guard;
mod input;
mod net;
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
    Conf { window_title: "Argentum (Rust)".into(), window_width: cfg.video.width, window_height: cfg.video.height, window_resizable: true, ..Default::default() }
}

#[macroquad::main(window_conf)]
async fn main() {
    let cfg = ClientConfig::load();
    clear_background(BLACK);
    draw_text("Cargando recursos de Argentum Online...", 40.0, 60.0, 30.0, WHITE);
    next_frame().await;

    let mut assets = Assets::load(cfg.recursos_path());
    let mut dev = DevTools::from_env();
    let hour = std::env::var("AO_HOUR").ok().and_then(|h| h.parse().ok()).unwrap_or(12.0);
    let mut renderer = Renderer::new(cfg.video.pixel_perfect, hour);
    let mut game = Game::new(DevTools::default_name().unwrap_or_default(), cfg.server.clone());
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

        game.poll(&mut assets, &mut renderer.fx);
        match &game.screen {
            Screen::Login => ui::login::draw(&mut game, lay.u),
            Screen::Kicked(reason) => {
                ui::hud::kicked(&reason.clone(), lay.u);
                if dev.shot.is_some() {
                    break;
                }
            }
            Screen::Game => {
                game.check_integrity();
                input::update(&mut game, &mut renderer, &lay, now);
                dev.update(&mut game, &mut renderer, now);
                game.update(dt);
                renderer.fx.update(dt);
                if game.map.is_some() {
                    renderer.draw_world(&game, &mut assets, &lay, t_ms, dt);
                }
                let hour = renderer.hour();
                ui::hud::draw(&mut game, &lay, hour, renderer.pixel_perfect);
                if dev.maybe_capture(now) {
                    break;
                }
            }
        }
        next_frame().await;
    }
}

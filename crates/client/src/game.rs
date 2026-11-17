//! Glue between the graphics-free `client-core` and this macroquad frontend: types,
//! colours, and turning core events into particles and textures.
use crate::assets::Assets;
use crate::render::fx::Fx;
use ao_protocol::Tone;
pub use client_core::{tile_center, ClientEvent, Ent, Game, Pt, Screen};
use macroquad::prelude::*;

pub fn v(p: Pt) -> Vec2 {
    vec2(p.x, p.y)
}

pub fn tone_color(t: Tone) -> Color {
    match t {
        Tone::Combat => Color::new(1.0, 0.45, 0.35, 1.0),
        Tone::Warn => Color::new(1.0, 0.85, 0.3, 1.0),
        Tone::Chat => WHITE,
        Tone::Good => Color::new(0.45, 1.0, 0.5, 1.0),
        Tone::Info => Color::new(0.65, 0.8, 1.0, 1.0),
    }
}

/// Reads the network and renders what the core reports.
pub fn poll(game: &mut Game, assets: &mut Assets, fx: &mut Fx) {
    game.poll();
    for ev in std::mem::take(&mut game.events) {
        match ev {
            ClientEvent::MapEntered { map } => {
                if let Some(m) = &game.map {
                    let files = assets.preload_map(m);
                    if std::env::var("AO_DEBUG").is_ok() {
                        eprintln!("map {map} '{}' textures={files}", m.name);
                    }
                }
            }
            ClientEvent::Puff { at } => fx.spawn_puff(v(at)),
            ClientEvent::DeathBurst { at } => fx.death_burst(v(at)),
            ClientEvent::Blood { at } => fx.blood(v(at)),
            ClientEvent::FloatText { at, amount } => {
                let (text, c) = if amount < 0 { (format!("{amount}"), Color::new(1.0, 0.3, 0.25, 1.0)) } else { (format!("+{amount}"), Color::new(0.4, 1.0, 0.45, 1.0)) };
                fx.float_text(v(at), text, c);
            }
            ClientEvent::Effect { effect, from, to } => fx.cast(effect, v(from), v(to)),
        }
    }
}

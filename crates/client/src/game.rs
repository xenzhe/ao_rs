//! Glue between the graphics-free `client-core` and this macroquad frontend: types,
//! colours, and turning core events and raw server messages into particles, official
//! FX, weather and sound.
//!
//! Env vars for screenshots (read here, so `devtools.rs` stays untouched):
//! - `AO_FXSELF=<fx>`: plays that FXs.ini effect on the own character forever (e.g. 16)
//! - `AO_EQUIP=<weapon>,<shield>,<helmet>`: shows that equipment on the own character
//! - `AO_WEATHER=rain|snow|fog|storm|clear`: forces the weather
use crate::assets::Assets;
use crate::audio::Channel;
use crate::render::characters::{self, FRAME};
use crate::render::fx::Fx;
use crate::render::weather::FOG_GRH;
use ao_protocol::Tone;
use client_core::p3::fx::{map_music, under_roof, volume_at, weather_for, Cue, FX_FOREVER, SND_RAIN_IN_LOOP, SND_RAIN_OUT_LOOP};
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

fn env(k: &str) -> Option<String> {
    std::env::var(k).ok().filter(|s| !s.is_empty())
}

/// Reads the network and renders what the core reports.
pub fn poll(game: &mut Game, assets: &mut Assets, fx: &mut Fx) {
    let msgs = game.poll();
    fx.ensure_official(&assets.root);
    let now_ms = (get_time() * 1000.0) as f32;
    let mut self_fx: Option<u16> = None;
    for ev in std::mem::take(&mut game.events) {
        match ev {
            ClientEvent::MapEntered { map } => {
                if let Some(m) = &game.map {
                    let files = assets.preload_map(m);
                    if std::env::var("AO_DEBUG").is_ok() {
                        eprintln!("map {map} '{}' textures={files} particles={}", m.name, m.particles.len());
                    }
                    fx.load_map(m);
                    let streams: Vec<usize> = fx.map_groups.iter().map(|g| g.group.stream).collect();
                    for s in streams {
                        fx.resolve_stream(assets, s);
                    }
                }
                let music = map_music(&assets.root, map).unwrap_or(0);
                if let Some(a) = &mut fx.audio {
                    a.music(music);
                }
                self_fx = env("AO_FXSELF").and_then(|s| s.parse().ok());
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

    // Raw messages the core does not turn into events: official FX, steps, hits.
    let cues = {
        let db = &assets.db;
        let files = |g: u32| db.frame(g, 0.0).map(|f| f.file).unwrap_or(0);
        match fx.official.as_mut() {
            Some(o) => o.cues.collect(game, &msgs, &files),
            None => Vec::new(),
        }
    };
    let me_tile = game.me().map(|e| (e.x, e.y)).unwrap_or((0, 0));
    for c in cues {
        match c {
            Cue::Fx { id, fx: f, loops } => fx.char_fx(assets, id, f, loops),
            Cue::Gone { id } if id == u32::MAX => fx.char_fx.clear(),
            Cue::Gone { id } => {
                fx.char_fx.remove(&id);
            }
            Cue::Sound { id, at } => {
                let steps = matches!(id, 23 | 24 | 69 | 201 | 199 | 200 | 197 | 198 | 50);
                if let Some(a) = &mut fx.audio {
                    a.play(id, volume_at(me_tile, at), if steps { Channel::Steps } else { Channel::Effects });
                }
            }
        }
    }
    // After the cues: entering a map clears every FX.
    if let Some(f) = self_fx {
        fx.char_fx(assets, game.my_id, f, FX_FOREVER);
    }
    characters::expire_fx(assets, &mut fx.char_fx, now_ms);

    // Weather of this map and whether the player is under a roof.
    if let Some(m) = &game.map {
        let w = weather_for(m.rain, m.snow, m.fog, get_time(), env("AO_WEATHER").as_deref());
        let indoor = under_roof(game, me_tile.0, me_tile.1);
        fx.weather.set(w, indoor);
        if w.fog && !fx.grh_tex.contains_key(&FOG_GRH) {
            fx.resolve_grhs(assets, &[FOG_GRH]);
        }
        for s in [client_core::p3::fx::STREAM_RAIN, client_core::p3::fx::STREAM_SNOW] {
            fx.resolve_stream(assets, s);
        }
        let amb = if fx.weather.raining() { if indoor { SND_RAIN_IN_LOOP } else { SND_RAIN_OUT_LOOP } } else { 0 };
        if let Some(a) = &mut fx.audio {
            a.ambient(amb);
        }
    }
    if let Some(a) = &mut fx.audio {
        a.update();
    }

    // Per-frame data for `draw_character` (ids by address, FX, equipment).
    let equip_override = env("AO_EQUIP").map(|s| {
        let n: Vec<u16> = s.split(',').map(|p| p.trim().parse().unwrap_or(0)).collect();
        (n.first().copied().unwrap_or(0), n.get(1).copied().unwrap_or(0), n.get(2).copied().unwrap_or(0))
    });
    FRAME.with(|f| {
        let mut f = f.borrow_mut();
        f.ids.clear();
        for (id, e) in &game.ents {
            f.ids.insert(characters::key(e), *id);
        }
        f.my_id = game.my_id;
        f.fx = fx.char_fx.clone();
        f.equip = fx.official.as_ref().map(|o| o.equip.clone());
        if f.additive.is_none() {
            f.additive = Some(fx.additive.clone());
        }
        f.equip_override = equip_override;
    });
}

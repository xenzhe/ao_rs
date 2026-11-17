//! Automation for tests and screenshots, kept out of the game code. All driven by env vars:
//! - `AO_AUTO=1` / `AO_SHOT=file.png`: log in automatically (`AO_NAME`, default "Tester")
//! - `AO_SHOT_AT=secs`: capture time after entering the map (default 4)
//! - `AO_WALK=up|down|left|right`: keep walking in that direction
//! - `AO_FXDEMO=1`: play the spell effects locally (never sent to the server) and capture
//! - `AO_HOUR=h`: start at that hour of the day
//! - `AO_OPEN=inventory,spells,...`: windows open from the start (names: lowercase
//!   `WindowId` without spaces, e.g. `worldmap`), for screenshots of the UI
use crate::game::{v, Game};
use crate::render::Renderer;
use crate::ui::{Ui, WindowId};
use ao_rules::spells::SpellEffect;
use ao_rules::Dir;
use macroquad::prelude::*;

pub struct DevTools {
    pub shot: Option<String>,
    pub auto_login: bool,
    shot_at: f64,
    walk: Option<Dir>,
    fx_demo: bool,
    demo_step: u8,
    entered_at: Option<f64>,
    opened: bool,
}

impl DevTools {
    pub fn from_env() -> DevTools {
        let var = |k: &str| std::env::var(k).ok();
        let shot = var("AO_SHOT");
        DevTools {
            auto_login: shot.is_some() || var("AO_AUTO").is_some(),
            shot,
            shot_at: var("AO_SHOT_AT").and_then(|s| s.parse().ok()).unwrap_or(4.0),
            walk: var("AO_WALK").and_then(|d| match d.as_str() {
                "up" => Some(Dir::Up),
                "down" => Some(Dir::Down),
                "left" => Some(Dir::Left),
                "right" => Some(Dir::Right),
                _ => None,
            }),
            fx_demo: var("AO_FXDEMO").is_some(),
            demo_step: 0,
            entered_at: None,
            opened: false,
        }
    }

    pub fn default_name() -> Option<String> {
        std::env::var("AO_NAME").ok()
    }

    /// Scripted actions for this frame.
    pub fn update(&mut self, game: &mut Game, renderer: &mut Renderer, ui: &mut Ui, now: f64) {
        if game.map.is_none() {
            return;
        }
        if !self.opened {
            self.opened = true;
            let wanted = std::env::var("AO_OPEN").unwrap_or_default().to_lowercase();
            for w in WindowId::ALL {
                if wanted.split(',').any(|n| n.trim() == format!("{w:?}").to_lowercase()) {
                    ui.open(w);
                }
            }
        }
        if self.shot.is_none() {
            return;
        }
        let t = now - *self.entered_at.get_or_insert(now);
        if let Some(d) = self.walk {
            if t > 0.5 {
                game.try_step(d, now);
            }
        }
        if self.fx_demo {
            let Some(me) = game.me() else { return };
            let c = v(me.world_pos()) - vec2(0.0, 24.0);
            let fx = &mut renderer.fx;
            match self.demo_step {
                0 if t > 1.6 => {
                    fx.cast(SpellEffect::Fireball, c, c + vec2(170.0, -50.0));
                    self.demo_step = 1;
                }
                1 if t > 1.75 => {
                    fx.cast(SpellEffect::Lightning, c, c + vec2(-150.0, -80.0));
                    self.demo_step = 2;
                }
                2 if t > 1.8 => {
                    fx.cast(SpellEffect::Heal, c, c);
                    fx.cast(SpellEffect::Fireball, c, c + vec2(-70.0, 120.0));
                    self.demo_step = 3;
                }
                _ => {}
            }
        }
    }

    /// Saves the screenshot when due; returns true when the client should exit.
    pub fn maybe_capture(&mut self, now: f64) -> bool {
        let Some(path) = &self.shot else { return false };
        let t = self.entered_at.map(|e| now - e).unwrap_or(0.0);
        let due = if self.fx_demo { self.demo_step == 3 && t > 2.0 } else { t > self.shot_at };
        if due || now > 25.0 {
            get_screen_data().export_png(path);
            return true;
        }
        false
    }
}

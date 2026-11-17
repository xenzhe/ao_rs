//! Weather (rain, snow, fog by map/zone) drawn over the world. OWNER: Opus D
//! (docs/P3_CONTRATO.md).
//!
//! As the official client: rain and snow are the particles.ind streams 58 and 57
//! (`Engine_MeteoParticle_Set`) anchored at screen (250, 0) of the 23x18 view, shown only
//! when the map allows them (`MapDat.LLUVIA/NIEVE`); fog is GRH 32014 tiled every 512 px
//! in two layers drifting in opposite directions (`Engine_Weather_UpdateFog`). What the
//! weather is comes from `client_core::p3::fx::weather_for`. Under a roof the rain and
//! snow fade out (only the indoor rain loop is heard).
use super::fx::Fx;
use client_core::p3::fx::{ParticleGroup, StreamDef, Weather, STREAM_RAIN, STREAM_SNOW};
use macroquad::prelude::*;

/// Fog texture (`InitGrh TempGrh, 32014`).
pub const FOG_GRH: u32 = 32014;
/// Screen anchor of the meteo stream (`ScreenX = 250: ScreenY = 0`).
const METEO_AT: Vec2 = Vec2::new(250.0, 0.0);

#[derive(Default)]
pub struct WeatherFx {
    pub state: Weather,
    pub indoor: bool,
    rain: Option<ParticleGroup>,
    snow: Option<ParticleGroup>,
    /// 0..1 fade of rain/snow (roofs and starting/stopping).
    show: f32,
    fog: f32,
    fog1: Vec2,
    fog2: Vec2,
}

fn meteo(slot: &mut Option<ParticleGroup>, want: bool, stream: usize, streams: &[StreamDef], dt_ms: f32, now: f32) {
    let Some(def) = streams.get(stream - 1) else { return };
    match slot {
        None if want => *slot = Some(ParticleGroup::new(stream, def, -1, 0xA511_E9B3 ^ stream as u32)),
        Some(g) => {
            if !want {
                g.stop();
            }
            g.update(def, dt_ms, now);
            if g.finished() {
                *slot = None;
            }
        }
        _ => {}
    }
}

impl WeatherFx {
    pub fn set(&mut self, state: Weather, indoor: bool) {
        self.state = state;
        self.indoor = indoor;
    }

    pub fn raining(&self) -> bool {
        self.rain.is_some()
    }

    pub fn update(&mut self, streams: &[StreamDef], dt_ms: f32, now: f32) {
        meteo(&mut self.rain, self.state.rain, STREAM_RAIN, streams, dt_ms, now);
        meteo(&mut self.snow, self.state.snow, STREAM_SNOW, streams, dt_ms, now);
        let target = if self.indoor { 0.0 } else { 1.0 };
        self.show += (target - self.show) * (dt_ms / 400.0).min(1.0);
        let fog_target = if self.state.fog { 1.0 } else { 0.0 };
        self.fog += (fog_target - self.fog) * (dt_ms / 1500.0).min(1.0);
        let wrap = |v: f32| v.rem_euclid(512.0);
        self.fog1 = vec2(wrap(self.fog1.x + dt_ms * 0.023), wrap(self.fog1.y + dt_ms * 0.018));
        self.fog2 = vec2(wrap(self.fog2.x - dt_ms * 0.042), wrap(self.fog2.y - dt_ms * 0.026));
    }

    /// How much the weather darkens the day (0..1), for the light map.
    pub fn gloom(&self) -> f32 {
        let wet = if self.rain.is_some() { 0.22 } else { 0.0 } + if self.snow.is_some() { 0.08 } else { 0.0 };
        wet + self.fog * 0.1
    }

    /// Drawn after the light map, over the world. `off` moves the fog with the camera.
    pub fn draw(&self, fx: &Fx, streams: &[StreamDef], off: Vec2, view: Vec4) {
        if self.fog > 0.01 {
            if let Some((tex, src)) = fx.grh_tex.get(&FOG_GRH).and_then(|g| g.frames.first()).cloned() {
                for (o, a) in [(self.fog1 - off * 0.5, 0.32), (self.fog2 - off * 0.8, 0.22)] {
                    let o = vec2(o.x.rem_euclid(512.0) - 512.0, o.y.rem_euclid(512.0) - 512.0);
                    let mut y = o.y;
                    while y < view.w {
                        let mut x = o.x;
                        while x < view.z {
                            draw_texture_ex(&tex, x, y, Color::new(1.0, 1.0, 1.0, a * self.fog), DrawTextureParams { source: Some(src), dest_size: Some(vec2(512.0, 512.0)), ..Default::default() });
                            x += 512.0;
                        }
                        y += 512.0;
                    }
                }
            }
        }
        if self.show < 0.02 {
            return;
        }
        for g in [&self.rain, &self.snow].into_iter().flatten() {
            let Some(def) = streams.get(g.stream - 1) else { continue };
            if def.additive {
                gl_use_material(&fx.additive);
            }
            fx.draw_group(g, def, METEO_AT, self.show, g.stream == STREAM_RAIN);
            gl_use_default_material();
        }
    }
}

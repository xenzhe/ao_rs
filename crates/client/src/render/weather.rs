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
    /// Fog texture of the resource set (`profile [Weather] FogGrh`).
    pub fog_grh: u32,
    fog1: Vec2,
    fog2: Vec2,
    /// Procedural weather (resource sets without weather graphics, e.g. TSAO).
    pub own: bool,
    own_tex: Option<Texture2D>,
    /// Screen-space drops / flakes: position, speed, size.
    drops: Vec<(Vec2, f32, f32)>,
    flakes: Vec<(Vec2, f32, f32)>,
    rain_k: f32,
    snow_k: f32,
    clock: f32,
}

/// Tileable soft cloud noise (value noise, 4 octaves) for the procedural fog.
fn cloud_texture() -> Texture2D {
    const N: usize = 256;
    let hash = |x: i32, y: i32, o: i32| {
        let mut h = (x as u32).wrapping_mul(374_761_393) ^ (y as u32).wrapping_mul(668_265_263) ^ (o as u32).wrapping_mul(2_246_822_519);
        h = (h ^ (h >> 13)).wrapping_mul(1_274_126_177);
        (h ^ (h >> 16)) as f32 / u32::MAX as f32
    };
    let mut img = Image::gen_image_color(N as u16, N as u16, Color::new(1.0, 1.0, 1.0, 0.0));
    for y in 0..N {
        for x in 0..N {
            let mut v = 0.0;
            let mut amp = 0.5;
            for o in 0..4 {
                let cells = 4 * 2i32.pow(o as u32);
                let (fx, fy) = (x as f32 * cells as f32 / N as f32, y as f32 * cells as f32 / N as f32);
                let (ix, iy) = (fx.floor() as i32, fy.floor() as i32);
                let (tx, ty) = (fx - ix as f32, fy - iy as f32);
                let (sx, sy) = (tx * tx * (3.0 - 2.0 * tx), ty * ty * (3.0 - 2.0 * ty));
                let c = |a: i32, b: i32| hash(a.rem_euclid(cells), b.rem_euclid(cells), o);
                let top = c(ix, iy) + (c(ix + 1, iy) - c(ix, iy)) * sx;
                let bot = c(ix, iy + 1) + (c(ix + 1, iy + 1) - c(ix, iy + 1)) * sx;
                v += (top + (bot - top) * sy) * amp;
                amp *= 0.5;
            }
            let a = ((v - 0.35) / 0.6).clamp(0.0, 1.0);
            img.set_pixel(x as u32, y as u32, Color::new(0.85, 0.88, 0.92, a * a));
        }
    }
    let t = Texture2D::from_image(&img);
    t.set_filter(FilterMode::Linear);
    t
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
        self.rain.is_some() || self.rain_k > 0.05
    }

    /// Procedural rain and snow (screen space, sized for the whole window).
    fn update_own(&mut self, dt: f32, view: Vec2) {
        use macroquad::rand::gen_range;
        self.clock += dt;
        let ease = (dt / 1.5).min(1.0);
        self.rain_k += (if self.state.rain { 1.0 } else { 0.0 } - self.rain_k) * ease;
        self.snow_k += (if self.state.snow { 1.0 } else { 0.0 } - self.snow_k) * ease;
        let area = (view.x * view.y / (1280.0 * 800.0)).max(0.3);
        let want_drops = (self.rain_k * 420.0 * area) as usize;
        let want_flakes = (self.snow_k * 260.0 * area) as usize;
        while self.drops.len() < want_drops {
            self.drops.push((vec2(gen_range(-60.0, view.x), gen_range(-view.y, view.y)), gen_range(900.0, 1300.0), gen_range(0.5, 1.0)));
        }
        while self.flakes.len() < want_flakes {
            self.flakes.push((vec2(gen_range(0.0, view.x), gen_range(-view.y, view.y)), gen_range(40.0, 90.0), gen_range(1.2, 3.2)));
        }
        self.drops.truncate(want_drops.max(self.drops.len().saturating_sub(8)));
        self.flakes.truncate(want_flakes.max(self.flakes.len().saturating_sub(4)));
        for d in &mut self.drops {
            d.0 += vec2(0.18, 1.0) * d.1 * dt;
            if d.0.y > view.y {
                d.0 = vec2(gen_range(-60.0, view.x), gen_range(-60.0, 0.0));
            }
        }
        let t = self.clock;
        for (i, f) in self.flakes.iter_mut().enumerate() {
            f.0 += vec2((t * 0.8 + i as f32).sin() * 18.0, f.1) * dt;
            if f.0.y > view.y {
                f.0 = vec2(gen_range(0.0, view.x), gen_range(-30.0, 0.0));
            }
        }
    }

    /// Procedural fog, rain and snow over the view (after the light map).
    fn draw_own(&self, off: Vec2, view: Vec4) {
        if self.fog > 0.01 {
            if let Some(tex) = &self.own_tex {
                for (o, a, s) in [(self.fog1 - off * 0.5, 0.45, 512.0), (self.fog2 - off * 0.8, 0.3, 384.0)] {
                    let o = vec2(o.x.rem_euclid(s) - s, o.y.rem_euclid(s) - s);
                    let mut y = o.y;
                    while y < view.w {
                        let mut x = o.x;
                        while x < view.z {
                            draw_texture_ex(tex, view.x + x, view.y + y, Color::new(1.0, 1.0, 1.0, a * self.fog), DrawTextureParams { dest_size: Some(vec2(s, s)), ..Default::default() });
                            x += s;
                        }
                        y += s;
                    }
                }
            }
        }
        let show = self.show;
        if show < 0.02 {
            return;
        }
        for (p, _, k) in &self.drops {
            let q = vec2(view.x, view.y) + *p;
            draw_line(q.x, q.y, q.x + 3.0, q.y + 17.0 * k, 1.2, Color::new(0.75, 0.82, 0.95, 0.35 * k * show * self.rain_k));
        }
        for (p, _, r) in &self.flakes {
            let q = vec2(view.x, view.y) + *p;
            draw_circle(q.x, q.y, *r, Color::new(1.0, 1.0, 1.0, 0.75 * show * self.snow_k));
        }
    }

    pub fn update(&mut self, streams: &[StreamDef], dt_ms: f32, now: f32) {
        if self.own {
            if self.state.fog && self.own_tex.is_none() {
                self.own_tex = Some(cloud_texture());
            }
            self.update_own(dt_ms / 1000.0, vec2(screen_width(), screen_height()));
        } else {
            meteo(&mut self.rain, self.state.rain, STREAM_RAIN, streams, dt_ms, now);
            meteo(&mut self.snow, self.state.snow, STREAM_SNOW, streams, dt_ms, now);
        }
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
        let wet = if self.rain.is_some() { 0.22 } else { self.rain_k * 0.22 } + if self.snow.is_some() { 0.08 } else { self.snow_k * 0.08 };
        wet + self.fog * 0.1
    }

    /// Drawn after the light map, over the world. `off` moves the fog with the camera.
    pub fn draw(&self, fx: &Fx, streams: &[StreamDef], off: Vec2, view: Vec4) {
        if self.own {
            return self.draw_own(off, view);
        }
        if self.fog > 0.01 {
            if let Some((tex, src)) = fx.grh_tex.get(&self.fog_grh).and_then(|g| g.frames.first()).cloned() {
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

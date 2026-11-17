//! Particles, projectiles, lightning, floating combat text and light emitters.
//! Additive particles + a separate light map is the classic DX8/DX9 look of AO engines.
//!
//! P3 (Opus D): the official engine too. Map particles of the .csm and weather streams
//! run the particles.ind simulation of `Graficos_Particulas.bas` (logic in
//! `client_core::p3::fx`), drawn as 4-colour quads (`rgb_list` corners) with additive
//! blending when the stream says so; FXs.ini animations play on characters
//! ([`super::characters`]); sounds go to [`crate::audio`].
use super::characters::CharFx;
use super::weather::WeatherFx;
use crate::assets::Assets;
use crate::audio::Audio;
use ao_formats::graphics::anchor;
use ao_formats::AoMap;
use ao_rules::spells::SpellEffect;
use client_core::p3::fx::{EquipDb, FxCues, FxDef, ParticleGroup, StreamDef};
use macroquad::miniquad::{BlendFactor, BlendState, BlendValue, Equation, PipelineParams};
use macroquad::models::Vertex;
use macroquad::prelude::*;
use macroquad::rand::gen_range;
use std::collections::HashMap;
use std::path::Path;
use std::rc::Rc;

const VS: &str = r#"#version 100
attribute vec3 position;
attribute vec2 texcoord;
attribute vec4 color0;
varying lowp vec2 uv;
varying lowp vec4 color;
uniform mat4 Model;
uniform mat4 Projection;
void main() {
    gl_Position = Projection * Model * vec4(position, 1);
    color = color0 / 255.0;
    uv = texcoord;
}"#;

const FS: &str = r#"#version 100
varying lowp vec4 color;
varying lowp vec2 uv;
uniform sampler2D Texture;
void main() {
    gl_FragColor = color * texture2D(Texture, uv);
}"#;

fn material(blend: BlendState) -> Material {
    load_material(
        ShaderSource::Glsl { vertex: VS, fragment: FS },
        MaterialParams {
            pipeline_params: PipelineParams { color_blend: Some(blend), alpha_blend: Some(blend), ..Default::default() },
            ..Default::default()
        },
    )
    .expect("material")
}

#[derive(Clone)]
struct Part {
    pos: Vec2,
    vel: Vec2,
    life: f32,
    max: f32,
    s0: f32,
    s1: f32,
    c0: Color,
    c1: Color,
    add: bool,
    grav: f32,
    drag: f32,
    light: f32,
}

struct Proj {
    from: Vec2,
    to: Vec2,
    t: f32,
    dur: f32,
    emit: f32,
}

struct Bolt {
    pts: Vec<Vec2>,
    life: f32,
}

struct Ring {
    pos: Vec2,
    life: f32,
    color: Color,
}

struct FText {
    pos: Vec2,
    text: String,
    color: Color,
    life: f32,
}

/// Frames of a GRH resolved to textures, for particles drawn without `Assets`.
pub struct GrhTex {
    pub frames: Vec<(Texture2D, Rect)>,
    /// Total animation time in ms (0 = static).
    pub speed: f32,
}

/// The official tables, loaded once on the first frame (they need `Recursos`).
pub struct Official {
    pub streams: Vec<StreamDef>,
    pub fxs: HashMap<u16, FxDef>,
    pub equip: Rc<EquipDb>,
    pub cues: FxCues,
}

/// A particles.ind stream placed in the world (map particles) or on screen (weather).
pub struct PlacedGroup {
    pub group: ParticleGroup,
    /// Anchor in world pixels (`ScreenX * 32 + 15` of the official map loop).
    pub at: Vec2,
    /// Under a roof: hidden (with its light) unless the player is indoors too, as the
    /// official tile loop draws roofs over them.
    pub roofed: bool,
}

impl PlacedGroup {
    fn visible(&self, indoor: bool) -> bool {
        !self.roofed || indoor
    }
}

pub struct Fx {
    pub official: Option<Official>,
    pub map_groups: Vec<PlacedGroup>,
    pub grh_tex: HashMap<u32, GrhTex>,
    pub weather: WeatherFx,
    pub audio: Option<Audio>,
    pub char_fx: HashMap<u32, Vec<CharFx>>,
    pub clock_ms: f32,
    parts: Vec<Part>,
    projs: Vec<Proj>,
    bolts: Vec<Bolt>,
    rings: Vec<Ring>,
    texts: Vec<FText>,
    flash: f32,
    soft: Texture2D,
    pub additive: Material,
    pub multiply: Material,
}

fn lerpc(a: Color, b: Color, t: f32) -> Color {
    Color::new(a.r + (b.r - a.r) * t, a.g + (b.g - a.g) * t, a.b + (b.b - a.b) * t, a.a + (b.a - a.a) * t)
}

fn rv(r: f32) -> Vec2 {
    let a = gen_range(0.0, std::f32::consts::TAU);
    vec2(a.cos(), a.sin()) * gen_range(0.0, r)
}

impl Fx {
    pub fn new() -> Fx {
        let mut img = Image::gen_image_color(32, 32, Color::new(1.0, 1.0, 1.0, 0.0));
        for y in 0..32 {
            for x in 0..32 {
                let d = (vec2(x as f32 - 15.5, y as f32 - 15.5).length() / 16.0).min(1.0);
                img.set_pixel(x, y, Color::new(1.0, 1.0, 1.0, (1.0 - d).powf(1.5)));
            }
        }
        let soft = Texture2D::from_image(&img);
        soft.set_filter(FilterMode::Linear);
        Fx {
            official: None,
            map_groups: Vec::new(),
            grh_tex: HashMap::new(),
            weather: WeatherFx::default(),
            audio: None,
            char_fx: HashMap::new(),
            clock_ms: 0.0,
            parts: Vec::new(),
            projs: Vec::new(),
            bolts: Vec::new(),
            rings: Vec::new(),
            texts: Vec::new(),
            flash: 0.0,
            soft,
            additive: material(BlendState::new(Equation::Add, BlendFactor::Value(BlendValue::SourceAlpha), BlendFactor::One)),
            multiply: material(BlendState::new(Equation::Add, BlendFactor::Value(BlendValue::DestinationColor), BlendFactor::Zero)),
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn emit(&mut self, pos: Vec2, vel: Vec2, life: f32, s0: f32, s1: f32, c0: Color, c1: Color, add: bool, grav: f32, drag: f32, light: f32) {
        if self.parts.len() < 3000 {
            self.parts.push(Part { pos, vel, life, max: life, s0, s1, c0, c1, add, grav, drag, light });
        }
    }

    /// Plays the visual effect of a spell (chosen in data/spells.toml) from `from` to `to`.
    pub fn cast(&mut self, effect: SpellEffect, from: Vec2, to: Vec2) {
        match effect {
            SpellEffect::Fireball => {
                let dur = (from.distance(to) / 430.0).max(0.12);
                self.projs.push(Proj { from, to, t: 0.0, dur, emit: 0.0 });
            }
            SpellEffect::Lightning => {
                let top = to + vec2(gen_range(-30.0, 30.0), -260.0);
                let mut pts = vec![top];
                let n = 12;
                for i in 1..n {
                    let f = i as f32 / n as f32;
                    pts.push(top.lerp(to, f) + vec2(gen_range(-14.0, 14.0), gen_range(-4.0, 4.0)));
                }
                pts.push(to);
                self.bolts.push(Bolt { pts, life: 0.35 });
                self.flash = 0.55;
                for _ in 0..45 {
                    let v = rv(260.0) + vec2(0.0, -60.0);
                    self.emit(to, v, gen_range(0.2, 0.5), 3.0, 0.5, Color::new(0.8, 0.9, 1.0, 1.0), Color::new(0.3, 0.4, 1.0, 0.0), true, 500.0, 2.0, 0.0);
                }
                self.rings.push(Ring { pos: to, life: 0.5, color: Color::new(0.5, 0.7, 1.0, 1.0) });
            }
            SpellEffect::Heal => {
                for _ in 0..55 {
                    let p = to + vec2(gen_range(-14.0, 14.0), gen_range(-6.0, 18.0));
                    self.emit(p, vec2(gen_range(-8.0, 8.0), gen_range(-70.0, -30.0)), gen_range(0.6, 1.2), gen_range(2.0, 4.0), 0.5, Color::new(0.5, 1.0, 0.6, 1.0), Color::new(1.0, 1.0, 0.6, 0.0), true, -20.0, 0.5, 0.0);
                }
                self.rings.push(Ring { pos: to + vec2(0.0, 14.0), life: 0.7, color: Color::new(0.4, 1.0, 0.5, 1.0) });
            }
        }
    }

    fn explode(&mut self, at: Vec2) {
        for _ in 0..70 {
            let v = rv(220.0);
            self.emit(at, v, gen_range(0.25, 0.6), gen_range(5.0, 10.0), 1.0, Color::new(1.0, 0.85, 0.3, 1.0), Color::new(0.9, 0.2, 0.05, 0.0), true, -40.0, 3.0, 1.0);
        }
        for _ in 0..18 {
            let v = rv(60.0) + vec2(0.0, -30.0);
            self.emit(at, v, gen_range(0.7, 1.3), 6.0, 16.0, Color::new(0.25, 0.22, 0.2, 0.55), Color::new(0.2, 0.2, 0.2, 0.0), false, -25.0, 1.5, 0.0);
        }
        self.rings.push(Ring { pos: at, life: 0.35, color: Color::new(1.0, 0.6, 0.2, 1.0) });
    }

    pub fn spawn_puff(&mut self, at: Vec2) {
        for _ in 0..14 {
            self.emit(at, rv(40.0) + vec2(0.0, -20.0), gen_range(0.4, 0.8), 4.0, 10.0, Color::new(0.85, 0.85, 0.85, 0.5), Color::new(0.8, 0.8, 0.8, 0.0), false, -10.0, 2.0, 0.0);
        }
    }

    pub fn death_burst(&mut self, at: Vec2) {
        for _ in 0..30 {
            self.emit(at - vec2(0.0, 12.0), rv(120.0), gen_range(0.4, 0.9), 3.0, 1.0, Color::new(0.6, 0.05, 0.05, 1.0), Color::new(0.3, 0.0, 0.0, 0.0), false, 300.0, 1.0, 0.0);
        }
        self.spawn_puff(at);
    }

    pub fn blood(&mut self, at: Vec2) {
        for _ in 0..10 {
            self.emit(at, rv(90.0) + vec2(0.0, -40.0), gen_range(0.3, 0.6), 2.5, 1.0, Color::new(0.7, 0.05, 0.05, 1.0), Color::new(0.4, 0.0, 0.0, 0.0), false, 400.0, 1.0, 0.0);
        }
    }

    pub fn float_text(&mut self, pos: Vec2, text: String, color: Color) {
        self.texts.push(FText { pos, text, color, life: 1.1 });
    }

    pub fn update(&mut self, dt: f32) {
        self.clock_ms += dt * 1000.0;
        if let Some(o) = &self.official {
            let now = self.clock_ms;
            for g in &mut self.map_groups {
                if let Some(def) = o.streams.get(g.group.stream - 1) {
                    g.group.update(def, dt * 1000.0, now);
                }
            }
            self.weather.update(&o.streams, dt * 1000.0, now);
        }
        // Projectiles leave a fire trail and explode on arrival.
        let mut arrived = Vec::new();
        let mut trail = Vec::new();
        for p in &mut self.projs {
            p.t += dt / p.dur;
            p.emit += dt;
            let pos = p.from.lerp(p.to, p.t.min(1.0)) - vec2(0.0, (p.t.min(1.0) * std::f32::consts::PI).sin() * 18.0);
            while p.emit > 0.006 {
                p.emit -= 0.006;
                trail.push(pos);
            }
            if p.t >= 1.0 {
                arrived.push(p.to);
            }
        }
        self.projs.retain(|p| p.t < 1.0);
        for pos in trail {
            self.emit(pos + rv(3.0), rv(25.0) + vec2(0.0, -15.0), gen_range(0.18, 0.4), gen_range(5.0, 8.0), 1.0, Color::new(1.0, 0.8, 0.3, 0.9), Color::new(0.9, 0.15, 0.0, 0.0), true, -60.0, 1.0, 0.0);
        }
        for at in arrived {
            self.explode(at);
        }

        for p in &mut self.parts {
            p.life -= dt;
            p.vel.y += p.grav * dt;
            p.vel *= 1.0 / (1.0 + p.drag * dt);
            p.pos += p.vel * dt;
        }
        self.parts.retain(|p| p.life > 0.0);
        for b in &mut self.bolts {
            b.life -= dt;
        }
        self.bolts.retain(|b| b.life > 0.0);
        for r in &mut self.rings {
            r.life -= dt;
        }
        self.rings.retain(|r| r.life > 0.0);
        for t in &mut self.texts {
            t.life -= dt;
            t.pos.y -= 28.0 * dt;
        }
        self.texts.retain(|t| t.life > 0.0);
        self.flash = (self.flash - dt * 2.5).max(0.0);
    }

    fn draw_part(&self, p: &Part, off: Vec2) {
        let k = 1.0 - p.life / p.max;
        let s = p.s0 + (p.s1 - p.s0) * k;
        let c = lerpc(p.c0, p.c1, k);
        let q = p.pos - off;
        draw_texture_ex(&self.soft, q.x - s, q.y - s, c, DrawTextureParams { dest_size: Some(vec2(s * 2.0, s * 2.0)), ..Default::default() });
    }

    /// Particles in the world layer (`off` converts world pixels to screen).
    pub fn draw(&self, off: Vec2) {
        // Alpha-blended map streams (leaves, smoke) are lit by the light map like the world.
        self.draw_map_groups(off, false);
        for p in self.parts.iter().filter(|p| !p.add) {
            self.draw_part(p, off);
        }
        gl_use_material(&self.additive);
        for p in self.parts.iter().filter(|p| p.add) {
            self.draw_part(p, off);
        }
        for pr in &self.projs {
            let t = pr.t.min(1.0);
            let pos = pr.from.lerp(pr.to, t) - vec2(0.0, (t * std::f32::consts::PI).sin() * 18.0) - off;
            draw_texture_ex(&self.soft, pos.x - 12.0, pos.y - 12.0, Color::new(1.0, 0.9, 0.5, 1.0), DrawTextureParams { dest_size: Some(vec2(24.0, 24.0)), ..Default::default() });
            draw_texture_ex(&self.soft, pos.x - 5.0, pos.y - 5.0, WHITE, DrawTextureParams { dest_size: Some(vec2(10.0, 10.0)), ..Default::default() });
        }
        for r in &self.rings {
            let k = r.life;
            let q = r.pos - off;
            draw_circle_lines(q.x, q.y, 6.0 + (0.6 - k).max(0.0) * 90.0, 2.0, Color::new(r.color.r, r.color.g, r.color.b, k.min(1.0)));
        }
        gl_use_default_material();
    }

    /// Effects drawn after the light map so they stay bright at night.
    pub fn draw_overlay(&self, off: Vec2, view: Vec4) {
        // Additive streams (fire, sparks, magic) glow over the light map, as in DX9 AO.
        self.draw_map_groups(off, true);
        if let Some(o) = &self.official {
            self.weather.draw(self, &o.streams, off, view);
        }
        gl_use_material(&self.additive);
        for b in &self.bolts {
            let a = (b.life / 0.35).min(1.0) * if (b.life * 60.0) as i32 % 3 == 0 { 0.5 } else { 1.0 };
            for w in b.pts.windows(2) {
                let (p, q) = (w[0] - off, w[1] - off);
                draw_line(p.x, p.y, q.x, q.y, 9.0, Color::new(0.3, 0.45, 1.0, 0.35 * a));
                draw_line(p.x, p.y, q.x, q.y, 4.0, Color::new(0.6, 0.75, 1.0, 0.8 * a));
                draw_line(p.x, p.y, q.x, q.y, 1.5, Color::new(1.0, 1.0, 1.0, a));
            }
        }
        if self.flash > 0.0 {
            draw_rectangle(view.x, view.y, view.z, view.w, Color::new(0.6, 0.7, 1.0, self.flash * 0.35));
        }
        gl_use_default_material();
        for t in &self.texts {
            let q = t.pos - off;
            let a = t.life.min(1.0);
            let d = measure_text(&t.text, None, 20, 1.0);
            draw_text(&t.text, q.x - d.width / 2.0 + 1.0, q.y + 1.0, 20.0, Color::new(0.0, 0.0, 0.0, a));
            draw_text(&t.text, q.x - d.width / 2.0, q.y, 20.0, Color::new(t.color.r, t.color.g, t.color.b, a));
        }
    }

    /// Soft radial light for the light map (call with the additive material active).
    pub fn light(&self, p: Vec2, r: f32, c: Color) {
        draw_texture_ex(&self.soft, p.x - r, p.y - r, c, DrawTextureParams { dest_size: Some(vec2(r * 2.0, r * 2.0)), ..Default::default() });
    }

    /// Light emitters for the light map (world pixels, radius, color).
    pub fn lights(&self) -> Vec<(Vec2, f32, Color)> {
        let mut out = Vec::new();
        for p in &self.projs {
            let t = p.t.min(1.0);
            out.push((p.from.lerp(p.to, t), 90.0, Color::new(1.0, 0.6, 0.25, 1.0)));
        }
        for b in &self.bolts {
            out.push((*b.pts.last().unwrap(), 220.0, Color::new(0.6, 0.7, 1.0, b.life / 0.35)));
        }
        for r in &self.rings {
            out.push((r.pos, 110.0, Color::new(r.color.r, r.color.g, r.color.b, r.life.min(1.0))));
        }
        for p in self.parts.iter().filter(|p| p.light > 0.0).step_by(4) {
            out.push((p.pos, 40.0, Color::new(1.0, 0.55, 0.2, (p.life / p.max) * 0.6)));
        }
        // Additive map streams light their surroundings with their own colour.
        if let Some(o) = &self.official {
            for g in &self.map_groups {
                if !g.visible(self.weather.indoor) {
                    continue;
                }
                let Some(def) = o.streams.get(g.group.stream - 1).filter(|d| d.additive) else { continue };
                let c = stream_color(def);
                out.push((g.at, 80.0, Color::new(c.r, c.g, c.b, 0.55)));
            }
        }
        out
    }
}

/// Whether a world pixel is covered by a roof sprite (layer 4). The official client
/// renders map particles before layer 4, so roofs hide them; here they are drawn after
/// the world, so a group under a roof is hidden instead (shown when the player is
/// indoors and roofs fade).
fn under_layer4(db: &ao_formats::GraphicsDb, map: &AoMap, p: Vec2) -> bool {
    let (tx, ty) = ((p.x / 32.0) as i32 + 1, (p.y / 32.0) as i32 + 1);
    for y in ty..=ty + 8 {
        for x in tx - 6..=tx + 6 {
            if !ao_formats::csm::on_map(x, y) {
                continue;
            }
            let g = map.layers[3][ao_formats::csm::tile(x, y)];
            let Some(gr) = (g > 0).then(|| db.frame(g, 0.0)).flatten() else { continue };
            let (ax, ay) = anchor(gr, (x - 1) as f32 * 32.0, (y - 1) as f32 * 32.0, true);
            if Rect::new(ax, ay, gr.w, gr.h).contains(p) {
                return true;
            }
        }
    }
    false
}

/// Average corner colour of a stream (for its light).
pub fn stream_color(def: &StreamDef) -> Color {
    let mut s = [0.0f32; 3];
    for c in &def.colors {
        for i in 0..3 {
            s[i] += c[i] as f32 / 255.0 / 4.0;
        }
    }
    Color::new(s[0], s[1], s[2], 1.0)
}

/// Draws a GRH frame as a quad with one colour per corner (`rgb_list` order of the
/// official `Grh_Render`: 0 bottom-left, 1 top-left, 2 bottom-right, 3 top-right),
/// rotated `angle` degrees around its centre and optionally resized.
#[allow(clippy::too_many_arguments)]
pub fn draw_quad(tex: &Texture2D, src: Rect, x: f32, y: f32, size: Vec2, angle: f32, colors: [Color; 4]) {
    let (tw, th) = (tex.width(), tex.height());
    let (u0, v0, u1, v1) = (src.x / tw, src.y / th, (src.x + src.w) / tw, (src.y + src.h) / th);
    let c = vec2(x + size.x / 2.0, y + size.y / 2.0);
    let (s, co) = angle.to_radians().sin_cos();
    let rot = |p: Vec2| {
        let d = p - c;
        c + vec2(d.x * co - d.y * s, d.x * s + d.y * co)
    };
    let corners = [(vec2(x, y + size.y), u0, v1, colors[0]), (vec2(x, y), u0, v0, colors[1]), (vec2(x + size.x, y + size.y), u1, v1, colors[2]), (vec2(x + size.x, y), u1, v0, colors[3])];
    let verts: Vec<Vertex> = corners
        .iter()
        .map(|(p, u, v, col)| {
            let p = if angle != 0.0 { rot(*p) } else { *p };
            Vertex::new(p.x, p.y, 0.0, *u, *v, *col)
        })
        .collect();
    let gl = unsafe { get_internal_gl() }.quad_gl;
    gl.texture(Some(tex));
    gl.draw_mode(DrawMode::Triangles);
    gl.geometry(&verts, &[0, 1, 3, 0, 3, 2]);
    gl.texture(None);
}

impl Fx {
    /// Loads the official tables the first time (FXs.ini, particles.ind, equipment,
    /// spell sounds) and opens the audio device.
    pub fn ensure_official(&mut self, root: &Path) {
        if self.official.is_some() {
            return;
        }
        self.official = Some(Official {
            streams: client_core::p3::fx::load_particles(root),
            fxs: client_core::p3::fx::load_fxs(root),
            equip: Rc::new(EquipDb::load(root)),
            cues: FxCues::with_spells(root),
        });
        self.audio = Some(Audio::new(root.to_path_buf()));
    }

    /// Places the map's own particle streams (`.csm` particles) and drops the old ones.
    pub fn load_map(&mut self, map: &AoMap, db: &ao_formats::GraphicsDb) {
        self.map_groups.clear();
        self.char_fx.clear();
        let Some(o) = &self.official else { return };
        for (i, (x, y, s)) in map.particles.iter().enumerate() {
            let s = *s as usize;
            let Some(def) = (s >= 1).then(|| o.streams.get(s - 1)).flatten() else { continue };
            let at = vec2((x - 1) as f32 * 32.0 + 15.0, (y - 1) as f32 * 32.0 + 15.0);
            let roofed = under_layer4(db, map, at);
            self.map_groups.push(PlacedGroup { group: ParticleGroup::new(s, def, -1, 0x9E37_79B9 ^ (i as u32 * 7919 + 1)), at, roofed });
        }
    }

    /// Makes sure every GRH a stream can emit has its textures resolved.
    pub fn resolve_stream(&mut self, assets: &mut Assets, stream: usize) {
        let Some(grhs) = self.official.as_ref().and_then(|o| o.streams.get(stream.wrapping_sub(1))).map(|d| d.grhs.clone()) else { return };
        self.resolve_grhs(assets, &grhs);
    }

    pub fn resolve_grhs(&mut self, assets: &mut Assets, grhs: &[u32]) {
        for &g in grhs {
            if self.grh_tex.contains_key(&g) {
                continue;
            }
            let Some(info) = assets.db.grh(g).cloned() else {
                self.grh_tex.insert(g, GrhTex { frames: vec![], speed: 0.0 });
                continue;
            };
            let mut frames = Vec::new();
            for f in &info.frames {
                if let Some(fg) = assets.db.grh(*f).cloned() {
                    if let Some(t) = assets.texture(fg.file) {
                        frames.push((t, Rect::new(fg.sx, fg.sy, fg.w, fg.h)));
                    }
                }
            }
            self.grh_tex.insert(g, GrhTex { frames, speed: if info.frames.len() > 1 { info.speed } else { 0.0 } });
        }
    }

    /// Draws one stream group with its anchor at `at` (render-target pixels).
    pub fn draw_group(&self, g: &ParticleGroup, def: &StreamDef, at: Vec2, alpha: f32, streak: bool) {
        let mut cols = [WHITE; 4];
        for (c, d) in cols.iter_mut().zip(def.colors.iter()) {
            *c = Color::from_rgba(d[0], d[1], d[2], (255.0 * alpha) as u8);
        }
        for p in &g.parts {
            if p.grh == 0 || p.alive <= 0 {
                continue;
            }
            let Some(gt) = self.grh_tex.get(&p.grh) else { continue };
            if gt.frames.is_empty() {
                continue;
            }
            let fi = if gt.speed > 0.0 { (((self.clock_ms - p.born_ms).max(0.0) / (gt.speed / gt.frames.len() as f32)) as usize) % gt.frames.len() } else { 0 };
            let (tex, src) = &gt.frames[fi];
            let mut size = def.resize.map(|(w, h)| vec2(w as f32, h as f32)).unwrap_or(vec2(src.w, src.h));
            let mut angle = p.angle;
            if streak {
                // Motion blur along this move's displacement (rain): same drop, longer.
                let fric = def.friction.max(1) as f32;
                let d = vec2(p.vx / fric, p.vy / fric);
                size = vec2(size.x.min(2.5), d.length().max(size.y) * 1.6);
                angle = -d.x.atan2(d.y).to_degrees();
            }
            let gi = ao_formats::graphics::Grh { frames: vec![], file: 0, sx: 0.0, sy: 0.0, w: size.x, h: size.y, speed: 0.0 };
            let (x, y) = anchor(&gi, at.x + p.x, at.y + p.y, true);
            draw_quad(tex, *src, x, y, size, angle, cols);
        }
    }

    fn draw_map_groups(&self, off: Vec2, additive: bool) {
        let Some(o) = &self.official else { return };
        let mut on = false;
        for g in &self.map_groups {
            let Some(def) = o.streams.get(g.group.stream - 1) else { continue };
            if def.additive != additive {
                continue;
            }
            if !g.visible(self.weather.indoor) {
                continue;
            }
            let at = g.at - off;
            if at.x < -300.0 || at.y < -300.0 || at.x > super::layout::RT_W + 300.0 || at.y > super::layout::RT_H + 300.0 {
                continue;
            }
            if additive && !on {
                gl_use_material(&self.additive);
                on = true;
            }
            self.draw_group(&g.group, def, at, 1.0, false);
        }
        if on {
            gl_use_default_material();
        }
    }

    /// Starts (`fx > 0`) or stops (`fx == 0`: the looping ones) an FX on an entity.
    /// Meditations play an intro once and then their loop (official composed FX).
    pub fn char_fx(&mut self, assets: &Assets, id: u32, fx: u16, loops: u8) {
        use client_core::p3::fx::{meditation_clips, FX_FOREVER};
        if fx == 0 {
            if let Some(l) = self.char_fx.get_mut(&id) {
                l.retain(|f| f.loops != FX_FOREVER);
            }
            return;
        }
        let Some(o) = self.official.as_ref() else { return };
        // Same clock as `draw_character`'s `t_ms`.
        let now = (get_time() * 1000.0) as f32;
        let mut add = Vec::new();
        match (loops == FX_FOREVER).then(|| meditation_clips(fx)).flatten() {
            Some((intro, lp)) => {
                let mut at = now;
                if let Some(d) = o.fxs.get(&intro).copied() {
                    add.push(CharFx { def: d, loops: 0, started_ms: now });
                    at += assets.db.grh(d.grh).map(|g| g.speed).unwrap_or(0.0);
                }
                if let Some(d) = o.fxs.get(&lp).copied() {
                    add.push(CharFx { def: d, loops: FX_FOREVER, started_ms: at });
                }
            }
            None => {
                if let Some(d) = o.fxs.get(&fx).copied() {
                    add.push(CharFx { def: d, loops, started_ms: now });
                }
            }
        }
        let list = self.char_fx.entry(id).or_default();
        if loops == FX_FOREVER {
            list.retain(|f| f.loops != FX_FOREVER);
        }
        list.extend(add);
    }
}

//! Particles, projectiles, lightning, floating combat text and light emitters.
//! Additive particles + a separate light map is the classic DX8/DX9 look of AO engines.
use macroquad::miniquad::{BlendFactor, BlendState, BlendValue, Equation, PipelineParams};
use macroquad::prelude::*;
use macroquad::rand::gen_range;
use ao_rules::spells::SpellEffect;

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

pub struct Fx {
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
        out
    }
}

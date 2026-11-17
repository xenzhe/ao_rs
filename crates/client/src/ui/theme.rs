//! Numic AO look (docs/ESTILO_UI.md): role-playing fonts, the palette, procedural materials
//! (leather, parchment) generated once, and the ornamental frames every window shares.
//! Widgets read the fonts through [`font`] so every screen looks the same.
#![allow(dead_code)] // toolkit: pieces used by the windows
use macroquad::prelude::*;
use std::cell::RefCell;
use std::path::Path;

// ---------------------------------------------------------------- palette

/// Warm near-black (scene backgrounds).
pub const INK_DEEP: Color = Color::new(0.035, 0.028, 0.022, 1.0);
/// Dark leather (window body, under the texture).
pub const LEATHER: Color = Color::new(0.11, 0.075, 0.05, 1.0);
pub const WOOD: Color = Color::new(0.09, 0.07, 0.05, 1.0);
pub const WOOD_LIGHT: Color = Color::new(0.16, 0.12, 0.08, 1.0);
/// Sunken surfaces: slots, fields, lists.
pub const RECESS: Color = Color::new(0.04, 0.03, 0.022, 0.92);
/// Raised surfaces: secondary buttons, cards.
pub const RAISED: Color = Color::new(0.19, 0.135, 0.085, 1.0);
pub const GOLD: Color = Color::new(0.70, 0.54, 0.27, 1.0);
pub const GOLD_BRIGHT: Color = Color::new(0.98, 0.82, 0.46, 1.0);
pub const GOLD_DIM: Color = Color::new(0.42, 0.32, 0.17, 1.0);
pub const GOLD_TEXT: Color = Color::new(0.92, 0.76, 0.44, 1.0);
pub const PARCHMENT: Color = Color::new(0.86, 0.79, 0.64, 1.0);
pub const INK: Color = Color::new(0.20, 0.13, 0.07, 1.0);
pub const TEXT: Color = Color::new(0.94, 0.91, 0.85, 1.0);
pub const TEXT_DIM: Color = Color::new(0.68, 0.63, 0.55, 1.0);
pub const TEXT_FAINT: Color = Color::new(0.45, 0.42, 0.38, 1.0);
pub const MUTED: Color = TEXT_DIM;
pub const GOOD: Color = Color::new(0.50, 0.90, 0.48, 1.0);
pub const BAD: Color = Color::new(0.95, 0.38, 0.32, 1.0);
pub const WARN: Color = Color::new(1.0, 0.78, 0.30, 1.0);
pub const HP: Color = Color::new(0.72, 0.10, 0.09, 1.0);
pub const MANA: Color = Color::new(0.14, 0.32, 0.86, 1.0);
pub const STAMINA: Color = Color::new(0.36, 0.62, 0.22, 1.0);
pub const HUNGER: Color = Color::new(0.85, 0.52, 0.16, 1.0);
pub const THIRST: Color = Color::new(0.22, 0.62, 0.80, 1.0);
pub const SLOT: Color = RECESS;

// ---------------------------------------------------------------- fonts

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum FontKind {
    /// Cinzel Decorative Bold: logo and scene titles, once per screen.
    Display,
    /// Cinzel: window titles, section headings, important names.
    Title,
    /// IM Fell English: role-playing descriptions, lore, item flavour.
    Narrative,
    NarrativeItalic,
    /// Alegreya Sans: the working font (data, buttons, lists, numbers).
    Body,
    /// Almendra: seals, signatures, small ornaments.
    Script,
}

const FONT_FILES: [(FontKind, &[u8]); 6] = [
    (FontKind::Display, include_bytes!("../../fonts/CinzelDecorative-Bold.ttf")),
    (FontKind::Title, include_bytes!("../../fonts/Cinzel.ttf")),
    (FontKind::Narrative, include_bytes!("../../fonts/IMFellEnglish-Regular.ttf")),
    (FontKind::NarrativeItalic, include_bytes!("../../fonts/IMFellEnglish-Italic.ttf")),
    (FontKind::Body, include_bytes!("../../fonts/AlegreyaSans-AO.ttf")),
    (FontKind::Script, include_bytes!("../../fonts/Almendra-Regular.ttf")),
];

struct Loaded {
    fonts: Vec<(FontKind, Font)>,
    leather: Option<Texture2D>,
    parchment: Option<Texture2D>,
}

thread_local! {
    static LOADED: RefCell<Loaded> = const { RefCell::new(Loaded { fonts: Vec::new(), leather: None, parchment: None }) };
}

/// The font of a kind (None: macroquad's default, if loading failed).
pub fn font(kind: FontKind) -> Option<Font> {
    LOADED.with(|l| l.borrow().fonts.iter().find(|(k, _)| *k == kind).map(|(_, f)| f.clone()))
}

// ---------------------------------------------------------------- materials

/// Deterministic value noise in 0..1 (no RNG state; cheap, used once at load).
fn hash(x: i32, y: i32, seed: u32) -> f32 {
    let mut h = (x as u32).wrapping_mul(374_761_393) ^ (y as u32).wrapping_mul(668_265_263) ^ seed.wrapping_mul(2_246_822_519);
    h = (h ^ (h >> 13)).wrapping_mul(1_274_126_177);
    ((h ^ (h >> 16)) & 0xffff) as f32 / 65535.0
}

fn smooth_noise(x: f32, y: f32, seed: u32) -> f32 {
    let (xi, yi) = (x.floor() as i32, y.floor() as i32);
    let (fx, fy) = (x - xi as f32, y - yi as f32);
    let s = |t: f32| t * t * (3.0 - 2.0 * t);
    let (a, b, c, d) = (hash(xi, yi, seed), hash(xi + 1, yi, seed), hash(xi, yi + 1, seed), hash(xi + 1, yi + 1, seed));
    let (u, v) = (s(fx), s(fy));
    a + (b - a) * u + (c - a) * v + (a - b - c + d) * u * v
}

fn fbm(x: f32, y: f32, seed: u32) -> f32 {
    let (mut v, mut amp, mut f) = (0.0, 0.5, 1.0);
    for o in 0..4 {
        v += amp * smooth_noise(x * f, y * f, seed + o);
        amp *= 0.5;
        f *= 2.0;
    }
    v
}

/// Tileable-enough procedural material (256²), generated once.
fn material(base: Color, contrast: f32, grain: f32, seed: u32) -> Texture2D {
    const N: u16 = 256;
    let mut img = Image::gen_image_color(N, N, base);
    for y in 0..N as u32 {
        for x in 0..N as u32 {
            let (fx, fy) = (x as f32 / 32.0, y as f32 / 32.0);
            // Large blotches + fine fibres stretched horizontally.
            let blot = fbm(fx, fy, seed) - 0.5;
            let fibre = smooth_noise(x as f32 / 2.0, y as f32 / 14.0, seed + 9) - 0.5;
            let k = 1.0 + blot * contrast + fibre * grain;
            img.set_pixel(x, y, Color::new((base.r * k).clamp(0.0, 1.0), (base.g * k).clamp(0.0, 1.0), (base.b * k).clamp(0.0, 1.0), 1.0));
        }
    }
    let t = Texture2D::from_image(&img);
    t.set_filter(FilterMode::Linear);
    t
}

/// Draws `tex` tiled over `r` (UV repeat by splitting into tiles).
fn tile_texture(tex: &Texture2D, r: Rect, tint: Color, scale: f32) {
    let ts = 256.0 * scale;
    let mut y = r.y;
    while y < r.bottom() {
        let h = (r.bottom() - y).min(ts);
        let mut x = r.x;
        while x < r.right() {
            let w = (r.right() - x).min(ts);
            draw_texture_ex(tex, x, y, tint, DrawTextureParams { dest_size: Some(vec2(w, h)), source: Some(Rect::new(0.0, 0.0, w / scale, h / scale)), ..Default::default() });
            x += ts;
        }
        y += ts;
    }
}

/// Which surface to paint.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Surface {
    /// Window body: textured dark leather with an inner vignette.
    Leather,
    /// Light parchment (letters, quests, descriptions): use `INK` text on it.
    Parchment,
    /// Sunken: slots, fields, lists.
    Recess,
    /// Raised card / secondary button.
    Raised,
}

/// Paints a surface (no border).
pub fn surface(r: Rect, s: Surface, u: f32) {
    match s {
        Surface::Leather => {
            let tex = LOADED.with(|l| l.borrow().leather.clone());
            match tex {
                Some(t) => tile_texture(&t, r, WHITE, u.max(1.0)),
                None => draw_rectangle(r.x, r.y, r.w, r.h, LEATHER),
            }
            inner_vignette(r, 26.0 * u, 0.45);
        }
        Surface::Parchment => {
            let tex = LOADED.with(|l| l.borrow().parchment.clone());
            match tex {
                Some(t) => tile_texture(&t, r, WHITE, u.max(1.0)),
                None => draw_rectangle(r.x, r.y, r.w, r.h, PARCHMENT),
            }
            inner_vignette(r, 18.0 * u, 0.35);
        }
        Surface::Recess => {
            draw_rectangle(r.x, r.y, r.w, r.h, RECESS);
            // Inner shadow from the top and left: it reads as carved in.
            for i in 0..4 {
                let a = 0.22 - i as f32 * 0.05;
                draw_line(r.x, r.y + i as f32, r.right(), r.y + i as f32, 1.0, Color::new(0.0, 0.0, 0.0, a));
                draw_line(r.x + i as f32, r.y, r.x + i as f32, r.bottom(), 1.0, Color::new(0.0, 0.0, 0.0, a * 0.7));
            }
            draw_line(r.x, r.bottom() - 0.5, r.right(), r.bottom() - 0.5, 1.0, Color::new(1.0, 0.85, 0.6, 0.06));
        }
        Surface::Raised => {
            draw_rectangle(r.x, r.y, r.w, r.h, RAISED);
            draw_rectangle(r.x, r.y, r.w, r.h * 0.5, Color::new(1.0, 0.9, 0.7, 0.05));
            draw_line(r.x, r.y + 0.5, r.right(), r.y + 0.5, 1.0, Color::new(1.0, 0.9, 0.7, 0.12));
            draw_line(r.x, r.bottom() - 0.5, r.right(), r.bottom() - 0.5, 1.0, Color::new(0.0, 0.0, 0.0, 0.5));
        }
    }
}

/// Darkens the edges of `r` inward over `depth` pixels.
pub fn inner_vignette(r: Rect, depth: f32, strength: f32) {
    let steps = 8;
    for i in 0..steps {
        let t = i as f32 / steps as f32;
        let a = strength * (1.0 - t) * (1.0 - t) / steps as f32 * 2.2;
        let d = depth * t;
        let c = Color::new(0.0, 0.0, 0.0, a);
        let w = depth / steps as f32;
        draw_rectangle(r.x, r.y + d, r.w, w, c);
        draw_rectangle(r.x, r.bottom() - d - w, r.w, w, c);
        draw_rectangle(r.x + d, r.y, w, r.h, c);
        draw_rectangle(r.right() - d - w, r.y, w, r.h, c);
    }
}

// ---------------------------------------------------------------- ornaments

/// A gold metal border: dark outline, gold body, bright bevel on top-left.
pub fn metal_border(r: Rect, width: f32) {
    draw_rectangle_lines(r.x - 1.0, r.y - 1.0, r.w + 2.0, r.h + 2.0, 2.0, Color::new(0.0, 0.0, 0.0, 0.8));
    draw_rectangle_lines(r.x, r.y, r.w, r.h, width, GOLD);
    draw_line(r.x + width, r.y + 1.0, r.right() - width, r.y + 1.0, 1.0, Color::new(1.0, 0.9, 0.6, 0.55));
    draw_line(r.x + 1.0, r.y + width, r.x + 1.0, r.bottom() - width, 1.0, Color::new(1.0, 0.9, 0.6, 0.35));
    draw_rectangle_lines(r.x + width + 2.0, r.y + width + 2.0, r.w - 2.0 * width - 4.0, r.h - 2.0 * width - 4.0, 1.0, GOLD_DIM);
}

/// Ornamental corner (filigree) at `p`, pointing into the box along (sx, sy) = ±1.
pub fn corner(p: Vec2, sx: f32, sy: f32, s: f32) {
    let g = GOLD;
    let hi = Color::new(1.0, 0.88, 0.55, 0.9);
    // L bracket.
    draw_line(p.x, p.y, p.x + sx * s, p.y, 3.0, g);
    draw_line(p.x, p.y, p.x, p.y + sy * s, 3.0, g);
    // Diamond stud.
    let c = p + vec2(sx, sy) * (s * 0.32);
    let d = s * 0.16;
    draw_triangle(c + vec2(0.0, -d), c + vec2(d, 0.0), c + vec2(0.0, d), g);
    draw_triangle(c + vec2(0.0, -d), c + vec2(-d, 0.0), c + vec2(0.0, d), g);
    draw_circle(c.x, c.y, d * 0.35, hi);
    // Curls.
    draw_circle_lines(p.x + sx * s * 0.78, p.y + sy * s * 0.12, s * 0.12, 1.5, g);
    draw_circle_lines(p.x + sx * s * 0.12, p.y + sy * s * 0.78, s * 0.12, 1.5, g);
}

/// `——◆——` separator centred on `cx`, width `w`.
pub fn divider(cx: f32, y: f32, w: f32, u: f32) {
    let d = 4.5 * u;
    let fade = |x0: f32, x1: f32| {
        let n = 12;
        for i in 0..n {
            let t0 = i as f32 / n as f32;
            let t1 = (i + 1) as f32 / n as f32;
            let a = 1.0 - t0;
            draw_line(x0 + (x1 - x0) * t0, y, x0 + (x1 - x0) * t1, y, 1.2 * u, Color::new(GOLD.r, GOLD.g, GOLD.b, 0.85 * a));
        }
    };
    fade(cx - d * 2.0, cx - w / 2.0);
    fade(cx + d * 2.0, cx + w / 2.0);
    let c = vec2(cx, y);
    draw_triangle(c + vec2(0.0, -d), c + vec2(d, 0.0), c + vec2(0.0, d), GOLD);
    draw_triangle(c + vec2(0.0, -d), c + vec2(-d, 0.0), c + vec2(0.0, d), GOLD);
}

/// Heading N2: small caps title + ornamental divider under it. Returns the y below.
pub fn section(title: &str, x: f32, y: f32, w: f32, u: f32) -> f32 {
    let size = 18.0 * u;
    super::widgets::text_with(FontKind::Title, &title.to_uppercase(), x, y + size, size, GOLD_TEXT);
    let line_y = y + size + 8.0 * u;
    draw_line(x, line_y, x + w, line_y, 1.0, Color::new(GOLD.r, GOLD.g, GOLD.b, 0.35));
    line_y + 10.0 * u
}

pub struct Theme;

impl Theme {
    /// Loads the embedded fonts and generates the materials (once, at start).
    pub fn load(_recursos: &Path) -> Theme {
        let fonts = FONT_FILES.iter().filter_map(|(k, bytes)| load_ttf_font_from_bytes(bytes).ok().map(|mut f| {
            f.set_filter(FilterMode::Linear);
            (*k, f)
        })).collect();
        let leather = material(Color::new(0.12, 0.082, 0.055, 1.0), 0.55, 0.18, 7);
        let parchment = material(Color::new(0.84, 0.76, 0.60, 1.0), 0.28, 0.10, 21);
        LOADED.with(|l| *l.borrow_mut() = Loaded { fonts, leather: Some(leather), parchment: Some(parchment) });
        Theme
    }

    /// A window (N1): drop shadow, leather body, metal frame with filigree corners and a
    /// title plaque over the top edge, close button.
    pub fn window_frame(&self, r: Rect, title: &str, u: f32, close_hover: bool) {
        // Soft shadow.
        for i in 1..=6 {
            let o = i as f32 * 2.0 * u;
            draw_rectangle(r.x + o * 0.5, r.y + o, r.w, r.h, Color::new(0.0, 0.0, 0.0, 0.07));
        }
        surface(r, Surface::Leather, u);
        metal_border(r, 2.5 * u);
        let s = 22.0 * u;
        corner(vec2(r.x + 3.0 * u, r.y + 3.0 * u), 1.0, 1.0, s);
        corner(vec2(r.right() - 3.0 * u, r.y + 3.0 * u), -1.0, 1.0, s);
        corner(vec2(r.x + 3.0 * u, r.bottom() - 3.0 * u), 1.0, -1.0, s);
        corner(vec2(r.right() - 3.0 * u, r.bottom() - 3.0 * u), -1.0, -1.0, s);
        // Title plaque, centred, over the top edge.
        let size = 20.0 * u;
        let tw = super::widgets::measure_with(FontKind::Title, title, size).width;
        let pw = tw + 64.0 * u;
        let plaque = Rect::new(r.x + (r.w - pw) / 2.0, r.y - 6.0 * u, pw, 32.0 * u);
        draw_rectangle(plaque.x, plaque.y, plaque.w, plaque.h, Color::new(0.07, 0.05, 0.035, 1.0));
        draw_rectangle(plaque.x, plaque.y, plaque.w, plaque.h * 0.5, Color::new(1.0, 0.85, 0.5, 0.05));
        metal_border(plaque, 2.0 * u);
        let mid = plaque.y + plaque.h / 2.0;
        for side in [-1.0f32, 1.0] {
            let tip = vec2(plaque.x + plaque.w / 2.0 + side * (plaque.w / 2.0 + 12.0 * u), mid);
            let base = vec2(plaque.x + plaque.w / 2.0 + side * plaque.w / 2.0, mid);
            draw_triangle(base + vec2(0.0, -7.0 * u), base + vec2(0.0, 7.0 * u), tip, GOLD);
        }
        super::widgets::text_with(FontKind::Title, title, plaque.x + (plaque.w - tw) / 2.0, mid + size * 0.36, size, GOLD_BRIGHT);
        // Close: a small engraved X.
        let c = Rect::new(r.right() - 30.0 * u, r.y + 8.0 * u, 22.0 * u, 22.0 * u);
        let col = if close_hover { BAD } else { GOLD_DIM };
        if close_hover {
            draw_circle(c.x + c.w / 2.0, c.y + c.h / 2.0, c.w * 0.55, Color::new(0.5, 0.08, 0.05, 0.6));
        }
        let (a, b) = (c.x + 6.0 * u, c.right() - 6.0 * u);
        let (t, bt) = (c.y + 6.0 * u, c.bottom() - 6.0 * u);
        draw_line(a, t, b, bt, 2.2 * u, col);
        draw_line(b, t, a, bt, 2.2 * u, col);
    }
}

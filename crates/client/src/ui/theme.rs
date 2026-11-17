//! The game's look: fonts (Cardo for titles, Alegreya Sans for text; both SIL OFL, shipped
//! with AO20 in `Recursos/OUTPUT`) and the colour palette of dark wood, parchment and gold.
//! Widgets read the fonts through [`font`] so every window looks the same.
#![allow(dead_code)] // toolkit: pieces used by the P3 windows
use macroquad::prelude::*;
use std::cell::RefCell;
use std::path::Path;

pub const WOOD: Color = Color::new(0.09, 0.07, 0.05, 0.97);
pub const WOOD_LIGHT: Color = Color::new(0.16, 0.12, 0.08, 1.0);
pub const GOLD: Color = Color::new(0.62, 0.48, 0.24, 1.0);
pub const GOLD_TEXT: Color = Color::new(0.88, 0.70, 0.36, 1.0);
pub const PARCHMENT: Color = Color::new(0.86, 0.79, 0.64, 1.0);
pub const INK: Color = Color::new(0.20, 0.13, 0.07, 1.0);
pub const TEXT: Color = Color::new(0.93, 0.90, 0.84, 1.0);
pub const MUTED: Color = Color::new(0.62, 0.58, 0.52, 1.0);
pub const GOOD: Color = Color::new(0.45, 1.0, 0.5, 1.0);
pub const BAD: Color = Color::new(1.0, 0.4, 0.35, 1.0);
pub const SLOT: Color = Color::new(0.05, 0.04, 0.03, 0.9);

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum FontKind {
    Title,
    Body,
}

thread_local! {
    static FONTS: RefCell<(Option<Font>, Option<Font>)> = const { RefCell::new((None, None)) };
}

/// The font of a kind (None: macroquad's default, if the AO fonts were not found).
pub fn font(kind: FontKind) -> Option<Font> {
    FONTS.with(|f| {
        let f = f.borrow();
        match kind {
            FontKind::Title => f.0.clone(),
            FontKind::Body => f.1.clone(),
        }
    })
}

pub struct Theme;

impl Theme {
    /// Loads the AO20 fonts from `recursos/OUTPUT` (silently falls back to the default).
    pub fn load(recursos: &Path) -> Theme {
        let load = |name: &str| std::fs::read(recursos.join("OUTPUT").join(name)).ok().and_then(|b| load_ttf_font_from_bytes(&b).ok());
        let title = load("Cardo-Bold.ttf");
        let body = load("Alegreya Sans AO.ttf");
        FONTS.with(|f| *f.borrow_mut() = (title, body));
        Theme
    }

    /// A window: wood panel, gold double border, title bar and close button.
    pub fn window_frame(&self, r: Rect, title: &str, u: f32, close_hover: bool) {
        draw_rectangle(r.x + 6.0 * u, r.y + 8.0 * u, r.w, r.h, Color::new(0.0, 0.0, 0.0, 0.35));
        draw_rectangle(r.x, r.y, r.w, r.h, WOOD);
        let bar = Rect::new(r.x, r.y, r.w, 30.0 * u);
        draw_rectangle(bar.x, bar.y, bar.w, bar.h, Color::new(0.22, 0.15, 0.08, 1.0));
        draw_rectangle(bar.x, bar.y, bar.w, bar.h * 0.45, Color::new(1.0, 0.85, 0.5, 0.06));
        draw_line(bar.x, bar.bottom(), bar.right(), bar.bottom(), 1.5 * u, GOLD);
        draw_rectangle_lines(r.x, r.y, r.w, r.h, 2.0 * u, GOLD);
        draw_rectangle_lines(r.x + 3.0 * u, r.y + 3.0 * u, r.w - 6.0 * u, r.h - 6.0 * u, 1.0, Color::new(0.30, 0.22, 0.12, 1.0));
        super::widgets::text_with(FontKind::Title, title, r.x + 14.0 * u, r.y + 21.0 * u, 19.0 * u, GOLD_TEXT);
        let c = Rect::new(r.right() - 28.0 * u, r.y + 4.0 * u, 22.0 * u, 22.0 * u);
        draw_rectangle(c.x, c.y, c.w, c.h, if close_hover { Color::new(0.5, 0.15, 0.1, 1.0) } else { WOOD_LIGHT });
        draw_rectangle_lines(c.x, c.y, c.w, c.h, 1.0, GOLD);
        super::widgets::text_centered("x", c.x + c.w / 2.0, c.y + 16.0 * u, 18.0 * u, TEXT);
    }
}

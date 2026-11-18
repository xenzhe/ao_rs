//! Shared UI state and the window manager (P3 base, kept by the integrator).
//!
//! Every frame the UI runs BEFORE the world input: windows draw themselves, and whatever
//! the UI used this frame (mouse over a window, a focused text field, a drag in progress)
//! is reported in [`Ui::mouse_captured`] / [`Ui::keyboard_captured`] so the world does not
//! also react to it.
//!
//! A window is a [`WindowId`]; its content is drawn by `ui::<module>::draw` (registry in
//! `ui/mod.rs`). Windows can be dragged by their title bar, are brought to the front on
//! click, close with their X or their hotkey, and remember their position.
use super::theme::Theme;
use macroquad::prelude::*;
use std::collections::HashMap;

/// Every window of the game. OWNERS in docs/P3_CONTRATO.md.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum WindowId {
    Inventory,
    Bank,
    Shop,
    Trade,
    Spells,
    Character,
    Guild,
    Party,
    Quests,
    WorldMap,
    Options,
    // P4
    Crafting,
    Gm,
    Events,
    Auction,
    Travel,
    Mail,
    Surgeon,
    // TSAO progression (ui/tsao.rs)
    Exchange,
    Bonus,
    Poll,
}

impl WindowId {
    pub const ALL: [WindowId; 21] = [
        WindowId::Travel,
        WindowId::Mail,
        WindowId::Surgeon,
        WindowId::Exchange,
        WindowId::Bonus,
        WindowId::Poll,
        WindowId::Inventory,
        WindowId::Bank,
        WindowId::Shop,
        WindowId::Trade,
        WindowId::Spells,
        WindowId::Character,
        WindowId::Guild,
        WindowId::Party,
        WindowId::Quests,
        WindowId::WorldMap,
        WindowId::Options,
        WindowId::Crafting,
        WindowId::Gm,
        WindowId::Events,
        WindowId::Auction,
    ];

    pub fn title(self) -> &'static str {
        match self {
            WindowId::Inventory => "Inventario",
            WindowId::Bank => "Bóveda",
            WindowId::Shop => "Comerciante",
            WindowId::Trade => "Comercio",
            WindowId::Spells => "Hechizos",
            WindowId::Character => "Personaje",
            WindowId::Guild => "Clan",
            WindowId::Party => "Grupo",
            WindowId::Quests => "Misiones",
            WindowId::WorldMap => "Mapa del mundo",
            WindowId::Options => "Opciones",
            WindowId::Crafting => "Artesanía",
            WindowId::Gm => "Panel de GM",
            WindowId::Events => "Eventos",
            WindowId::Auction => "Subastas",
            WindowId::Travel => "Viajes",
            WindowId::Mail => "Correo",
            WindowId::Surgeon => "Cirujano",
            WindowId::Exchange => "Canje",
            WindowId::Bonus => "Bonificación de clase",
            WindowId::Poll => "Votación",
        }
    }

    /// Hotkey that toggles it (None: opened by the game, e.g. the bank by a banker).
    pub fn hotkey(self) -> Option<KeyCode> {
        use client_core::p3::keys::Action;
        let a = match self {
            WindowId::Inventory => Action::Inventory,
            WindowId::Spells => Action::Spells,
            WindowId::Character => Action::Character,
            WindowId::Guild => Action::Guild,
            WindowId::Party => Action::Party,
            WindowId::Quests => Action::Quests,
            WindowId::WorldMap => Action::WorldMap,
            WindowId::Options => Action::Options,
            WindowId::Events => Action::Events,
            WindowId::Auction => Action::Auction,
            WindowId::Gm => return Some(KeyCode::F10),
            _ => return None,
        };
        crate::keys::code(a)
    }

    /// Size of the window in UI units (multiplied by `u`), content included.
    pub fn default_size(self) -> Vec2 {
        // Item windows decide their own size (inventory.rs).
        if let Some(s) = super::inventory::wanted_size(self) {
            return s;
        }
        match self {
            WindowId::Inventory | WindowId::Bank | WindowId::Shop => vec2(330.0, 420.0),
            WindowId::Trade => vec2(560.0, 420.0),
            WindowId::Spells => vec2(720.0, 560.0),
            WindowId::Character => vec2(780.0, 580.0),
            WindowId::Guild => vec2(740.0, 580.0),
            WindowId::Party => vec2(380.0, 430.0),
            WindowId::Quests => vec2(780.0, 580.0),
            WindowId::WorldMap => vec2(800.0, 640.0),
            WindowId::Options => vec2(700.0, 600.0),
            WindowId::Crafting => vec2(760.0, 560.0),
            WindowId::Gm => vec2(720.0, 580.0),
            WindowId::Events => vec2(780.0, 580.0),
            WindowId::Auction => vec2(780.0, 560.0),
            WindowId::Travel => vec2(440.0, 560.0),
            WindowId::Mail => vec2(820.0, 560.0),
            WindowId::Surgeon => vec2(560.0, 480.0),
            WindowId::Exchange => vec2(780.0, 560.0),
            WindowId::Bonus => vec2(560.0, 400.0),
            WindowId::Poll => vec2(480.0, 420.0),
        }
    }
}

/// Something being dragged with the mouse (an inventory slot, a spell...). The owner of the
/// source puts it here on press; the owner of the target takes it on release.
#[derive(Clone, Debug, PartialEq)]
pub struct Drag {
    pub from: WindowId,
    /// Slot (1-based) or index inside `from`.
    pub index: u8,
    /// What it is (obj.dat id, spell id...), for drawing the icon under the cursor.
    pub what: u16,
    pub amount: u32,
}

struct Placed {
    pos: Vec2,
    z: u32,
}

pub struct Ui {
    pub theme: Theme,
    /// UI scale (from the layout).
    pub u: f32,
    open: HashMap<WindowId, Placed>,
    next_z: u32,
    moving: Option<(WindowId, Vec2)>,
    /// Text field with keyboard focus (any string id unique to the field).
    pub focus: Option<String>,
    pub drag: Option<Drag>,
    /// Set during the frame by everything that used the mouse / keyboard.
    pub mouse_captured: bool,
    pub keyboard_captured: bool,
    /// Rects of the open windows this frame (for hit tests of the next frame).
    hits: Vec<(WindowId, Rect)>,
    /// Server-driven windows currently opened for a server session (bank, shop, trade).
    pub server_windows: std::collections::HashSet<WindowId>,
    /// In the world (not the account screens): window hotkeys only work here.
    pub in_game: bool,
}

impl Ui {
    pub fn new(theme: Theme) -> Ui {
        Ui { theme, u: 1.0, open: HashMap::new(), next_z: 1, moving: None, focus: None, drag: None, mouse_captured: false, keyboard_captured: false, hits: Vec::new(), server_windows: Default::default(), in_game: false }
    }

    pub fn is_open(&self, w: WindowId) -> bool {
        self.open.contains_key(&w)
    }

    pub fn open(&mut self, w: WindowId) {
        let z = self.next_z;
        self.next_z += 1;
        let size = w.default_size() * self.u;
        let n = self.open.len() as f32;
        let pos = vec2((screen_width() - size.x) / 2.0 + n * 24.0 * self.u, (screen_height() - size.y) / 2.0 + n * 24.0 * self.u);
        self.open.entry(w).and_modify(|p| p.z = z).or_insert(Placed { pos, z });
    }

    pub fn close(&mut self, w: WindowId) {
        self.open.remove(&w);
    }

    pub fn toggle(&mut self, w: WindowId) {
        if self.is_open(w) {
            self.close(w)
        } else {
            self.open(w)
        }
    }

    /// Open windows, back to front.
    pub fn open_windows(&self) -> Vec<WindowId> {
        let mut v: Vec<_> = self.open.iter().map(|(w, p)| (p.z, *w)).collect();
        v.sort_by_key(|(z, _)| *z);
        v.into_iter().map(|(_, w)| w).collect()
    }

    /// Start of the frame: hotkeys and capture flags.
    pub fn begin_frame(&mut self, u: f32) {
        self.u = u;
        super::fx_ui::begin_frame();
        self.mouse_captured = self.drag.is_some() || self.moving.is_some() || self.hits.iter().any(|(_, r)| r.contains(mouse_position().into()));
        self.keyboard_captured = self.focus.is_some();
        self.hits.clear();
        if !self.keyboard_captured && self.in_game {
            for w in WindowId::ALL {
                if w.hotkey().is_some_and(is_key_pressed) {
                    self.toggle(w);
                }
            }
        }
        if is_mouse_button_pressed(MouseButton::Left) && !self.mouse_captured {
            // A click outside every window drops the text focus.
            self.focus = None;
        }
    }

    /// Draws the frame of window `w` (if open) and returns its content rect. Handles
    /// dragging by the title bar, bring-to-front and the close button.
    pub fn window(&mut self, w: WindowId) -> Option<Rect> {
        let u = self.u;
        let size = w.default_size() * u;
        let topmost_under_mouse = self.topmost_at(mouse_position().into());
        let p = self.open.get_mut(&w)?;
        let r = Rect::new(p.pos.x.clamp(0.0, (screen_width() - size.x).max(0.0)), p.pos.y.clamp(0.0, (screen_height() - size.y).max(0.0)), size.x, size.y);
        let title = Rect::new(r.x, r.y, r.w, 30.0 * u);
        // Matches `Theme::window_frame` (engraved X in the top-right corner).
        let close = Rect::new(r.right() - 30.0 * u, r.y + 8.0 * u, 22.0 * u, 22.0 * u);
        let m: Vec2 = mouse_position().into();
        let front = topmost_under_mouse == Some(w);
        if front && is_mouse_button_pressed(MouseButton::Left) {
            p.z = self.next_z;
            self.next_z += 1;
            if close.contains(m) {
                self.open.remove(&w);
                return None;
            }
            if title.contains(m) {
                self.moving = Some((w, m - vec2(r.x, r.y)));
            }
        }
        if let Some((mw, off)) = self.moving {
            if mw == w {
                if is_mouse_button_down(MouseButton::Left) {
                    self.open.get_mut(&w).unwrap().pos = m - off;
                } else {
                    self.moving = None;
                }
            }
        }
        self.theme.window_frame(r, w.title(), u, close.contains(m));
        self.hits.push((w, r));
        Some(Rect::new(r.x + 12.0 * u, title.bottom() + 8.0 * u, r.w - 24.0 * u, r.h - title.h - 20.0 * u))
    }

    /// The open window on top at screen point `p` (for drops and hover across windows).
    pub fn topmost_at(&self, p: Vec2) -> Option<WindowId> {
        self.open.iter().filter(|(w, pl)| Rect::new(pl.pos.x, pl.pos.y, w.default_size().x * self.u, w.default_size().y * self.u).contains(p)).max_by_key(|(_, pl)| pl.z).map(|(w, _)| *w)
    }

    /// Marks the mouse as used by the UI this frame (a HUD element, a popup...).
    pub fn capture_mouse_if_over(&mut self, r: Rect) {
        if r.contains(mouse_position().into()) {
            self.mouse_captured = true;
        }
    }
}

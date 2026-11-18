//! The player's key bindings (`client_core::p3::keys`) as macroquad key codes, kept in
//! memory and saved when the Options window changes one.
use client_core::p3::keys::{Action, KeyBinds};
use macroquad::prelude::*;
use std::cell::RefCell;

thread_local! {
    static BINDS: RefCell<KeyBinds> = RefCell::new(KeyBinds::load());
}

pub fn binds() -> KeyBinds {
    BINDS.with(|b| b.borrow().clone())
}

pub fn set_binds(k: KeyBinds) {
    k.save();
    BINDS.with(|b| *b.borrow_mut() = k);
}

pub fn code(a: Action) -> Option<KeyCode> {
    BINDS.with(|b| from_name(b.borrow().key(a)))
}

/// Held (movement).
pub fn down(a: Action) -> bool {
    match code(a) {
        // Either Control counts for the attack.
        Some(KeyCode::LeftControl | KeyCode::RightControl) => is_key_down(KeyCode::LeftControl) || is_key_down(KeyCode::RightControl),
        Some(k) => is_key_down(k),
        None => false,
    }
}

/// Pressed this frame.
pub fn pressed(a: Action) -> bool {
    code(a).is_some_and(is_key_pressed)
}

/// The key pressed this frame, if any (for rebinding). Escape cancels.
pub fn any_pressed() -> Option<KeyCode> {
    ALL.iter().copied().find(|k| is_key_pressed(*k))
}

const ALL: [KeyCode; 70] = [
    KeyCode::A, KeyCode::B, KeyCode::C, KeyCode::D, KeyCode::E, KeyCode::F, KeyCode::G, KeyCode::H, KeyCode::I, KeyCode::J, KeyCode::K, KeyCode::L, KeyCode::M,
    KeyCode::N, KeyCode::O, KeyCode::P, KeyCode::Q, KeyCode::R, KeyCode::S, KeyCode::T, KeyCode::U, KeyCode::V, KeyCode::W, KeyCode::X, KeyCode::Y, KeyCode::Z,
    KeyCode::Key0, KeyCode::Key1, KeyCode::Key2, KeyCode::Key3, KeyCode::Key4, KeyCode::Key5, KeyCode::Key6, KeyCode::Key7, KeyCode::Key8, KeyCode::Key9,
    KeyCode::F1, KeyCode::F2, KeyCode::F3, KeyCode::F4, KeyCode::F5, KeyCode::F6, KeyCode::F7, KeyCode::F8, KeyCode::F9, KeyCode::F10, KeyCode::F11, KeyCode::F12,
    KeyCode::Up, KeyCode::Down, KeyCode::Left, KeyCode::Right, KeyCode::Space, KeyCode::Tab, KeyCode::LeftControl, KeyCode::RightControl, KeyCode::LeftShift,
    KeyCode::RightShift, KeyCode::LeftAlt, KeyCode::Home, KeyCode::End, KeyCode::PageUp, KeyCode::PageDown, KeyCode::Insert, KeyCode::Delete, KeyCode::KpMultiply,
    KeyCode::KpAdd, KeyCode::KpSubtract, KeyCode::Comma, KeyCode::Period,
];

pub fn from_name(s: &str) -> Option<KeyCode> {
    ALL.iter().copied().find(|k| format!("{k:?}") == s)
}

pub fn name(k: KeyCode) -> String {
    format!("{k:?}")
}

/// Short label for a key cap.
pub fn label(s: &str) -> String {
    match s {
        "Up" => "↑".into(),
        "Down" => "↓".into(),
        "Left" => "←".into(),
        "Right" => "→".into(),
        "LeftControl" | "RightControl" => "Ctrl".into(),
        "LeftShift" | "RightShift" => "Shift".into(),
        "LeftAlt" => "Alt".into(),
        "Space" => "Espacio".into(),
        "KpMultiply" => "*".into(),
        s if s.starts_with("Key") => s[3..].into(),
        s => s.into(),
    }
}

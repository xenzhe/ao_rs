//! Visual consequences of server messages, for the frontend to render. Positions are
//! computed when the message arrives (a despawned entity is gone right after).
use ao_rules::spells::SpellEffect;
use std::ops::{Add, Sub};

/// A point in world pixels.
#[derive(Clone, Copy, PartialEq, Debug, Default)]
pub struct Pt {
    pub x: f32,
    pub y: f32,
}

pub const fn pt(x: f32, y: f32) -> Pt {
    Pt { x, y }
}

impl Add for Pt {
    type Output = Pt;
    fn add(self, o: Pt) -> Pt {
        pt(self.x + o.x, self.y + o.y)
    }
}

impl Sub for Pt {
    type Output = Pt;
    fn sub(self, o: Pt) -> Pt {
        pt(self.x - o.x, self.y - o.y)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum ClientEvent {
    /// A new map was loaded into `Game::map` (the frontend preloads its textures).
    MapEntered { map: u16 },
    /// Something appeared.
    Puff { at: Pt },
    /// A creature died.
    DeathBurst { at: Pt },
    /// Someone was hurt.
    Blood { at: Pt },
    /// Health change over someone's head (negative = damage).
    FloatText { at: Pt, amount: i32 },
    /// A spell flew from `from` to `to`.
    Effect { effect: SpellEffect, from: Pt, to: Pt },
}

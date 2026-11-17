//! A composed character: shadow, body, head and visible equipment (weapon, shield,
//! helmet) in the order of the official `Char_Render` for each heading, death/ghost and
//! status visuals, and the official FX (FXs.ini) playing on it. OWNER: Opus D
//! (docs/P3_CONTRATO.md).
//!
//! `draw_character` gets no entity id (its signature belongs to the world renderer), so
//! `game::poll` publishes a per-frame [`CharFrame`]: which id each `Ent` is (by address;
//! the entity map is not restructured between `poll` and drawing), the local player id
//! and the FX playing on each entity.
use crate::assets::Assets;
use crate::game::{v, Ent};
use ao_formats::graphics::{anchor, Grh};
use ao_protocol::status;
use client_core::p3::fx::{draw_order, Anim, EquipAnim, EquipDb, FxDef, Layer, FX_FOREVER, STRIP_FRAME_MS};
use macroquad::prelude::*;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

/// An official FX playing on a character.
#[derive(Clone, Copy, Debug)]
pub struct CharFx {
    pub def: FxDef,
    pub loops: u8,
    pub started_ms: f32,
}

#[derive(Default)]
pub struct CharFrame {
    pub ids: HashMap<usize, u32>,
    pub my_id: u32,
    pub fx: HashMap<u32, Vec<CharFx>>,
    pub equip: Option<Rc<EquipDb>>,
    pub additive: Option<Material>,
    /// `AO_EQUIP=weapon,shield,helmet`: local override for screenshots (own character).
    pub equip_override: Option<(u16, u16, u16)>,
}

thread_local! {
    pub static FRAME: RefCell<CharFrame> = RefCell::new(CharFrame::default());
}

/// Address key of an entity for [`CharFrame::ids`].
pub fn key(e: &Ent) -> usize {
    e as *const Ent as usize
}

/// Total time of an FX GRH animation (one loop), in ms.
fn fx_loop_ms(assets: &Assets, grh: u32) -> f32 {
    assets.db.grh(grh).map(|g| g.speed).filter(|s| *s > 0.0).unwrap_or(600.0)
}

/// Drops finished FX (played `loops + 1` times). Called once per frame from `poll`.
pub fn expire_fx(assets: &Assets, fx: &mut HashMap<u32, Vec<CharFx>>, now_ms: f32) {
    for list in fx.values_mut() {
        list.retain(|f| f.loops == FX_FOREVER || now_ms - f.started_ms < fx_loop_ms(assets, f.def.grh) * (f.loops as f32 + 1.0));
    }
    fx.retain(|_, l| !l.is_empty());
}

fn tint_for(flags: u32, me: bool) -> Option<Color> {
    if flags & (status::INVISIBLE | status::HIDDEN) != 0 {
        // Others never see an invisible/hidden character; you see yourself faded.
        return me.then_some(Color::new(1.0, 1.0, 1.0, 0.35));
    }
    if flags & status::DEAD != 0 {
        return Some(Color::new(0.75, 0.82, 1.0, 0.55));
    }
    if flags & (status::PARALYZED | status::IMMOBILIZED) != 0 {
        return Some(Color::new(0.6, 0.75, 1.0, 1.0));
    }
    if flags & status::POISONED != 0 {
        return Some(Color::new(0.65, 1.0, 0.6, 1.0));
    }
    Some(WHITE)
}

/// Draws one direction of an equipment animation anchored like `Draw_Grh(.., center=1)`.
fn draw_anim(assets: &mut Assets, a: Anim, x: f32, y: f32, t_ms: f32, tint: Color) {
    match a {
        Anim::None => {}
        Anim::Grh(g) => assets.draw(g, x, y, true, t_ms, tint),
        Anim::Strip { file, x: sx, y: sy, w, h, n } => {
            let f = if n > 1 { ((t_ms / STRIP_FRAME_MS) as u32) % n } else { 0 };
            let g = Grh { frames: vec![], file, sx: sx + f as f32 * w, sy, w, h, speed: 0.0 };
            let (dx, dy) = anchor(&g, x, y, true);
            if let Some(tex) = assets.texture(file) {
                draw_texture_ex(&tex, dx.round(), dy.round(), tint, DrawTextureParams { source: Some(Rect::new(g.sx, g.sy, w, h)), ..Default::default() });
            }
        }
    }
}

fn pick(table: Option<&HashMap<u16, EquipAnim>>, id: u16, heading: usize) -> Anim {
    if id == 0 {
        return Anim::None;
    }
    table.and_then(|t| t.get(&id)).map(|a| a[heading]).unwrap_or(Anim::None)
}

/// Body, head, helmet, weapon and shield at the character's tile, then its FX.
pub fn draw_character(assets: &mut Assets, e: &Ent, origin: Vec2, t_ms: f32) {
    FRAME.with(|f| draw_with(assets, e, origin, t_ms, &f.borrow()));
}

fn draw_with(assets: &mut Assets, e: &Ent, origin: Vec2, t_ms: f32, frame: &CharFrame) {
    let id = frame.ids.get(&key(e)).copied();
    let me = id.is_some_and(|i| i == frame.my_id);
    let Some(tint) = tint_for(e.status, me) else { return };
    let tl = v(e.tile_tl()) - origin;
    let heading = e.dir.heading();
    let Some(body) = assets.db.bodies.get(&e.body).cloned() else { return };
    let dead = e.status & status::DEAD != 0;
    let frozen = e.status & (status::PARALYZED | status::IMMOBILIZED) != 0;
    if !dead {
        draw_ellipse(tl.x + 16.0, tl.y + 29.0, 11.0, 4.0, 0.0, Color::new(0.0, 0.0, 0.0, 0.3 * tint.a));
    }
    // Walk animation only while moving (Char_Render stops the GRHs when idle).
    let anim_t = if e.moving() && !frozen { e.walk_ms } else { 0.0 };
    // Ghosts float a little.
    let bob = if dead { ((t_ms * 0.004).sin() * 2.0).round() } else { 0.0 };
    let b = tl + vec2(body.body_offset.0, body.body_offset.1 + bob);
    let (mut weapon, mut shield, mut helmet) = (e.weapon, e.shield, e.helmet);
    if me {
        if let Some(o) = frame.equip_override {
            (weapon, shield, helmet) = o;
        }
    }
    let eq = frame.equip.as_deref();
    let head = assets.db.heads.get(&e.head).map(|h| h[heading]).unwrap_or(0);
    let (hx, hy) = (b.x + body.head_offset.0, b.y + body.head_offset.1 - 1.0);
    for layer in draw_order(heading) {
        match layer {
            Layer::Body => assets.draw(body.walk[heading], b.x, b.y, true, anim_t, tint),
            Layer::Head if head > 0 => assets.draw(head, hx, hy, true, 0.0, tint),
            Layer::Helmet => draw_anim(assets, pick(eq.map(|d| &d.helmets), helmet, heading), hx, hy, 0.0, tint),
            Layer::Weapon => draw_anim(assets, pick(eq.map(|d| &d.weapons), weapon, heading), b.x, b.y, anim_t, tint),
            Layer::Shield => draw_anim(assets, pick(eq.map(|d| &d.shields), shield, heading), b.x, b.y, anim_t, tint),
            _ => {}
        }
    }
    // Official FX over the character: (tile + offset + 20 px), alpha 220, additive unless PNG.
    if let Some(list) = id.and_then(|i| frame.fx.get(&i)) {
        for f in list.iter().filter(|f| f.started_ms <= t_ms) {
            let (x, y) = (tl.x + f.def.offset.0, tl.y + f.def.offset.1 + 20.0);
            let c = Color::new(1.0, 1.0, 1.0, 220.0 / 255.0 * tint.a.max(0.5));
            let add = !f.def.png;
            if add {
                if let Some(m) = &frame.additive {
                    gl_use_material(m);
                }
            }
            assets.draw(f.def.grh, x, y, true, t_ms - f.started_ms, c);
            if add {
                gl_use_default_material();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_tints() {
        assert_eq!(tint_for(status::INVISIBLE, false), None);
        assert!(tint_for(status::INVISIBLE, true).unwrap().a < 0.5);
        assert!(tint_for(status::DEAD, false).unwrap().a < 1.0);
        assert_eq!(tint_for(0, false), Some(WHITE));
    }
}

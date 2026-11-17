//! A composed character: shadow, body, head and (P3) visible equipment (weapon, shield,
//! helmet), death/ghost and status visuals. OWNER: Opus D (docs/P3_CONTRATO.md).
use crate::assets::Assets;
use crate::game::{v, Ent};
use macroquad::prelude::*;

/// Body + head at the character's tile, as the official composed character.
pub fn draw_character(assets: &mut Assets, e: &Ent, origin: Vec2, t_ms: f32) {
    let _ = t_ms;
    let tl = v(e.tile_tl()) - origin;
    let heading = e.dir.heading();
    let Some(body) = assets.db.bodies.get(&e.body).cloned() else { return };
    draw_ellipse(tl.x + 16.0, tl.y + 29.0, 11.0, 4.0, 0.0, Color::new(0.0, 0.0, 0.0, 0.3));
    let anim_t = if e.moving() { e.walk_ms } else { 0.0 };
    let b = tl + vec2(body.body_offset.0, body.body_offset.1);
    assets.draw(body.walk[heading], b.x, b.y, true, anim_t, WHITE);
    if let Some(head) = assets.db.heads.get(&e.head).map(|h| h[heading]) {
        if head > 0 {
            assets.draw(head, b.x + body.head_offset.0, b.y + body.head_offset.1 - 1.0, true, 0.0, WHITE);
        }
    }
}

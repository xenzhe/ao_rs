//! The account screens happen in the world (docs/ESTILO_UI.md §1.8): login, character
//! selection and creation are drawn over a live scene of an official map, with actors that
//! walk and turn, the map's own particles and lights, and a free cinematic camera.
//! OWNER: account screens (`login.rs`, `account.rs`).
//!
//! Before entering the game there is no game server, so the scene keeps its own
//! `client_core::Game` (only `map` and `ents`) with hand-placed actors. It draws with the
//! same pieces as `Renderer::draw_world` (`render::world`, `render::lighting`, the shared
//! `Fx` for particles) but with its own render targets and camera:
//! - the targets have the size of the official view and are never resized; the camera can
//!   sit at any sub-pixel position and zoom in through the blit's source rectangle;
//! - the world is drawn at an integer origin and the blit shifts the fraction, so a slow
//!   travelling never stutters;
//! - no tile cursor, no names, no window border (what the game view adds for play).
//!
//! `AO_SCENE_SCOUT=x,y` (development): a zoomed-out view of the scene map centred on that
//! tile, with the tile grid numbered, to pick spots for the stages.
use crate::assets::Assets;
use crate::game::{v, Ent, Game};
use crate::render::characters::{key as ent_key, FRAME};
use crate::render::fx::{Fx, PlacedGroup};
use crate::render::layout::{RT_H, RT_W, TILE};
use crate::render::world::{self, WorldFades};
use crate::render::{lighting, Renderer};
use ao_formats::AoMap;
use ao_protocol::{status, EntityKind};
use ao_rules::Dir;
use client_core::p3::fx::ParticleGroup;
use client_core::p3::world::ContinuousWorld;
use macroquad::prelude::*;
use std::path::PathBuf;

/// Ullathorpe: town, plaza and the woods around it.
pub const SCENE_MAP: u16 = 1;
/// Id of the invisible camera anchor (`game.me()` when no actor has the focus).
const ANCHOR_ID: u32 = 1;
/// Scene actor ids start here (far from anything the real game uses).
pub const ACTOR_BASE: u32 = 0x7F00_0000;
/// The bonfire object of obj.dat (OBJ63 "Fogata").
const BONFIRE_GRH: u32 = 12731;
/// Particle stream of a burning torch/fire in particles.ind (used around Ullathorpe).
const FIRE_STREAM: usize = 235;
const SMOKE_STREAM: usize = 303;

/// How an actor behaves.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Act {
    /// Stands still facing `Dir`, turns now and then to look around.
    Idle(Dir),
    /// Stands facing `Dir` and never turns.
    Still(Dir),
    /// Walks in place (the walk cycle without moving), facing `Dir`.
    InPlace(Dir),
    /// Wanders within `radius` tiles of its home, pausing between steps.
    Wander(i32),
    /// Walks to `to` (greedily, waiting when someone is in the way) and then stands facing
    /// `face`, glancing around when `idle`.
    Go { to: (i32, i32), face: Dir, idle: bool },
}

/// What an actor looks like.
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct Look {
    pub body: u16,
    pub head: u16,
    pub weapon: u16,
    pub shield: u16,
    pub helmet: u16,
}

struct Actor {
    id: u32,
    home: (i32, i32),
    act: Act,
    next: f64,
}

/// Camera goal.
#[derive(Clone, Copy, Debug)]
pub struct Shot {
    /// World pixel at the centre of the screen.
    pub at: Vec2,
    /// 1 = base scale; > 1 closer.
    pub zoom: f32,
}

/// A slow travelling along waypoints (world pixels), looping.
struct Travel {
    points: Vec<Vec2>,
    speed: f32,
    /// Distance travelled along the loop.
    s: f32,
}

pub struct Scene {
    recursos: PathBuf,
    game: Game,
    cw: Option<ContinuousWorld>,
    fades: WorldFades,
    world_rt: RenderTarget,
    light_rt: RenderTarget,
    loaded: Option<bool>,
    actors: Vec<Actor>,
    /// Camera centre (world px) and zoom, eased towards the goal.
    cam: Vec2,
    zoom: f32,
    goal: Shot,
    travel: Option<Travel>,
    /// How fast the camera follows (1/s).
    pub follow: f32,
    hour: f32,
    pub hour_goal: f32,
    /// Fade to black for cuts (0 clear .. 1 black) and the pending jump.
    black: f32,
    cut_to: Option<Shot>,
    /// Bonfires placed by the scene: tile, and whether its particles are in the Fx.
    fires: Vec<(i32, i32)>,
    /// Particle groups this scene put in `fx.map_groups` (fingerprint: count and first anchor).
    fx_print: Option<(usize, Vec2)>,
    /// Extra soft lights this frame (world px, radius, colour), from the screens.
    lights: Vec<(Vec2, f32, Color)>,
    /// Actor that is `game.me()` (torch light, trees fading in front of it).
    pub me: Option<u32>,
    scout: Option<(i32, i32)>,
    /// Draw time statistics (ms), for the report.
    perf: (f64, u32),
    born: f64,
    /// Set by the screens while a Play request is out (see `sync_fx`).
    pub hold_fx: bool,
    rng: u32,
}

fn new_targets() -> (RenderTarget, RenderTarget) {
    let w = render_target(RT_W as u32, RT_H as u32);
    w.texture.set_filter(FilterMode::Nearest);
    let l = render_target(RT_W as u32, RT_H as u32);
    l.texture.set_filter(FilterMode::Linear);
    (w, l)
}

fn rt_camera(rt: &RenderTarget, size: Vec2) -> Camera2D {
    Camera2D { zoom: vec2(2.0 / size.x, 2.0 / size.y), target: size / 2.0, render_target: Some(rt.clone()), ..Default::default() }
}

/// Feet of tile (x, y) in world pixels (as `Ent::world_pos`).
pub fn tile_feet(x: i32, y: i32) -> Vec2 {
    vec2((x - 1) as f32 * TILE + 16.0, (y - 1) as f32 * TILE + 28.0)
}

impl Scene {
    pub fn new(recursos: PathBuf) -> Scene {
        let (world_rt, light_rt) = new_targets();
        let scout = std::env::var("AO_SCENE_SCOUT").ok().and_then(|s| {
            let (x, y) = s.split_once(',')?;
            Some((x.trim().parse().ok()?, y.trim().parse().ok()?))
        });
        Scene {
            game: Game::new(String::new(), String::new(), recursos.clone()),
            recursos,
            cw: None,
            fades: WorldFades::default(),
            world_rt,
            light_rt,
            loaded: None,
            actors: Vec::new(),
            cam: Vec2::ZERO,
            zoom: 1.0,
            goal: Shot { at: Vec2::ZERO, zoom: 1.0 },
            travel: None,
            follow: 1.6,
            hour: 19.0,
            hour_goal: 19.0,
            black: 1.0,
            cut_to: None,
            fires: Vec::new(),
            fx_print: None,
            lights: Vec::new(),
            me: None,
            scout,
            perf: (0.0, 0),
            born: get_time(),
            hold_fx: false,
            rng: 0x2545_F491,
        }
    }

    /// The map loaded and the scene can be drawn (false: draw a plain backdrop instead).
    pub fn ready(&self) -> bool {
        self.loaded == Some(true)
    }

    fn load(&mut self, assets: &mut Assets) {
        if self.loaded.is_some() {
            return;
        }
        self.recursos = assets.root.clone();
        match AoMap::load(&self.recursos, SCENE_MAP) {
            Ok(m) => {
                assets.preload_map(&m);
                self.game.map = Some(m);
                self.loaded = Some(true);
                // Invisible anchor: keeps a `me` for the code that needs one.
                let mut a = Ent::new(String::new(), EntityKind::Creature, 1, 0, 2, 2, Dir::Down, 1, 1);
                a.status = status::INVISIBLE;
                self.game.ents.insert(ANCHOR_ID, a);
                self.game.my_id = ANCHOR_ID;
            }
            Err(e) => {
                eprintln!("scene: map {SCENE_MAP} not loaded: {e:?}");
                self.loaded = Some(false);
            }
        }
    }

    pub fn map(&self) -> Option<&AoMap> {
        self.game.map.as_ref()
    }

    fn rand(&mut self) -> u32 {
        self.rng ^= self.rng << 13;
        self.rng ^= self.rng >> 17;
        self.rng ^= self.rng << 5;
        self.rng
    }

    // -------------------------------------------------------------- camera

    /// Moves the camera there smoothly (a cut through black when it is far away).
    pub fn shot(&mut self, s: Shot) {
        self.travel = None;
        // Across the town it flies (a travelling over the roofs); only very far it cuts.
        let far = self.cam.distance(s.at) > 70.0 * TILE;
        if far && self.loaded == Some(true) && self.cam != Vec2::ZERO {
            self.cut_to = Some(s);
        } else if self.cam == Vec2::ZERO {
            self.cam = s.at;
            self.zoom = s.zoom;
        }
        self.goal = s;
    }

    /// Places the camera there at once (first frame of a screen).
    pub fn jump(&mut self, s: Shot) {
        self.travel = None;
        self.cam = s.at;
        self.zoom = s.zoom;
        self.goal = s;
        self.cut_to = None;
    }

    /// Places the camera there at once only when it is far (a fresh scene); otherwise the
    /// camera glides there.
    pub fn jump_if_far(&mut self, s: Shot) {
        if self.cam == Vec2::ZERO || self.cam.distance(s.at) > 26.0 * TILE {
            self.jump(s);
        } else {
            self.aim(s);
        }
    }

    /// A looping travelling along waypoints in tiles, `speed` in pixels per second.
    pub fn travel(&mut self, tiles: &[(f32, f32)], speed: f32, zoom: f32) {
        let points: Vec<Vec2> = tiles.iter().map(|(x, y)| vec2((x - 1.0) * TILE + 16.0, (y - 1.0) * TILE + 16.0)).collect();
        let Some(first) = points.first().copied() else { return };
        if self.cam == Vec2::ZERO {
            self.cam = first;
            self.zoom = zoom;
        } else if self.cam.distance(first) > 70.0 * TILE {
            self.cut_to = Some(Shot { at: first, zoom });
        }
        self.goal = Shot { at: first, zoom };
        self.travel = Some(Travel { points, speed, s: 0.0 });
    }

    /// Sets the focus goal without cancelling a travelling's speed (for hover changes).
    pub fn aim(&mut self, s: Shot) {
        self.travel = None;
        self.goal = s;
    }

    /// Integer world scale for this screen: the visible area always fits in the target
    /// with one tile to spare, and each world pixel covers whole screen pixels.
    pub fn base_scale() -> f32 {
        let (sw, sh) = (screen_width(), screen_height());
        (sw / (RT_W - 2.0 * TILE)).max(sh / (RT_H - 2.0 * TILE)).ceil().max(1.0)
    }

    fn scale(&self) -> f32 {
        if self.scout.is_some() {
            return 1.0;
        }
        Self::base_scale() * self.zoom
    }

    /// Screen position of a world pixel.
    pub fn to_screen(&self, w: Vec2) -> Vec2 {
        vec2(screen_width(), screen_height()) / 2.0 + (w - self.cam) * self.scale()
    }

    /// Screen scale of one world pixel (for sizing things drawn over actors).
    pub fn pixel(&self) -> f32 {
        self.scale()
    }

    fn update_camera(&mut self, dt: f32) {
        // Cut through black.
        if let Some(to) = self.cut_to {
            self.black = (self.black + dt / 0.28).min(1.0);
            if self.black >= 1.0 {
                self.cam = to.at;
                self.zoom = to.zoom;
                self.cut_to = None;
            }
            return;
        }
        self.black = (self.black - dt / 0.55).max(0.0);
        if let Some(t) = &mut self.travel {
            t.s += t.speed * dt;
            let n = t.points.len();
            let total: f32 = (0..n).map(|i| t.points[i].distance(t.points[(i + 1) % n])).sum();
            if total > 0.0 {
                let mut s = t.s.rem_euclid(total);
                for i in 0..n {
                    let (a, b) = (t.points[i], t.points[(i + 1) % n]);
                    let d = a.distance(b);
                    if s <= d {
                        self.goal.at = a.lerp(b, if d > 0.0 { s / d } else { 0.0 });
                        break;
                    }
                    s -= d;
                }
            }
        }
        let k = 1.0 - (-dt * self.follow).exp();
        self.cam += (self.goal.at - self.cam) * k;
        self.zoom += (self.goal.zoom - self.zoom) * k;
        let kh = 1.0 - (-dt * 0.8).exp();
        self.hour += (self.hour_goal - self.hour) * kh;
    }

    // -------------------------------------------------------------- actors

    /// Whether an actor may stand on (x, y): on the map, not water, free and, when `strict`,
    /// not blocked (scripted walks ignore the blocks: the camp is on decorative land).
    pub fn walkable(&self, x: i32, y: i32, except: u32, strict: bool) -> bool {
        let Some(m) = &self.game.map else { return false };
        let b = m.blocked(x, y);
        if !ao_formats::csm::on_map(x, y) || b & ao_formats::csm::FLAG_WATER != 0 || (strict && b != 0) {
            return false;
        }
        if self.fires.contains(&(x, y)) {
            return false;
        }
        !self.game.ents.iter().any(|(id, e)| *id != except && *id != ANCHOR_ID && ((e.x, e.y) == (x, y) || (e.px, e.py) == (x, y) && e.moving()))
    }

    /// Adds or updates actor `id` at its home tile (an existing actor keeps walking where
    /// it is unless `home` changed).
    pub fn put(&mut self, id: u32, look: Look, home: (i32, i32), act: Act) {
        let dir = match act {
            Act::Idle(d) | Act::Still(d) | Act::InPlace(d) => d,
            _ => Dir::Down,
        };
        if let Some(a) = self.actors.iter_mut().find(|a| a.id == id) {
            let moved = a.home != home;
            a.home = home;
            if a.act != act {
                a.act = act;
                a.next = 0.0;
            }
            if let Some(e) = self.game.ents.get_mut(&id) {
                (e.body, e.head, e.weapon, e.shield, e.helmet) = (look.body, look.head, look.weapon, look.shield, look.helmet);
                if moved {
                    e.snap(home.0, home.1);
                    e.dir = dir;
                }
                if matches!(act, Act::Still(_) | Act::InPlace(_)) {
                    e.dir = dir;
                }
            }
            return;
        }
        let mut e = Ent::new(String::new(), EntityKind::Creature, look.body, look.head, home.0, home.1, dir, 1, 1);
        (e.weapon, e.shield, e.helmet) = (look.weapon, look.shield, look.helmet);
        self.game.ents.insert(id, e);
        let next = get_time() + (self.rand() % 3000) as f64 / 1000.0;
        self.actors.push(Actor { id, home, act, next });
    }

    pub fn remove(&mut self, id: u32) {
        self.actors.retain(|a| a.id != id);
        self.game.ents.remove(&id);
    }

    /// Removes every actor whose id is in `range`.
    pub fn remove_range(&mut self, range: std::ops::Range<u32>) {
        self.actors.retain(|a| !range.contains(&a.id));
        self.game.ents.retain(|id, _| !range.contains(id));
    }

    pub fn has(&self, id: u32) -> bool {
        self.game.ents.contains_key(&id)
    }

    /// Feet of an actor in world pixels.
    pub fn feet(&self, id: u32) -> Option<Vec2> {
        self.game.ents.get(&id).map(|e| v(e.world_pos()))
    }

    pub fn ent_mut(&mut self, id: u32) -> Option<&mut Ent> {
        self.game.ents.get_mut(&id)
    }

    /// Places a bonfire (sprite, fire and smoke particles, warm light) on a tile.
    pub fn bonfire(&mut self, x: i32, y: i32) {
        if !self.fires.contains(&(x, y)) {
            self.fires.push((x, y));
            self.fx_print = None;
        }
    }

    /// Adds a soft light for this frame only (world px).
    pub fn light(&mut self, at: Vec2, radius: f32, c: Color) {
        self.lights.push((at, radius, c));
    }

    fn update_actors(&mut self, dt: f32, now: f64) {
        for i in 0..self.actors.len() {
            let (id, act, home, next) = {
                let a = &self.actors[i];
                (a.id, a.act, a.home, a.next)
            };
            // Walk progress, like `Game::update`.
            if let Some(e) = self.game.ents.get_mut(&id) {
                if let Act::InPlace(d) = act {
                    e.dir = d;
                    e.px = e.x;
                    e.py = e.y;
                    e.t = 0.5;
                    e.walk_ms += dt * 1000.0;
                    continue;
                }
                if e.t < 1.0 {
                    e.t += dt * 1000.0 / ao_rules::WALK_MS as f32;
                    e.walk_ms += dt * 1000.0;
                    if e.t > 0.9999 {
                        e.t = 1.0;
                    }
                    continue;
                }
                e.walk_ms = 0.0;
            }
            if now < next {
                continue;
            }
            let Some((ex, ey)) = self.game.ents.get(&id).map(|e| (e.x, e.y)) else { continue };
            let r = self.rand();
            let mut wait = 0.6 + (r % 2600) as f64 / 1000.0;
            match act {
                Act::Still(d) => {
                    if let Some(e) = self.game.ents.get_mut(&id) {
                        e.dir = d;
                    }
                    wait = 1.0;
                }
                Act::InPlace(_) => {}
                Act::Idle(d) => {
                    // Mostly faces its direction; sometimes glances to a side.
                    let look = match r % 5 {
                        0 => side(d, true),
                        1 => side(d, false),
                        _ => d,
                    };
                    if let Some(e) = self.game.ents.get_mut(&id) {
                        e.dir = look;
                    }
                    wait += 1.5;
                }
                Act::Wander(radius) => {
                    // A few steps in one direction, then a pause.
                    let d = Dir::ALL[(r % 4) as usize];
                    let (dx, dy) = d.delta();
                    let (nx, ny) = (ex + dx, ey + dy);
                    let inside = (nx - home.0).abs() <= radius && (ny - home.1).abs() <= radius;
                    if inside && self.walkable(nx, ny, id, true) {
                        if let Some(e) = self.game.ents.get_mut(&id) {
                            e.dir = d;
                            e.step_to(nx, ny);
                        }
                        wait = if r.is_multiple_of(3) { 1.2 + (r % 1800) as f64 / 1000.0 } else { 0.0 };
                    } else {
                        wait = 0.3;
                    }
                }
                Act::Go { to, face, idle } => {
                    if (ex, ey) == to {
                        let look = if idle && r.is_multiple_of(4) { side(face, r.is_multiple_of(8)) } else { face };
                        if let Some(e) = self.game.ents.get_mut(&id) {
                            e.dir = look;
                        }
                        wait = if idle { wait + 1.5 } else { 0.25 };
                    } else {
                        // The longer axis first, the other one if that is taken.
                        let (dx, dy) = (to.0 - ex, to.1 - ey);
                        let hx = if dx > 0 { Dir::Right } else { Dir::Left };
                        let vy = if dy > 0 { Dir::Down } else { Dir::Up };
                        let tries = if dx.abs() >= dy.abs() { [(dx != 0).then_some(hx), (dy != 0).then_some(vy)] } else { [(dy != 0).then_some(vy), (dx != 0).then_some(hx)] };
                        wait = 0.35;
                        // Blocked both ways (two walkers face to face): a side step.
                        let sidestep = Dir::ALL[(r % 4) as usize];
                        for d in tries.into_iter().flatten().chain(std::iter::once(sidestep)) {
                            let (sx, sy) = d.delta();
                            if self.walkable(ex + sx, ey + sy, id, false) {
                                if let Some(e) = self.game.ents.get_mut(&id) {
                                    e.dir = d;
                                    e.step_to(ex + sx, ey + sy);
                                }
                                wait = 0.0;
                                break;
                            }
                        }
                    }
                }
            }
            self.actors[i].next = now + wait;
        }
    }

    // -------------------------------------------------------------- drawing

    /// Keeps the scene's particle groups (map + bonfires) in the shared `Fx`. The game
    /// replaces `fx.map_groups` when it enters a map, so this checks a fingerprint.
    fn sync_fx(&mut self, fx: &mut Fx, assets: &mut Assets) {
        fx.ensure_official(&assets.root);
        // Entering the world: the game's map owns the particles and the music now.
        if self.hold_fx {
            return;
        }
        let print = (fx.map_groups.len(), fx.map_groups.first().map(|g| g.at).unwrap_or(Vec2::NAN));
        if self.fx_print.is_some_and(|p| p.0 == print.0 && (p.1 == print.1 || p.1.is_nan() && print.1.is_nan())) {
            return;
        }
        let Some(map) = &self.game.map else { return };
        fx.load_map(map, &assets.db);
        if let Some(o) = &fx.official {
            for (i, (x, y)) in self.fires.iter().enumerate() {
                for (k, stream) in [FIRE_STREAM, SMOKE_STREAM].into_iter().enumerate() {
                    let Some(def) = o.streams.get(stream - 1) else { continue };
                    let at = vec2((x - 1) as f32 * TILE + 15.0, (y - 1) as f32 * TILE + 15.0 - k as f32 * 18.0);
                    fx.map_groups.push(PlacedGroup { group: ParticleGroup::new(stream, def, -1, 0x51ED_0000 ^ (i as u32 * 31 + k as u32)), at, roofed: false });
                }
            }
        }
        let streams: Vec<usize> = fx.map_groups.iter().map(|g| g.group.stream).collect();
        for s in streams {
            fx.resolve_stream(assets, s);
        }
        // The map's own music (Ullathorpe) while the account screens are open.
        let music = client_core::p3::fx::map_music(&assets.root, SCENE_MAP).unwrap_or(0);
        if let Some(a) = &mut fx.audio {
            a.music(music);
        }
        self.fx_print = Some((fx.map_groups.len(), fx.map_groups.first().map(|g| g.at).unwrap_or(Vec2::NAN)));
    }

    /// Plays an official FX (FXs.ini id) on an actor; `FX_FOREVER` loops until stopped.
    pub fn play_fx(&mut self, renderer: &mut Renderer, assets: &Assets, id: u32, fx: u16, loops: u8) {
        renderer.fx.char_fx(assets, id, fx, loops);
    }

    pub fn stop_fx(&mut self, renderer: &mut Renderer, id: u32) {
        renderer.fx.char_fx.remove(&id);
    }

    /// A short burst of light and sparks at a world pixel (confirmations).
    pub fn burst(&mut self, renderer: &mut Renderer, at: Vec2) {
        renderer.fx.cast(ao_rules::spells::SpellEffect::Heal, at, at);
    }

    /// Updates and draws the scene over the whole screen. Returns false when the map is
    /// not available (the caller draws a plain backdrop).
    pub fn draw(&mut self, renderer: &mut Renderer, assets: &mut Assets, dt: f32) -> bool {
        self.load(assets);
        if self.loaded != Some(true) {
            self.lights.clear();
            return false;
        }
        let t0 = get_time();
        let now = get_time();
        let t_ms = (now * 1000.0) as f32;
        self.sync_fx(&mut renderer.fx, assets);
        renderer.fx.update(dt);
        self.update_actors(dt, now);
        self.update_camera(dt);
        if let Some((x, y)) = self.scout {
            self.cam = vec2((x - 1) as f32 * TILE + 16.0, (y - 1) as f32 * TILE + 16.0);
        }
        // `me`: the focused actor, or the anchor (kept off the scene, in a map corner).
        self.game.my_id = self.me.filter(|id| self.game.ents.contains_key(id)).unwrap_or(ANCHOR_ID);

        let size = if self.scout.is_some() { vec2(screen_width(), screen_height()).floor() } else { vec2(RT_W, RT_H) };
        if self.scout.is_some() && self.world_rt.texture.size() != size {
            self.world_rt = render_target(size.x as u32, size.y as u32);
            self.world_rt.texture.set_filter(FilterMode::Nearest);
            self.light_rt = render_target(size.x as u32, size.y as u32);
        }
        let origin = (self.cam - size / 2.0).floor();
        let number = SCENE_MAP;
        let cw = self.cw.get_or_insert_with(|| ContinuousWorld::new(assets.root.clone()));
        cw.update(number, 1);
        for n in cw.take_new() {
            if let Some(m) = cw.map(n) {
                assets.preload_map(m);
            }
        }
        // Ids by address for `draw_character` (FX on our actors).
        FRAME.with(|f| {
            let mut f = f.borrow_mut();
            for (id, e) in &self.game.ents {
                f.ids.insert(ent_key(e), *id);
            }
            f.fx = renderer.fx.char_fx.clone();
        });

        let fx = &renderer.fx;
        let amb = lighting::ambient(self.hour, false);
        let night = (1.0 - amb.g).clamp(0.0, 1.0);
        set_camera(&rt_camera(&self.light_rt, size));
        lighting::build_light_map(&self.game, fx, origin, amb, t_ms);
        gl_use_material(&fx.additive);
        for (i, (x, y)) in self.fires.iter().enumerate() {
            let p = tile_feet(*x, *y) - origin - vec2(0.0, 14.0);
            let flick = 1.0 + (t_ms * 0.011 + i as f32).sin() * 0.05 + (t_ms * 0.027).sin() * 0.04;
            fx.light(p, 230.0 * flick, Color::new(1.0, 0.62, 0.30, 0.95 * night.max(0.35)));
            fx.light(p, 90.0 * flick, Color::new(1.0, 0.8, 0.5, 0.8 * night.max(0.35)));
        }
        for (p, r, c) in self.lights.drain(..) {
            fx.light(p - origin, r, Color::new(c.r, c.g, c.b, c.a * night.max(0.3)));
        }
        gl_use_default_material();

        set_camera(&rt_camera(&self.world_rt, size));
        clear_background(BLACK);
        let window = Rect::new(0.0, 0.0, size.x, size.y);
        if let Some(cw) = &self.cw {
            world::draw_world(&self.game, assets, &mut self.fades, cw, origin, window, t_ms, dt);
        }
        for (x, y) in &self.fires {
            let p = vec2((x - 1) as f32 * TILE, (y - 1) as f32 * TILE) - origin;
            assets.draw(BONFIRE_GRH, p.x, p.y, true, t_ms, WHITE);
        }
        fx.draw(origin);
        gl_use_material(&fx.multiply);
        draw_texture_ex(&self.light_rt.texture, 0.0, 0.0, WHITE, DrawTextureParams { dest_size: Some(size), ..Default::default() });
        gl_use_default_material();
        if self.scout.is_some() {
            scout_grid(origin, size);
        }
        set_default_camera();

        // Blit: the visible window around the camera, at any sub-pixel position.
        let (sw, sh) = (screen_width(), screen_height());
        let s = self.scale();
        let vis = vec2(sw, sh) / s;
        let src = Rect::new(self.cam.x - origin.x - vis.x / 2.0, self.cam.y - origin.y - vis.y / 2.0, vis.x, vis.y);
        draw_texture_ex(&self.world_rt.texture, 0.0, 0.0, WHITE, DrawTextureParams { dest_size: Some(vec2(sw, sh)), source: Some(src), ..Default::default() });
        self.ambience(night, t_ms / 1000.0, s);
        // Steady state only: the first seconds load maps and textures.
        if now - self.born > 1.5 {
            self.perf.0 += (get_time() - t0) * 1000.0;
            self.perf.1 += 1;
        }
        true
    }

    /// Night touches in screen space: a warm bloom around the bonfires and fireflies
    /// drifting over the grass near them.
    fn ambience(&self, night: f32, t: f32, s: f32) {
        if night < 0.15 {
            return;
        }
        for (i, (x, y)) in self.fires.iter().enumerate() {
            let p = self.to_screen(tile_feet(*x, *y) - vec2(0.0, 12.0));
            let flick = 1.0 + (t * 11.0 + i as f32).sin() * 0.06 + (t * 27.0).sin() * 0.04;
            for k in 0..10 {
                let r = (120.0 - k as f32 * 11.0) * s * 0.5 * flick;
                draw_circle(p.x, p.y, r, Color::new(1.0, 0.55, 0.2, 0.016 * night));
            }
            // Embers rising from the fire.
            for k in 0..10 {
                let life = (t * 0.45 + k as f32 * 0.137).fract();
                let sx = (k as f32 * 12.9898).sin() * 10.0 + (t * 2.0 + k as f32).sin() * 4.0 * life;
                let q = p + vec2(sx * s * 0.5, -life * 70.0 * s * 0.5 - 6.0 * s);
                let a = (1.0 - life) * night;
                draw_circle(q.x, q.y, 0.9 * s * 0.5 + 0.5, Color::new(1.0, 0.7, 0.3, a));
            }
            // Fireflies over the meadow around the fire.
            let centre = tile_feet(*x, *y);
            for k in 0..22 {
                let h1 = ((k as f32 + 1.0) * 78.233).sin().fract().abs();
                let h2 = ((k as f32 + 1.0) * 39.425).sin().fract().abs();
                let w = centre + vec2((h1 - 0.5) * 16.0 * 32.0, (h2 - 0.5) * 10.0 * 32.0) + vec2((t * (0.3 + h2 * 0.4) + k as f32).sin() * 26.0, (t * (0.25 + h1 * 0.3) + k as f32 * 2.0).cos() * 18.0);
                let q = self.to_screen(w);
                let blink = ((t * (0.8 + h1) + k as f32 * 1.7).sin().max(0.0)).powi(3) * night;
                if blink > 0.02 {
                    draw_circle(q.x, q.y, 3.5 * s * 0.5 + 2.0, Color::new(0.75, 1.0, 0.45, 0.10 * blink));
                    draw_circle(q.x, q.y, 0.6 * s * 0.5 + 0.6, Color::new(0.92, 1.0, 0.7, 0.9 * blink));
                }
            }
        }
    }

    /// Black over everything while a cut is in progress (call after the UI).
    pub fn draw_black(&self) {
        if self.black > 0.001 {
            draw_rectangle(0.0, 0.0, screen_width(), screen_height(), Color::new(0.0, 0.0, 0.0, self.black));
        }
    }

    /// Average scene draw time so far (ms) and frames counted.
    pub fn perf(&self) -> (f64, u32) {
        (if self.perf.1 > 0 { self.perf.0 / self.perf.1 as f64 } else { 0.0 }, self.perf.1)
    }
}

/// The direction to the left (`left`) or right of `d`.
fn side(d: Dir, left: bool) -> Dir {
    match (d, left) {
        (Dir::Down, true) | (Dir::Up, false) => Dir::Right,
        (Dir::Down, false) | (Dir::Up, true) => Dir::Left,
        (Dir::Left, true) | (Dir::Right, false) => Dir::Down,
        (Dir::Left, false) | (Dir::Right, true) => Dir::Up,
    }
}

/// Development grid with tile numbers every 5 tiles.
fn scout_grid(origin: Vec2, size: Vec2) {
    let (tx0, ty0) = ((origin.x / TILE).floor() as i32 + 1, (origin.y / TILE).floor() as i32 + 1);
    let (tx1, ty1) = (tx0 + (size.x / TILE) as i32 + 1, ty0 + (size.y / TILE) as i32 + 1);
    for x in tx0..=tx1 {
        let px = (x - 1) as f32 * TILE - origin.x;
        draw_line(px, 0.0, px, size.y, 1.0, Color::new(1.0, 1.0, 1.0, if x % 5 == 0 { 0.5 } else { 0.12 }));
    }
    for y in ty0..=ty1 {
        let py = (y - 1) as f32 * TILE - origin.y;
        draw_line(0.0, py, size.x, py, 1.0, Color::new(1.0, 1.0, 1.0, if y % 5 == 0 { 0.5 } else { 0.12 }));
    }
    for x in tx0..=tx1 {
        for y in ty0..=ty1 {
            if x % 5 == 0 && y % 5 == 0 {
                let p = vec2((x - 1) as f32 * TILE, (y - 1) as f32 * TILE) - origin;
                draw_text(format!("{x},{y}"), p.x + 2.0, p.y + 12.0, 16.0, YELLOW);
            }
        }
    }
}

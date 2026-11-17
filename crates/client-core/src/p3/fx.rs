//! Graphics-free side of effects, particles, weather, visible equipment and sound: the
//! readers of the official tables and the logic that decides what to show or play.
//! OWNER: Opus D (docs/P3_CONTRATO.md). Everything here is testable without a window.
//!
//! Sources (AO20 client, `argentum-online-client-classic/CODIGO`):
//! - `Recursos.bas` `CargarParticulasBinary` (particles.ind), `CargarAnimArmas` /
//!   `CargarAnimEscudos` (armas.dat, escudos.dat, `Std` templates of moldes.ini),
//!   `CargarCascos` (cascos), `tMapDat` (map music), `Pasos` table (step sounds).
//! - `Graficos_Particulas.bas` `Particle_Render` / `Particle_Group_Render` (simulation).
//! - `Graficos_Renderizado.bas` `Char_Render` (equipment order, FX offsets).
//! - `TileEngine.bas` `GetTerrenoDePaso`, `UpdatePlayerRoof`; `TileEngine_Map.bas`
//!   `PlayCurrentMapMusic`; `Protocol.bas` rain/snow (`Particula_Lluvia`=58, `Nieve`=57).
use crate::state::Game;
use ao_formats::ini::{num, Ini};
use ao_protocol::S2C;
use std::collections::HashMap;
use std::path::Path;

// ---------------------------------------------------------------- FXs.ini

/// One official FX (`[FXn]` of FXs.ini): an animated GRH drawn on a character.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FxDef {
    pub grh: u32,
    pub offset: (f32, f32),
    /// `IsPNG=1`: normal alpha; otherwise additive (`Draw_GrhFX` with `alpha=True`).
    pub png: bool,
}

pub fn parse_fxs(ini: &Ini) -> HashMap<u16, FxDef> {
    let mut out = HashMap::new();
    let n = num(ini.section("init"), "numfxs") as u16;
    for i in 1..=n {
        let s = ini.section(&format!("fx{i}"));
        let grh = num(s, "animacion") as u32;
        if grh > 0 {
            out.insert(i, FxDef { grh, offset: (num(s, "offsetx"), num(s, "offsety")), png: num(s, "ispng") as i32 == 1 });
        }
    }
    out
}

pub fn load_fxs(recursos: &Path) -> HashMap<u16, FxDef> {
    parse_fxs(&Ini::load(&recursos.join("init").join("FXs.ini")))
}

/// `loops` of `S2C::Fx`: 255 = until stopped (meditation), otherwise plays `loops + 1`
/// times like `Draw_GrhFX` (`Elapsed < num * (Loops + 1)`).
pub const FX_FOREVER: u8 = 255;

/// Composed meditations of the official client (`AddComposedMetitation` in Recursos.bas):
/// an intro FX played once and then a loop FX.
const COMPOSED_MEDITATIONS: [(u16, u16); 15] = [(122, 126), (123, 130), (124, 134), (127, 126), (128, 126), (129, 126), (131, 130), (132, 130), (133, 130), (135, 134), (136, 134), (137, 134), (139, 138), (140, 138), (141, 138)];

/// Intro + loop FX for a meditation. The server sends the AO20 server ids of
/// Meditaciones.dat (153..=172 by level, odd = citizen, even = criminal), which the
/// client's fxs.ind (150 FX) does not have. Our fallback (not official behaviour): map
/// them to the official composed meditations in level order.
pub fn meditation_clips(fx: u16) -> Option<(u16, u16)> {
    if let Some(c) = COMPOSED_MEDITATIONS.iter().find(|c| c.0 == fx) {
        return Some(*c);
    }
    if (153..=172).contains(&fx) {
        return Some(COMPOSED_MEDITATIONS[((fx - 153) / 2) as usize]);
    }
    None
}

// ---------------------------------------------------------------- particles.ind

/// One particle stream of particles.ind (`Stream` type of Graficos_Particulas.bas).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct StreamDef {
    pub name: String,
    pub count: i32,
    pub x1: i32,
    pub y1: i32,
    pub x2: i32,
    pub y2: i32,
    pub angle: i32,
    pub vecx1: i32,
    pub vecx2: i32,
    pub vecy1: i32,
    pub vecy2: i32,
    pub life1: i32,
    pub life2: i32,
    pub friction: i32,
    pub spin: bool,
    pub spin_l: f32,
    pub spin_h: f32,
    pub additive: bool,
    pub gravity: bool,
    pub grav_strength: i32,
    pub bounce_strength: i32,
    pub xmove: bool,
    pub ymove: bool,
    pub move_x1: i32,
    pub move_x2: i32,
    pub move_y1: i32,
    pub move_y2: i32,
    pub grhs: Vec<u32>,
    /// Corner colours already swapped like `General_Particle_Create` (stored B,G,R).
    pub colors: [[u8; 3]; 4],
    /// Ticks between moves (`frame_speed`, in `timerTicksPerFrame` units).
    pub speed: f32,
    /// Moves the group lives (-1 = forever).
    pub life_counter: i32,
    pub resize: Option<(i32, i32)>,
}

struct Bin<'a> {
    d: &'a [u8],
    p: usize,
}

impl Bin<'_> {
    fn take(&mut self, n: usize) -> Result<&[u8], String> {
        let s = self.d.get(self.p..self.p + n).ok_or_else(|| format!("particles.ind truncado en {}", self.p))?;
        self.p += n;
        Ok(s)
    }
    fn u8(&mut self) -> Result<u8, String> {
        Ok(self.take(1)?[0])
    }
    fn i16(&mut self) -> Result<i16, String> {
        Ok(i16::from_le_bytes(self.take(2)?.try_into().unwrap()))
    }
    fn i32(&mut self) -> Result<i32, String> {
        Ok(i32::from_le_bytes(self.take(4)?.try_into().unwrap()))
    }
    fn f32(&mut self) -> Result<f32, String> {
        Ok(f32::from_le_bytes(self.take(4)?.try_into().unwrap()))
    }
    fn string(&mut self) -> Result<String, String> {
        let n = self.i16()? as u16 as usize;
        Ok(self.take(n)?.iter().map(|&b| b as char).collect())
    }
}

/// Parses particles.ind: an Integer count and then each `Stream` UDT as VB6 `Get` writes
/// it (unpadded; strings with a u16 length; the dynamic `grh_list()` with a descriptor
/// of u16 dimensions + per dimension i32 count and i32 lower bound). Index 0 = stream 1.
pub fn parse_particles(d: &[u8]) -> Result<Vec<StreamDef>, String> {
    let mut b = Bin { d, p: 0 };
    let n = b.i16()?.max(0) as usize;
    let mut out = Vec::with_capacity(n);
    for _ in 0..n {
        let mut s = StreamDef { name: b.string()?, count: b.i32()?, ..Default::default() };
        let _num_grhs = b.i32()?;
        let _id = b.i32()?;
        s.x1 = b.i32()?;
        s.y1 = b.i32()?;
        s.x2 = b.i32()?;
        s.y2 = b.i32()?;
        s.angle = b.i32()?;
        s.vecx1 = b.i32()?;
        s.vecx2 = b.i32()?;
        s.vecy1 = b.i32()?;
        s.vecy2 = b.i32()?;
        s.life1 = b.i32()?;
        s.life2 = b.i32()?;
        s.friction = b.i32()?;
        s.spin = b.u8()? != 0;
        s.spin_l = b.f32()?;
        s.spin_h = b.f32()?;
        s.additive = b.u8()? != 0;
        s.gravity = b.u8()? != 0;
        s.grav_strength = b.i32()?;
        s.bounce_strength = b.i32()?;
        s.xmove = b.u8()? != 0;
        s.ymove = b.u8()? != 0;
        s.move_x1 = b.i32()?;
        s.move_x2 = b.i32()?;
        s.move_y1 = b.i32()?;
        s.move_y2 = b.i32()?;
        let dims = b.i16()?;
        if dims == 1 {
            let count = b.i32()?.max(0) as usize;
            let _lbound = b.i32()?;
            for _ in 0..count {
                let g = b.i32()?;
                if g > 0 {
                    s.grhs.push(g as u32);
                }
            }
        } else if dims != 0 {
            return Err(format!("particles.ind: grh_list con {dims} dimensiones en '{}'", s.name));
        }
        for c in s.colors.iter_mut() {
            let (r, g, bl) = (b.i32()?, b.i32()?, b.i32()?);
            // General_Particle_Create: SetRGBA(rgb, .B, .G, .R)
            *c = [bl.clamp(0, 255) as u8, g.clamp(0, 255) as u8, r.clamp(0, 255) as u8];
        }
        s.speed = b.f32()?;
        s.life_counter = b.i32()?;
        let resize = b.i16()? != 0;
        let (rx, ry) = (b.i16()? as i32, b.i16()? as i32);
        s.resize = resize.then_some((rx, ry));
        out.push(s);
    }
    if b.p != d.len() {
        return Err(format!("particles.ind: sobran {} bytes", d.len() - b.p));
    }
    Ok(out)
}

pub fn load_particles(recursos: &Path) -> Vec<StreamDef> {
    std::fs::read(recursos.join("init").join("particles.ind")).ok().and_then(|d| parse_particles(&d).ok()).unwrap_or_default()
}

/// Weather streams (Declares.bas `Particula_Lluvia`, `Particula_Nieve`).
pub const STREAM_RAIN: usize = 58;
pub const STREAM_SNOW: usize = 57;

/// Small deterministic RNG (xorshift) so the simulation is reproducible in tests.
#[derive(Clone, Debug)]
pub struct Rng(u32);

impl Rng {
    pub fn new(seed: u32) -> Rng {
        Rng(seed.max(1))
    }
    pub fn next_u32(&mut self) -> u32 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.0 = x;
        x
    }
    /// `RandomNumber(a, b)`: integer in a..=b (order-insensitive).
    pub fn range(&mut self, a: i32, b: i32) -> i32 {
        let (lo, hi) = if a <= b { (a, b) } else { (b, a) };
        lo + (self.next_u32() % ((hi - lo) as u32 + 1)) as i32
    }
    pub fn rangef(&mut self, a: f32, b: f32) -> f32 {
        let t = (self.next_u32() % 10_000) as f32 / 10_000.0;
        a + (b - a) * t
    }
}

/// One live particle, in pixels relative to the group's anchor.
#[derive(Clone, Debug, Default)]
pub struct Particle {
    pub x: f32,
    pub y: f32,
    pub vx: f32,
    pub vy: f32,
    pub angle: f32,
    pub alive: i32,
    /// GRH index (0 = dead slot) and the time it was born (for its animation).
    pub grh: u32,
    pub born_ms: f32,
}

/// `particle_group`: a stream instance with the official per-tick update.
#[derive(Clone, Debug)]
pub struct ParticleGroup {
    pub stream: usize,
    pub parts: Vec<Particle>,
    frame_counter: f32,
    alive_counter: i32,
    never_die: bool,
    dying: bool,
    rng: Rng,
}

/// `engineBaseSpeed`: ticks per elapsed millisecond.
pub const ENGINE_BASE_SPEED: f32 = 0.018;

impl ParticleGroup {
    /// `life` overrides the stream's own `life_counter` (0 = use it; -1 = forever).
    pub fn new(stream: usize, def: &StreamDef, life: i32, seed: u32) -> ParticleGroup {
        let life = if life == 0 { def.life_counter } else { life };
        ParticleGroup {
            stream,
            parts: vec![Particle::default(); def.count.clamp(0, 400) as usize],
            frame_counter: 0.0,
            alive_counter: life,
            never_die: life == -1,
            dying: false,
            rng: Rng::new(seed),
        }
    }

    pub fn finished(&self) -> bool {
        self.dying && self.parts.iter().all(|p| p.grh == 0 || p.alive <= 0)
    }

    /// Stops spawning; live particles finish their life.
    pub fn stop(&mut self) {
        self.never_die = false;
        self.alive_counter = 0;
    }

    /// Advances `dt_ms`. Moves happen every `speed` ticks (`frame_counter` against
    /// `frame_speed`), at most a few per frame so a hitch never explodes the stream.
    pub fn update(&mut self, def: &StreamDef, dt_ms: f32, now_ms: f32) {
        self.frame_counter += dt_ms * ENGINE_BASE_SPEED;
        let speed = def.speed.max(0.05);
        let mut moves = 0;
        while self.frame_counter > speed && moves < 4 {
            self.frame_counter -= speed;
            moves += 1;
            self.step(def, now_ms);
        }
        if moves == 4 {
            self.frame_counter = 0.0;
        }
    }

    fn step(&mut self, def: &StreamDef, now_ms: f32) {
        if self.alive_counter == 0 {
            self.dying = true;
        }
        let dying = self.dying;
        let fric = if def.friction == 0 { 1 } else { def.friction };
        for i in 0..self.parts.len() {
            let rng = &mut self.rng;
            let p = &mut self.parts[i];
            if p.alive <= 0 && !dying {
                if def.grhs.is_empty() {
                    continue;
                }
                p.grh = def.grhs[rng.range(0, def.grhs.len() as i32 - 1) as usize];
                p.born_ms = now_ms;
                p.x = (rng.range(def.x1, def.x2) - 16) as f32;
                p.y = (rng.range(def.y1, def.y2) - 16) as f32;
                p.vx = rng.range(def.vecx1, def.vecx2) as f32;
                p.vy = rng.range(def.vecy1, def.vecy2) as f32;
                p.angle = def.angle as f32;
                p.alive = rng.range(def.life1.max(1), def.life2.max(1));
            } else {
                if p.alive <= 0 {
                    p.grh = 0;
                    continue;
                }
                if def.gravity {
                    p.vy += def.grav_strength as f32;
                    if p.y > 0.0 {
                        p.vy = def.bounce_strength as f32;
                    }
                }
                if def.spin {
                    p.angle += rng.rangef(def.spin_l, def.spin_h) / 5.0;
                    p.angle = p.angle.rem_euclid(360.0);
                }
                if def.xmove {
                    p.vx = rng.range(def.move_x1, def.move_x2) as f32;
                }
                if def.ymove {
                    p.vy = rng.range(def.move_y1, def.move_y2) as f32;
                }
            }
            // VB integer division `\` on the rounded operands.
            p.x += ((p.vx.round() as i32) / fric) as f32;
            p.y += ((p.vy.round() as i32) / fric) as f32;
            p.alive -= 1;
        }
        if !self.never_die && self.alive_counter > 0 {
            self.alive_counter -= 1;
        }
    }
}

// ---------------------------------------------------------------- equipment

/// One direction of an equipment animation.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Anim {
    None,
    /// A GRH of graficos.ini.
    Grh(u32),
    /// A row of frames from a `Std` template (moldes.ini), like the generated GRHs of
    /// `CargarAnimArmas`: `n` frames of `w`x`h` starting at (x, y) of texture `file`.
    Strip { file: u32, x: f32, y: f32, w: f32, h: f32, n: u32 },
}

/// Weapon/shield walk or helmet per AO heading (index 1..=4: N, E, S, W).
pub type EquipAnim = [Anim; 5];

/// Time per frame of a template strip (`.speed = NumFrames / 0.018` over `NumFrames`).
pub const STRIP_FRAME_MS: f32 = 1.0 / ENGINE_BASE_SPEED;

#[derive(Default, Debug)]
pub struct EquipDb {
    pub weapons: HashMap<u16, EquipAnim>,
    pub shields: HashMap<u16, EquipAnim>,
    pub helmets: HashMap<u16, EquipAnim>,
}

/// Template strips for each heading: moldes.ini rows go S, N, W, E (`BodiesHeading`).
fn template(moldes: &Ini, std: u32, file: u32) -> EquipAnim {
    const DIR_TO_HEADING: [usize; 4] = [3, 1, 4, 2];
    let mut out = [Anim::None; 5];
    let Some(m) = moldes.section(&format!("molde{std}")) else { return out };
    let m = Some(m);
    let (x, mut y, w, h) = (num(m, "x"), num(m, "y"), num(m, "width"), num(m, "height"));
    for (j, heading) in DIR_TO_HEADING.iter().enumerate() {
        let n = num(m, &format!("dir{}", j + 1)) as u32;
        if n > 0 {
            out[*heading] = Anim::Strip { file, x, y, w, h, n };
        }
        y += h;
    }
    out
}

fn equip_table(ini: &Ini, moldes: &Ini, count_key: &str, prefix: &str, dir_key: &str) -> HashMap<u16, EquipAnim> {
    let mut out = HashMap::new();
    let n = num(ini.section("init"), count_key) as u16;
    for i in 1..=n {
        let Some(s) = ini.section(&format!("{prefix}{i}")) else { continue };
        let s = Some(s);
        let std = num(s, "std") as u32;
        let anim = if std > 0 {
            template(moldes, std, num(s, "filenum") as u32)
        } else {
            let mut a = [Anim::None; 5];
            for (h, slot) in a.iter_mut().enumerate().skip(1) {
                let g = num(s, &format!("{dir_key}{h}")) as u32;
                if g > 0 {
                    *slot = Anim::Grh(g);
                }
            }
            a
        };
        if anim.iter().any(|a| *a != Anim::None) {
            out.insert(i, anim);
        }
    }
    out
}

impl EquipDb {
    pub fn parse(armas: &Ini, escudos: &Ini, cascos: &Ini, moldes: &Ini) -> EquipDb {
        EquipDb {
            weapons: equip_table(armas, moldes, "numarmas", "arma", "dir"),
            shields: equip_table(escudos, moldes, "numescudos", "esc", "dir"),
            helmets: equip_table(cascos, moldes, "numheads", "head", "head"),
        }
    }

    pub fn load(recursos: &Path) -> EquipDb {
        let init = recursos.join("init");
        let l = |f: &str| Ini::load(&init.join(f));
        EquipDb::parse(&l("armas.dat"), &l("escudos.dat"), &l("cascos.ini"), &l("moldes.ini"))
    }
}

/// What `Char_Render` draws for a heading, back to front (B = body, H = head+helmet).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Layer {
    Body,
    Head,
    Helmet,
    Weapon,
    Shield,
}

pub fn draw_order(heading: usize) -> &'static [Layer] {
    use Layer::*;
    match heading {
        2 => &[Shield, Body, Head, Helmet, Weapon],    // EAST
        1 => &[Weapon, Shield, Body, Head, Helmet],    // NORTH
        4 => &[Weapon, Body, Head, Helmet, Shield],    // WEST
        _ => &[Body, Head, Helmet, Shield, Weapon],    // SOUTH
    }
}

// ---------------------------------------------------------------- weather

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Weather {
    pub rain: bool,
    pub snow: bool,
    pub fog: bool,
}

/// The official client only shows rain/snow when the server says it rains (`bRain`,
/// `bNieve`, from `S2C::Weather`) AND the map allows it (`MapDat.LLUVIA/NIEVE`); fog when
/// the server is cloudy (`Nieblando`) or the map always has it. `AO_WEATHER=rain|snow|
/// fog|storm|clear` forces it (screenshots).
pub fn weather_for(rain_map: bool, snow_map: bool, fog_map: bool, server: (u8, bool), forced: Option<&str>) -> Weather {
    match forced {
        Some("rain") => return Weather { rain: true, ..Default::default() },
        Some("snow") => return Weather { snow: true, ..Default::default() },
        Some("fog") => return Weather { fog: true, ..Default::default() },
        Some("storm") => return Weather { rain: true, fog: true, ..Default::default() },
        Some("clear") => return Weather::default(),
        _ => {}
    }
    let (fog, precipitation) = server;
    Weather { rain: rain_map && precipitation, snow: snow_map && precipitation, fog: fog_map || fog > 0 }
}

/// Is the tile under a roof (`HayTecho`)? Rain and snow are hidden there.
pub fn under_roof(game: &Game, x: i32, y: i32) -> bool {
    let Some(m) = &game.map else { return false };
    if !ao_formats::csm::on_map(x, y) {
        return false;
    }
    m.triggers.get(ao_formats::csm::tile(x, y)).is_some_and(|t| m.is_roof(*t))
}

// ---------------------------------------------------------------- sound

/// Music number of a map: `tMapDat.music_numberLow` of the .csm header (the
/// `AoMap` reader skips it). Played as `OGG/ost_<n>.ogg`.
pub fn parse_map_music(d: &[u8]) -> Option<i32> {
    const FIVE_LAYER_SIGNATURE: i32 = 0x324C_3557;
    let mut b = Bin { d, p: 0 };
    let first = b.i32().ok()?;
    let counts = if first == FIVE_LAYER_SIGNATURE { 6 } else { 4 };
    for _ in 0..counts + 6 {
        b.i32().ok()?;
    }
    b.take(8).ok()?;
    b.string().ok()?;
    b.u8().ok()?;
    b.string().ok()?;
    let _hi = b.i32().ok()?;
    b.i32().ok()
}

pub fn map_music(recursos: &Path, number: u16) -> Option<i32> {
    parse_map_music(&std::fs::read(ao_formats::csm::map_path(recursos, number)).ok()?)
}

/// Step sound for the layer-1 texture file under the character (`GetTerrenoDePaso` +
/// `Pasos` table of Recursos.bas); `foot` alternates 0/1.
pub fn step_sound(terrain_file: u32, layer2: u32, foot: bool) -> u16 {
    let f = terrain_file;
    let pair: (u16, u16) = if (6000..=6004).contains(&f) || (550..=552).contains(&f) || (6018..=6020).contains(&f) {
        (201, 69) // forest
    } else if (7500..=7508).contains(&f) || f == 1533 || f == 2508 {
        (23, 24) // dungeon
    } else if (5000..=5004).contains(&f) {
        (199, 200) // snow
    } else if f == 6021 || f == 186 || f == 8007 {
        (197, 198) // desert
    } else if f == 20 && layer2 == 0 {
        (SND_SAILING, SND_SAILING)
    } else {
        (23, 24) // floor
    };
    if foot {
        pair.0
    } else {
        pair.1
    }
}

pub const SND_SAILING: u16 = 50;
/// Server Declares.bas: `SND_IMPACTO`, swing (miss), `SND_RESURRECCION`.
pub const SND_HIT: u16 = 10;
pub const SND_SWING: u16 = 2;
pub const SND_MEDITATE: u16 = 158;
pub const SND_RAIN_OUT_LOOP: u16 = 194;
pub const SND_RAIN_IN_LOOP: u16 = 191;

// ---------------------------------------------------------------- cues

/// What the effects/sound layer should do for this frame's server messages.
#[derive(Clone, Debug, PartialEq)]
pub enum Cue {
    /// Start (`fx > 0`) or stop (`fx == 0`) an official FX on an entity.
    Fx { id: u32, fx: u16, loops: u8 },
    /// A sound at a tile (None = not positional, e.g. own UI).
    Sound { id: u16, at: Option<(i32, i32)> },
    /// The entity is gone: drop its looping FX.
    Gone { id: u32 },
}

/// Turns the raw messages `Game::poll` returned (already applied to the state) into
/// cues. Steps of other characters come from `Moved`; the own character is predicted
/// locally, so its steps are detected by watching its tile change.
#[derive(Default)]
pub struct FxCues {
    my_tile: Option<(i32, i32)>,
    foot: bool,
    /// fx id -> wav of the first Hechizos.dat spell with that FX (spell sounds).
    pub fx_wav: HashMap<u16, u16>,
}

impl FxCues {
    pub fn with_spells(recursos: &Path) -> FxCues {
        let mut fx_wav = HashMap::new();
        if let Ok(l) = ao_formats::dat::spells::load(recursos) {
            for (_, s) in l.data.iter() {
                if s.fx > 0 && s.wav > 0 {
                    fx_wav.entry(s.fx as u16).or_insert(s.wav as u16);
                }
            }
        }
        FxCues { fx_wav, ..Default::default() }
    }

    fn terrain_step(&mut self, game: &Game, x: i32, y: i32, files: &dyn Fn(u32) -> u32) -> u16 {
        self.foot = !self.foot;
        let (l1, l2) = game.map.as_ref().filter(|_| ao_formats::csm::on_map(x, y)).map(|m| (m.layers[0][ao_formats::csm::tile(x, y)], m.layers[1][ao_formats::csm::tile(x, y)])).unwrap_or((0, 0));
        step_sound(if l1 > 0 { files(l1) } else { 0 }, l2, self.foot)
    }

    /// `files` gives the texture file of a GRH (for the terrain of a step).
    pub fn collect(&mut self, game: &Game, msgs: &[S2C], files: &dyn Fn(u32) -> u32) -> Vec<Cue> {
        let mut out = Vec::new();
        for m in msgs {
            match m {
                S2C::Fx { id, fx, loops } => {
                    out.push(Cue::Fx { id: *id, fx: *fx, loops: *loops });
                    let at = game.ents.get(id).map(|e| (e.x, e.y));
                    if *fx > 0 {
                        if let Some(w) = self.fx_wav.get(fx) {
                            out.push(Cue::Sound { id: *w, at });
                        } else if *loops == FX_FOREVER {
                            out.push(Cue::Sound { id: SND_MEDITATE, at });
                        }
                    }
                }
                S2C::Despawn { id } => out.push(Cue::Gone { id: *id }),
                S2C::EnterMap { .. } => {
                    self.my_tile = None;
                    out.push(Cue::Gone { id: u32::MAX });
                }
                // Only a real step (state.rs also uses Moved for turns and snaps).
                S2C::Moved { id, x, y, .. } if *id != game.my_id && game.ents.get(id).is_some_and(|e| e.t == 0.0 && (e.px, e.py) != (e.x, e.y)) => {
                    let s = self.terrain_step(game, *x, *y, files);
                    out.push(Cue::Sound { id: s, at: Some((*x, *y)) });
                }
                S2C::Attack { target, hit, .. } => {
                    let at = game.ents.get(target).map(|e| (e.x, e.y));
                    out.push(Cue::Sound { id: if *hit { SND_HIT } else { SND_SWING }, at });
                }
                _ => {}
            }
        }
        if let Some(me) = game.me() {
            let tile = (me.x, me.y);
            if self.my_tile.is_some_and(|t| t != tile && (t.0 - tile.0).abs() + (t.1 - tile.1).abs() == 1) {
                let s = self.terrain_step(game, tile.0, tile.1, files);
                out.push(Cue::Sound { id: s, at: Some(tile) });
            }
            self.my_tile = Some(tile);
        }
        out
    }
}

/// Volume of a positional sound heard from `me` (`ComputeVolumeAtPos`: fades out over
/// the visible area), 0..1.
pub fn volume_at(me: (i32, i32), at: Option<(i32, i32)>) -> f32 {
    let Some(at) = at else { return 1.0 };
    let d = (((at.0 - me.0).pow(2) + (at.1 - me.1).pow(2)) as f32).sqrt();
    (1.0 - d / 14.0).clamp(0.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ao_formats::recursos_dir;
    use ao_protocol::EntityKind;
    use ao_rules::Dir;

    fn root() -> Option<std::path::PathBuf> {
        let r = recursos_dir();
        r.join("init").is_dir().then_some(r)
    }

    #[test]
    fn fxs_ini() {
        let ini = Ini::parse(b"[INIT]\nNumFxs=2\n[FX1]\nAnimacion=115\nOffsetX=0\nOffsetY=10\nIsPNG=0\n[FX2]\nAnimacion=12834\nOffsetX=2\nOffsetY=-30\nIsPNG=1\n");
        let f = parse_fxs(&ini);
        assert_eq!(f[&1], FxDef { grh: 115, offset: (0.0, 10.0), png: false });
        assert_eq!(f[&2].offset, (2.0, -30.0));
        assert!(f[&2].png);
        if let Some(r) = root() {
            let all = load_fxs(&r);
            assert!(all.len() > 100, "FXs.ini: {}", all.len());
        }
    }

    /// Data audit: spell FX of Hechizos.dat missing from FXs.ini, and map music ids
    /// without an `ost_N.ogg` (printed with `--nocapture`).
    #[test]
    fn official_data_coverage() {
        let Some(r) = root() else { return };
        let fxs = load_fxs(&r);
        let spells = ao_formats::dat::spells::load(&r).unwrap().data;
        let mut missing: Vec<i32> = spells.iter().map(|(_, s)| s.fx).filter(|f| *f > 0 && !fxs.contains_key(&(*f as u16))).collect();
        missing.sort();
        missing.dedup();
        let mut no_ost = std::collections::BTreeMap::new();
        for n in 1..=1000u16 {
            if let Some(m) = map_music(&r, n).filter(|m| *m > 0) {
                if !r.join("OGG").join(format!("ost_{m}.ogg")).exists() {
                    no_ost.entry(m).or_insert_with(Vec::new).push(n);
                }
            }
        }
        if std::env::var("AO_DUMP_STREAMS").is_ok() {
            let m = ao_formats::AoMap::load(&r, 1).unwrap();
            for (x, y, s) in m.particles.iter().filter(|p| (38..=48).contains(&p.0) && (55..=64).contains(&p.1)) {
                eprintln!("p ({x},{y}) s={s} trig={}", m.triggers[ao_formats::csm::tile(*x, *y)]);
            }
        }
        eprintln!("spell fx sin FXs.ini: {missing:?}");
        eprintln!("musica sin ost: {:?}", no_ost.iter().map(|(m, v)| (m, v.len())).collect::<Vec<_>>());
        assert!(missing.len() < 10, "{missing:?}");
    }

    #[test]
    fn meditations_compose() {
        assert_eq!(meditation_clips(153), Some((122, 126)));
        assert_eq!(meditation_clips(154), Some((122, 126)));
        assert_eq!(meditation_clips(171), Some((135, 134)));
        assert_eq!(meditation_clips(131), Some((131, 130)));
        assert_eq!(meditation_clips(16), None);
    }

    #[test]
    fn particles_ind_reads_to_the_end() {
        let Some(r) = root() else { return };
        let d = std::fs::read(r.join("init").join("particles.ind")).unwrap();
        let s = parse_particles(&d).expect("parse");
        assert!(s.len() > 300);
        let f = &s[0];
        assert_eq!(f.name, "Fountain");
        assert_eq!((f.count, f.friction, f.life_counter), (20, 8, -1));
        assert_eq!(f.grhs.len(), 4);
        assert!(f.additive && f.gravity);
        // Stored R=255,G=0,B=0 is shown blue (B,G,R swap of General_Particle_Create).
        assert_eq!(f.colors[0], [0, 0, 255]);
        assert!((f.speed - 0.5).abs() < 1e-6);
        assert_eq!(f.resize, Some((10, 10)));
        assert!(!s[STREAM_RAIN - 1].grhs.is_empty() && !s[STREAM_SNOW - 1].grhs.is_empty());
        if std::env::var("AO_DUMP_STREAMS").is_ok() {
            for i in [STREAM_RAIN, STREAM_SNOW] {
                eprintln!("{i}: {:?}", s[i - 1]);
            }
        }
    }

    #[test]
    fn particles_ind_rejects_garbage() {
        assert!(parse_particles(&[5, 0, 1]).is_err());
        assert_eq!(parse_particles(&[0, 0]).unwrap().len(), 0);
    }

    fn fountain() -> StreamDef {
        StreamDef { count: 10, x1: 16, y1: 16, x2: 16, y2: 16, vecx1: -20, vecx2: 20, vecy1: -10, vecy2: -50, life1: 10, life2: 50, friction: 8, gravity: true, grav_strength: 2, bounce_strength: -5, grhs: vec![7], speed: 0.5, life_counter: -1, ..Default::default() }
    }

    #[test]
    fn particle_group_spawns_moves_and_dies() {
        let def = fountain();
        let mut g = ParticleGroup::new(1, &def, 0, 7);
        // 0.5 ticks / 0.018 per ms = one move every ~28 ms.
        g.update(&def, 20.0, 0.0);
        assert!(g.parts.iter().all(|p| p.grh == 0), "no move before frame_speed");
        g.update(&def, 20.0, 40.0);
        assert!(g.parts.iter().all(|p| p.grh == 7 && p.alive > 0));
        // Spawned at x1-16 = 0, gravity pulls up (vy negative) then bounces.
        for _ in 0..40 {
            g.update(&def, 28.0, 0.0);
        }
        assert!(g.parts.iter().all(|p| p.y <= 16.0), "fountain stays around its anchor");
        assert!(!g.finished());
        g.stop();
        for _ in 0..200 {
            g.update(&def, 28.0, 0.0);
        }
        assert!(g.finished());
    }

    #[test]
    fn group_with_limited_life_ends_alone() {
        let mut def = fountain();
        def.gravity = false;
        let mut g = ParticleGroup::new(1, &def, 5, 3);
        for _ in 0..100 {
            g.update(&def, 30.0, 0.0);
        }
        assert!(g.finished());
        // A zero friction never divides by zero.
        def.friction = 0;
        let mut g = ParticleGroup::new(1, &def, 0, 3);
        g.update(&def, 100.0, 0.0);
    }

    #[test]
    fn equipment_tables() {
        let armas = Ini::parse(b"[INIT]\nNumArmas=2\n[ARMA1]\nDir1=4719\nDir2=4721\nDir3=4718\nDir4=4720\n[ARMA2]\nStd=1\nFileNum=2002\n");
        let moldes = Ini::parse(b"[Molde1]\nX=0\nY=0\nWidth=27\nHeight=47\nDir1=6\nDir2=6\nDir3=5\nDir4=5\n");
        let cascos = Ini::parse(b"[INIT]\nNumHeads=1\n[HEAD1]\nHead1=4312\nHead2=4315\nHead3=4311\nHead4=4314\n");
        let db = EquipDb::parse(&armas, &Ini::parse(b""), &cascos, &moldes);
        assert_eq!(db.weapons[&1][1], Anim::Grh(4719));
        // Template rows: S (heading 3) first, then N (1), W (4), E (2).
        assert_eq!(db.weapons[&2][3], Anim::Strip { file: 2002, x: 0.0, y: 0.0, w: 27.0, h: 47.0, n: 6 });
        assert_eq!(db.weapons[&2][1], Anim::Strip { file: 2002, x: 0.0, y: 47.0, w: 27.0, h: 47.0, n: 6 });
        assert_eq!(db.weapons[&2][2], Anim::Strip { file: 2002, x: 0.0, y: 141.0, w: 27.0, h: 47.0, n: 5 });
        assert_eq!(db.helmets[&1][3], Anim::Grh(4311));
        if let Some(r) = root() {
            let db = EquipDb::load(&r);
            assert!(db.weapons.len() > 100 && db.shields.len() > 50 && db.helmets.len() > 50, "{} {} {}", db.weapons.len(), db.shields.len(), db.helmets.len());
        }
    }

    #[test]
    fn char_render_order() {
        assert_eq!(draw_order(3), &[Layer::Body, Layer::Head, Layer::Helmet, Layer::Shield, Layer::Weapon]);
        assert_eq!(draw_order(1)[0], Layer::Weapon);
        assert_eq!(draw_order(2)[0], Layer::Shield);
        assert_eq!(*draw_order(4).last().unwrap(), Layer::Shield);
    }

    #[test]
    fn weather_rules() {
        assert!(weather_for(false, false, false, (0, false), Some("rain")).rain);
        assert_eq!(weather_for(true, false, false, (0, true), None), Weather { rain: true, ..Default::default() });
        assert_eq!(weather_for(true, false, false, (0, false), None), Weather::default(), "server not raining");
        assert!(!weather_for(false, false, false, (0, true), None).rain, "the map must allow rain");
        assert!(weather_for(false, true, false, (20, true), None).snow);
        assert!(weather_for(false, false, true, (0, false), None).fog, "foggy map");
        assert!(weather_for(false, false, false, (30, false), None).fog, "cloudy server");
    }

    #[test]
    fn map_music_and_steps() {
        assert_eq!(step_sound(6001, 0, true), 201);
        assert_eq!(step_sound(5002, 0, false), 200);
        assert_eq!(step_sound(20, 0, true), SND_SAILING);
        assert_eq!(step_sound(20, 5, true), 23);
        assert_eq!(step_sound(1, 0, false), 24);
        let Some(r) = root() else { return };
        let mut with_music = 0;
        for n in 1..=50 {
            if let Some(m) = map_music(&r, n) {
                assert!((0..2000).contains(&m), "map {n} music {m}");
                with_music += (m > 0) as i32;
            }
        }
        assert!(with_music > 10);
    }

    #[test]
    fn cues_from_messages() {
        let mut game = Game::new("yo".into(), String::new(), std::path::PathBuf::new());
        game.my_id = 1;
        game.ents.insert(1, crate::Ent::new("yo".into(), EntityKind::Player, 1, 1, 50, 50, Dir::Down, 10, 10));
        game.ents.insert(2, crate::Ent::new("bicho".into(), EntityKind::Creature, 1, 1, 52, 50, Dir::Down, 10, 10));
        let mut c = FxCues::default();
        c.fx_wav.insert(4, 99);
        let files = |_g: u32| 6001;
        let msgs = vec![
            S2C::Fx { id: 1, fx: 16, loops: FX_FOREVER },
            S2C::Fx { id: 2, fx: 4, loops: 0 },
            S2C::Attack { attacker: 1, target: 2, hit: true, damage: 3, critical: false },
            S2C::Despawn { id: 2 },
        ];
        let cues = c.collect(&game, &msgs, &files);
        assert!(cues.contains(&Cue::Fx { id: 1, fx: 16, loops: FX_FOREVER }));
        assert!(cues.contains(&Cue::Sound { id: SND_MEDITATE, at: Some((50, 50)) }));
        assert!(cues.contains(&Cue::Sound { id: 99, at: Some((52, 50)) }));
        assert!(cues.contains(&Cue::Sound { id: SND_HIT, at: Some((52, 50)) }));
        assert!(cues.contains(&Cue::Gone { id: 2 }));
        // Own step: the tile changed by one.
        game.ents.get_mut(&1).unwrap().step_to(51, 50);
        let cues = c.collect(&game, &[], &files);
        // No map loaded: floor steps.
        assert!(matches!(cues[..], [Cue::Sound { id: 23 | 24, at: Some((51, 50)) }]));
        assert_eq!(volume_at((50, 50), Some((50, 50))), 1.0);
        assert_eq!(volume_at((50, 50), Some((80, 50))), 0.0);
    }
}

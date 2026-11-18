//! Client actions and state for spells, skills, options and map actions (`impl Game` blocks,
//! graphics-free and testable with bots). OWNER: Sonnet 4 (docs/P3_CONTRATO.md).
//!
//! What lives here (the windows in `client/src/ui/{hud,spells,character,worldmap,options}.rs`
//! only draw it):
//! - [`decode_bmp`]: BMP reader (8/16/24/32 bit) for the AO20 minimaps and world maps, which
//!   macroquad cannot load.
//! - [`WorldData`] / [`map_names`]: `init/mapsworlddata.dat` and `init/namemapa.dat`.
//! - [`fallback_minimap`]: a minimap generated from the `.csm` when the official image is missing.
//! - [`SpellKeys`]: the 1-9 spell hotkeys, saved in `config/spellkeys.ini`.
//! - [`Options`]: volumes and display flags, saved in `config/options.ini`.
//! - `impl Game`: [`Game::assign_skill`], [`Game::toggle_meditation`], [`Game::toggle_safe`],
//!   [`Game::arm_spell`].
//!
//! ## Formats found in AO20 (cited from the official VB6 client)
//! - `Recursos/Minimapas/Mapa<N>.bmp`: 100x100, 16 bit, one pixel per map tile
//!   (`General.bas::LoadMinimap`). `init/minimap.bin` (211416 bytes) is a compiled copy of the
//!   same images; it is not needed and its layout is not used here.
//! - `Recursos/interface/es_mapa<W>.bmp`: 513x594, 24 bit, the picture of world `W`; each map
//!   of the world is a 27x27 px cell (`mapsworlddata.dat`: `[WORLDMAP<W>]`, `ALTO`, `ANCHO`
//!   and `<cell>=<map number>`, row-major, 1-based). Inside a cell, tiles 14..87 (x) and
//!   11..90 (y) are the usable area (`ModUtils.bas::MINIMAP_*`, `ModMinimap.bas`).
//! - `init/namemapa.dat`: `mapa<N>=<name>`, `mapa<N>Desc=<description>`, Windows-1252.
use crate::Game;
use ao_formats::ini::Ini;
use ao_formats::AoMap;
use ao_protocol::{status, FactionC2S, Tone, C2S};
use ao_rules::progression::MAX_SKILL;
use ao_rules::spells::{SpellDef, SpellEffect, SpellTarget};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

// ---------------------------------------------------------------------------------------
// Names
// ---------------------------------------------------------------------------------------

pub const ATTRIBUTE_NAMES: [&str; 5] = ["Fuerza", "Agilidad", "Inteligencia", "Constitución", "Carisma"];

/// Name of skill `id` (1..=24, the wire id).
pub fn skill_name(id: u8) -> &'static str {
    ao_formats::dat::balance::Skill::from_id(id as i32).and_then(|s| s.spanish_name()).unwrap_or("?")
}

/// `e_Facciones` status as the player sees it.
pub fn faction_name(status: u8) -> &'static str {
    match status {
        0 => "Criminal",
        1 => "Ciudadano",
        2 => "Legión Oscura",
        3 => "Ejército Real",
        4 => "Consejo del Caos",
        5 => "Consejo Real",
        _ => "Desconocido",
    }
}

/// Whether a faction status is on the criminal side (shown in red).
pub fn is_criminal(status: u8) -> bool {
    matches!(status, 0 | 2 | 4)
}

/// How a status chip is painted.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Mood {
    Good,
    Bad,
    Neutral,
}

/// Status chips of the character (`ao_protocol::status` bits), worst first.
pub fn status_chips(flags: u32) -> Vec<(&'static str, Mood)> {
    const LIST: [(u32, &str, Mood); 9] = [
        (status::DEAD, "Muerto", Mood::Bad),
        (status::PARALYZED, "Paralizado", Mood::Bad),
        (status::IMMOBILIZED, "Inmovilizado", Mood::Bad),
        (status::POISONED, "Envenenado", Mood::Bad),
        (status::BLIND, "Ciego", Mood::Bad),
        (status::DUMB, "Estúpido", Mood::Bad),
        (status::MEDITATING, "Meditando", Mood::Good),
        (status::INVISIBLE, "Invisible", Mood::Neutral),
        (status::HIDDEN, "Oculto", Mood::Neutral),
    ];
    LIST.iter().filter(|(bit, _, _)| flags & bit != 0).map(|(_, n, m)| (*n, *m)).collect()
}

/// Lines of the spell tooltip (first = name).
pub fn spell_lines(s: &SpellDef, level: u32) -> Vec<String> {
    let amount = s.base_amount(level);
    let what = match (s.effect, s.target) {
        (SpellEffect::Heal, _) => format!("Cura unos {amount} puntos de vida (±15%)."),
        (SpellEffect::Fireball, _) => format!("Bola de fuego: unos {amount} de daño (±15%)."),
        (SpellEffect::Lightning, _) => format!("Rayo: unos {amount} de daño (±15%)."),
    };
    let target = match s.target {
        SpellTarget::Enemy => "Objetivo: criaturas.",
        SpellTarget::Ally => "Objetivo: personajes (tú incluido).",
    };
    vec![
        s.name.clone(),
        what,
        target.to_string(),
        format!("Maná {}   Alcance {} casillas", s.mana, s.range),
        format!("Enfriamiento {:.1} s", s.cooldown_ms as f32 / 1000.0),
    ]
}

// ---------------------------------------------------------------------------------------
// BMP
// ---------------------------------------------------------------------------------------

/// A decoded picture: RGBA8, top row first.
#[derive(Clone, Debug, PartialEq)]
pub struct Rgba {
    pub w: u32,
    pub h: u32,
    pub data: Vec<u8>,
}

fn le16(b: &[u8], o: usize) -> Option<u32> {
    Some(u16::from_le_bytes(b.get(o..o + 2)?.try_into().ok()?) as u32)
}

fn le32(b: &[u8], o: usize) -> Option<u32> {
    Some(u32::from_le_bytes(b.get(o..o + 4)?.try_into().ok()?))
}

/// Scales the channel selected by `mask` to 0..=255.
fn channel(v: u32, mask: u32) -> u8 {
    if mask == 0 {
        return 255;
    }
    let shift = mask.trailing_zeros();
    let max = mask >> shift;
    (((v & mask) >> shift) * 255 / max.max(1)) as u8
}

/// Decodes an uncompressed BMP of 8 (palette), 16 (555 or bitfields), 24 or 32 bits.
/// Returns `None` for anything else (RLE, corrupt files).
pub fn decode_bmp(b: &[u8]) -> Option<Rgba> {
    if b.get(0..2)? != b"BM" {
        return None;
    }
    let data_off = le32(b, 10)? as usize;
    let dib = le32(b, 14)? as usize;
    let (w, h_raw) = (le32(b, 18)? as i32, le32(b, 22)? as i32);
    let bpp = le16(b, 28)?;
    let compression = le32(b, 30).unwrap_or(0);
    if w <= 0 || h_raw == 0 || w > 16384 || h_raw.abs() > 16384 || !matches!(compression, 0 | 3) {
        return None;
    }
    let (w, h, top_down) = (w as usize, h_raw.unsigned_abs() as usize, h_raw < 0);
    let stride = (w * bpp as usize).div_ceil(32) * 4;
    let rows = b.get(data_off..data_off.checked_add(stride.checked_mul(h)?)?)?;
    let masks = if bpp == 16 || bpp == 32 {
        if compression == 3 {
            (le32(b, 14 + 40)?, le32(b, 14 + 44)?, le32(b, 14 + 48)?)
        } else if bpp == 16 {
            (0x7C00, 0x03E0, 0x001F)
        } else {
            (0x00FF_0000, 0x0000_FF00, 0x0000_00FF)
        }
    } else {
        (0, 0, 0)
    };
    let palette: Vec<[u8; 3]> = if bpp == 8 {
        let n = le32(b, 46).filter(|n| *n > 0).unwrap_or(256).min(256) as usize;
        (0..n).map(|i| b.get(14 + dib + i * 4..14 + dib + i * 4 + 3).map(|p| [p[2], p[1], p[0]])).collect::<Option<_>>()?
    } else {
        Vec::new()
    };
    let mut data = Vec::with_capacity(w * h * 4);
    for y in 0..h {
        let src_y = if top_down { y } else { h - 1 - y };
        let row = &rows[src_y * stride..(src_y + 1) * stride];
        for x in 0..w {
            let px = match bpp {
                8 => palette.get(row[x] as usize).copied().unwrap_or([0, 0, 0]),
                16 => {
                    let v = u16::from_le_bytes([row[x * 2], row[x * 2 + 1]]) as u32;
                    [channel(v, masks.0), channel(v, masks.1), channel(v, masks.2)]
                }
                24 => [row[x * 3 + 2], row[x * 3 + 1], row[x * 3]],
                32 => {
                    let v = u32::from_le_bytes([row[x * 4], row[x * 4 + 1], row[x * 4 + 2], row[x * 4 + 3]]);
                    [channel(v, masks.0), channel(v, masks.1), channel(v, masks.2)]
                }
                _ => return None,
            };
            data.extend_from_slice(&[px[0], px[1], px[2], 255]);
        }
    }
    Some(Rgba { w: w as u32, h: h as u32, data })
}

// ---------------------------------------------------------------------------------------
// World map data
// ---------------------------------------------------------------------------------------

/// First and last usable tile inside a map when it is drawn as a cell of the world picture.
pub const WORLD_TILE_X: (i32, i32) = (14, 87);
pub const WORLD_TILE_Y: (i32, i32) = (11, 90);

#[derive(Clone, Debug, Default, PartialEq)]
pub struct World {
    pub cols: u32,
    pub rows: u32,
    /// Map number per cell, row-major (0 = empty cell).
    pub cells: Vec<u16>,
}

impl World {
    pub fn map_at(&self, cx: u32, cy: u32) -> u16 {
        if cx >= self.cols || cy >= self.rows {
            return 0;
        }
        self.cells.get((cy * self.cols + cx) as usize).copied().unwrap_or(0)
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct WorldData {
    /// Index 0 = world 1 (picture `es_mapa1.bmp`).
    pub worlds: Vec<World>,
}

impl WorldData {
    pub fn parse(bytes: &[u8]) -> WorldData {
        let ini = Ini::parse(bytes);
        let mut worlds = Vec::new();
        for n in 1..=8 {
            let Some(sec) = ini.section(&format!("WORLDMAP{n}")) else { break };
            let get = |k: &str| sec.get(k).and_then(|v| v.parse::<u32>().ok()).unwrap_or(0);
            let (cols, rows) = (get("ancho"), get("alto"));
            let cells = (1..=cols * rows).map(|i| sec.get(&i.to_string()).and_then(|v| v.parse().ok()).unwrap_or(0)).collect();
            worlds.push(World { cols, rows, cells });
        }
        WorldData { worlds }
    }

    pub fn load(recursos: &Path) -> WorldData {
        WorldData::parse(&std::fs::read(recursos.join("init").join("mapsworlddata.dat")).unwrap_or_default())
    }

    /// (world index, column, row) of map `number`.
    pub fn locate(&self, number: u16) -> Option<(usize, u32, u32)> {
        if number == 0 {
            return None;
        }
        self.worlds.iter().enumerate().find_map(|(i, w)| w.cells.iter().position(|c| *c == number).map(|p| (i, p as u32 % w.cols, p as u32 / w.cols)))
    }
}

/// Position of tile (x, y) of a map inside its cell, 0..1 on both axes (VB6 `RenderMinimapCentered`).
pub fn tile_in_cell(x: i32, y: i32) -> (f32, f32) {
    let fx = (x.clamp(WORLD_TILE_X.0, WORLD_TILE_X.1) - WORLD_TILE_X.0) as f32 + 0.5;
    let fy = (y.clamp(WORLD_TILE_Y.0, WORLD_TILE_Y.1) - WORLD_TILE_Y.0) as f32 + 0.5;
    (fx / (WORLD_TILE_X.1 - WORLD_TILE_X.0 + 1) as f32, fy / (WORLD_TILE_Y.1 - WORLD_TILE_Y.0 + 1) as f32)
}

/// Name and description of a map, from `namemapa.dat`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct MapInfo {
    pub name: String,
    pub desc: String,
}

pub fn map_names(bytes: &[u8]) -> HashMap<u16, MapInfo> {
    let ini = Ini::parse(bytes);
    let mut out: HashMap<u16, MapInfo> = HashMap::new();
    let Some(sec) = ini.section("NameMapa") else { return out };
    for (k, v) in sec {
        let Some(rest) = k.strip_prefix("mapa") else { continue };
        if let Some(n) = rest.strip_suffix("desc").and_then(|n| n.parse::<u16>().ok()) {
            out.entry(n).or_default().desc = v.trim().to_string();
        } else if let Ok(n) = rest.parse::<u16>() {
            out.entry(n).or_default().name = v.trim().to_string();
        }
    }
    out
}

pub fn load_map_names(recursos: &Path) -> HashMap<u16, MapInfo> {
    map_names(&std::fs::read(recursos.join("init").join("namemapa.dat")).unwrap_or_default())
}

/// Official minimap of map `number` (`Recursos/Minimapas/Mapa<N>.bmp`), if present.
pub fn load_minimap(recursos: &Path, number: u16) -> Option<Rgba> {
    decode_bmp(&std::fs::read(recursos.join("Minimapas").join(format!("Mapa{number}.bmp"))).ok()?)
}

/// A 100x100 minimap made from the map itself (one pixel per tile: water, blocked, free,
/// safe zone), for maps without an official image.
pub fn fallback_minimap(map: &AoMap) -> Rgba {
    use ao_formats::csm::{BLOCK_ALL, FLAG_WATER, MAP_SIZE, TRIGGER_SAFE_ZONE};
    let n = MAP_SIZE as u32;
    let mut data = Vec::with_capacity((n * n * 4) as usize);
    for y in 1..=MAP_SIZE {
        for x in 1..=MAP_SIZE {
            let b = map.blocked(x, y);
            let c = if b & FLAG_WATER != 0 {
                [38, 72, 130]
            } else if b & BLOCK_ALL == BLOCK_ALL {
                [58, 52, 46]
            } else if map.trigger(x, y) == TRIGGER_SAFE_ZONE {
                [96, 120, 78]
            } else {
                [74, 96, 52]
            };
            data.extend_from_slice(&[c[0], c[1], c[2], 255]);
        }
    }
    Rgba { w: n, h: n, data }
}

// ---------------------------------------------------------------------------------------
// Config files
// ---------------------------------------------------------------------------------------

/// Folder of the player's local settings: `$AO_CONFIG_DIR`, else `config/`.
pub fn config_dir() -> PathBuf {
    std::env::var("AO_CONFIG_DIR").map(PathBuf::from).unwrap_or_else(|_| PathBuf::from("config"))
}

pub(crate) fn write_config(path: &Path, text: &str) -> bool {
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    std::fs::write(path, text).is_ok()
}

/// The 1-9 spell hotkeys (spell ids, 0 = free). Saved in `config/spellkeys.ini` as
/// `1=<spell id>` lines. Without a file the keys follow the first spells of the book, which
/// is what the original client does.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SpellKeys {
    pub ids: [u16; 9],
    /// A file was read or the player bound something: do not overwrite with the default.
    pub custom: bool,
}

impl SpellKeys {
    pub fn parse(text: &str) -> SpellKeys {
        let mut k = SpellKeys { custom: true, ..Default::default() };
        for line in text.lines() {
            let Some((a, b)) = line.split_once('=') else { continue };
            if let (Ok(i), Ok(id)) = (a.trim().parse::<usize>(), b.trim().parse::<u16>()) {
                if (1..=9).contains(&i) {
                    k.ids[i - 1] = id;
                }
            }
        }
        k
    }

    pub fn to_text(&self) -> String {
        self.ids.iter().enumerate().map(|(i, id)| format!("{}={}\n", i + 1, id)).collect()
    }

    pub fn path() -> PathBuf {
        config_dir().join("spellkeys.ini")
    }

    pub fn load() -> SpellKeys {
        std::fs::read_to_string(Self::path()).map(|t| SpellKeys::parse(&t)).unwrap_or_default()
    }

    pub fn save(&self) -> bool {
        write_config(&Self::path(), &self.to_text())
    }

    /// Spell on key `key` (0-based), taking the book as default when nothing was configured.
    pub fn spell_on(&self, key: usize, book: &[u16]) -> u16 {
        if self.custom {
            return self.ids.get(key).copied().unwrap_or(0);
        }
        book.iter().copied().filter(|id| *id != 0).nth(key).unwrap_or(0)
    }

    /// Key (0-based) that casts `spell`.
    pub fn key_of(&self, spell: u16, book: &[u16]) -> Option<usize> {
        (0..9).find(|k| spell != 0 && self.spell_on(*k, book) == spell)
    }

    /// Binds `spell` to `key` (0-based), freeing any other key that had it. Materialises the
    /// default first so the other keys keep working.
    pub fn bind(&mut self, key: usize, spell: u16, book: &[u16]) {
        if key >= 9 {
            return;
        }
        if !self.custom {
            for k in 0..9 {
                self.ids[k] = self.spell_on(k, book);
            }
            self.custom = true;
        }
        if spell != 0 {
            for id in self.ids.iter_mut().filter(|id| **id == spell) {
                *id = 0;
            }
        }
        self.ids[key] = spell;
    }

    /// Swaps what keys `a` and `b` (0-based) cast (dragging on the action bar).
    pub fn swap(&mut self, a: usize, b: usize, book: &[u16]) {
        if a >= 9 || b >= 9 || a == b {
            return;
        }
        if !self.custom {
            for k in 0..9 {
                self.ids[k] = self.spell_on(k, book);
            }
            self.custom = true;
        }
        self.ids.swap(a, b);
    }
}

/// Short label of a spell for an action slot: the most telling word, cut to `max` chars
/// ("Curar Heridas Leves" -> "Heridas", "Dardo Mágico" -> "Dardo").
pub fn short_spell_name(name: &str, max: usize) -> String {
    const WEAK: [&str; 7] = ["curar", "remover", "menor", "leves", "graves", "de", "del"];
    let words: Vec<&str> = name.split_whitespace().collect();
    let pick = words.iter().find(|w| !WEAK.contains(&w.to_lowercase().as_str()) && w.chars().count() > 2).or(words.first()).copied().unwrap_or("");
    let mut s: String = pick.chars().take(max).collect();
    if pick.chars().count() > max && max > 1 {
        s = pick.chars().take(max - 1).collect::<String>() + ".";
    }
    s
}

/// A value shown by a HUD gauge (0..1) with a trailing "recent damage" part: when the value
/// drops, the trail waits `hold` seconds and then slides down; when it rises, the trail jumps.
/// `hit` is the time of the last drop (for the flash).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Trail {
    pub value: f32,
    pub trail: f32,
    pub hit: f64,
    pub gain: f64,
    started: bool,
}

impl Default for Trail {
    fn default() -> Self {
        Trail { value: 1.0, trail: 1.0, hit: f64::MIN, gain: f64::MIN, started: false }
    }
}

impl Trail {
    pub const HOLD: f64 = 0.45;
    /// Fraction per second the trail slides.
    pub const SPEED: f32 = 0.8;

    /// Feeds the current value; the first call only sets it (no flash on entering).
    pub fn update(&mut self, v: f32, now: f64, dt: f32) {
        let v = v.clamp(0.0, 1.0);
        if !self.started {
            *self = Trail { value: v, trail: v, started: true, ..Trail::default() };
            return;
        }
        if v < self.value - 1e-4 {
            self.hit = now;
        } else if v > self.value + 1e-4 {
            self.gain = now;
        }
        self.value = v;
        if self.trail < v {
            self.trail = v;
        } else if now - self.hit > Self::HOLD {
            self.trail = (self.trail - Self::SPEED * dt).max(v);
        }
    }

    /// 1 right after a drop, fading to 0 in `secs`.
    pub fn flash(&self, now: f64, secs: f64) -> f32 {
        (1.0 - (now - self.hit) / secs).clamp(0.0, 1.0) as f32
    }
}

/// Player options saved in `config/options.ini`.
///
/// **For the audio code**: [`audio_volumes`] returns the (music, effects) volumes to apply
/// (0 when muted). The Options window rewrites `config/options.ini` as soon as a slider
/// moves, so re-read it when the window changes something (or every second or so); the
/// client crate also exposes the in-memory copy as `ui::options::current()` (no disk read).
#[derive(Clone, Debug, PartialEq)]
pub struct Options {
    /// Music volume, 0..=1.
    pub music: f32,
    /// Sound effects volume, 0..=1.
    pub sfx: f32,
    /// Silence everything without losing the sliders.
    pub mute: bool,
    /// Show fps and scale in the bottom bar.
    pub show_fps: bool,
}

impl Default for Options {
    fn default() -> Self {
        Options { music: 0.6, sfx: 0.8, mute: false, show_fps: true }
    }
}

impl Options {
    pub fn parse(text: &str) -> Options {
        let mut o = Options::default();
        for line in text.lines() {
            let Some((k, v)) = line.split_once('=') else { continue };
            let (k, v) = (k.trim(), v.trim());
            let f = || v.parse::<f32>().ok().map(|f| f.clamp(0.0, 1.0));
            let b = || v.parse::<bool>().ok();
            match k {
                "music" => o.music = f().unwrap_or(o.music),
                "sfx" => o.sfx = f().unwrap_or(o.sfx),
                "mute" => o.mute = b().unwrap_or(o.mute),
                "show_fps" => o.show_fps = b().unwrap_or(o.show_fps),
                _ => {}
            }
        }
        o
    }

    pub fn to_text(&self) -> String {
        format!("music={:.2}\nsfx={:.2}\nmute={}\nshow_fps={}\n", self.music, self.sfx, self.mute, self.show_fps)
    }

    pub fn path() -> PathBuf {
        config_dir().join("options.ini")
    }

    pub fn load() -> Options {
        std::fs::read_to_string(Self::path()).map(|t| Options::parse(&t)).unwrap_or_default()
    }

    pub fn save(&self) -> bool {
        write_config(&Self::path(), &self.to_text())
    }

    /// (music, effects) volumes to apply: 0 when muted.
    pub fn volumes(&self) -> (f32, f32) {
        if self.mute {
            (0.0, 0.0)
        } else {
            (self.music, self.sfx)
        }
    }
}

/// Volumes (music, effects) from `config/options.ini`, for the audio code.
pub fn audio_volumes() -> (f32, f32) {
    Options::load().volumes()
}

// ---------------------------------------------------------------------------------------
// Player actions
// ---------------------------------------------------------------------------------------

impl Game {
    /// Spends `points` free points on skill `skill` (1..=24). Checks what the server checks
    /// (free points, `MAX_SKILL`) so a legit player never trips the anti-cheat, and applies
    /// the change locally at once so fast clicks cannot overspend; the server's `Skills`
    /// message then overwrites it. Returns whether something was sent.
    pub fn assign_skill(&mut self, skill: u8, points: u16) -> bool {
        let idx = (skill as usize).wrapping_sub(1);
        let Some(current) = self.local.skills.get(idx).copied() else { return false };
        let room = MAX_SKILL.saturating_sub(current) as u16;
        let points = points.min(self.local.free_skill_points).min(room);
        if points == 0 {
            return false;
        }
        self.local.skills[idx] = current + points as u8;
        self.local.free_skill_points -= points;
        self.send(&C2S::AssignSkill { skill, points });
        true
    }

    /// Toggles meditation (`C2S::Meditate`); a dead character cannot.
    pub fn toggle_meditation(&mut self) -> bool {
        if self.me().is_some_and(|e| e.status & status::DEAD != 0) {
            self.say("Estás muerto.", Tone::Warn);
            return false;
        }
        self.send(&C2S::Meditate);
        true
    }

    /// Toggles the citizen "seguro" (the padlock of the HUD).
    pub fn toggle_safe(&mut self) {
        self.send(&C2S::Faction(FactionC2S::ToggleSafe));
    }

    /// Arms spell `id` for the next click on the world (what the 1-9 keys and the spell
    /// book do). Returns false if the spell is not in the book.
    pub fn arm_spell(&mut self, id: u16) -> bool {
        let Some(i) = self.spells.iter().position(|s| s.id == id) else { return false };
        self.selected = i;
        self.casting = true;
        true
    }

    /// Spells of the book in slot order: (slot 0-based, definition).
    pub fn book(&self) -> Vec<(usize, &SpellDef)> {
        self.local.spell_slots.iter().enumerate().filter(|(_, id)| **id != 0).filter_map(|(i, id)| self.spells.iter().find(|s| s.id == *id).map(|s| (i, s))).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bmp(bpp: u16, w: u32, h: i32, compression: u32, extra: &[u8], pixels: &[u8]) -> Vec<u8> {
        let dib = 40 + extra.len() as u32;
        let off = 14 + dib;
        let mut b = Vec::new();
        b.extend_from_slice(b"BM");
        b.extend_from_slice(&(off + pixels.len() as u32).to_le_bytes());
        b.extend_from_slice(&[0; 4]);
        b.extend_from_slice(&off.to_le_bytes());
        b.extend_from_slice(&dib.to_le_bytes());
        b.extend_from_slice(&w.to_le_bytes());
        b.extend_from_slice(&h.to_le_bytes());
        b.extend_from_slice(&1u16.to_le_bytes());
        b.extend_from_slice(&bpp.to_le_bytes());
        b.extend_from_slice(&compression.to_le_bytes());
        b.extend_from_slice(&[0; 20]);
        b.extend_from_slice(extra);
        b.extend_from_slice(pixels);
        b
    }

    #[test]
    fn bmp_24_bottom_up_with_padding() {
        // 2x2: bottom row (red, green) first, then top row (blue, white); rows padded to 8 bytes.
        let px = [0, 0, 255, 0, 255, 0, 0, 0, /* top */ 255, 0, 0, 255, 255, 255, 0, 0];
        let img = decode_bmp(&bmp(24, 2, 2, 0, &[], &px)).unwrap();
        assert_eq!((img.w, img.h), (2, 2));
        assert_eq!(&img.data[0..4], &[0, 0, 255, 255], "top-left is blue");
        assert_eq!(&img.data[4..8], &[255, 255, 255, 255]);
        assert_eq!(&img.data[8..12], &[255, 0, 0, 255], "bottom-left is red");
        assert_eq!(&img.data[12..16], &[0, 255, 0, 255]);
    }

    #[test]
    fn bmp_16_bitfields_565_and_555() {
        let masks: Vec<u8> = [0xF800u32, 0x07E0, 0x001F].iter().flat_map(|m| m.to_le_bytes()).collect();
        let red = 0xF800u16.to_le_bytes();
        let img = decode_bmp(&bmp(16, 1, 1, 3, &masks, &[red[0], red[1], 0, 0])).unwrap();
        assert_eq!(&img.data[0..4], &[255, 0, 0, 255]);
        let green555 = 0x03E0u16.to_le_bytes();
        let img = decode_bmp(&bmp(16, 1, 1, 0, &[], &[green555[0], green555[1], 0, 0])).unwrap();
        assert_eq!(&img.data[0..4], &[0, 255, 0, 255]);
    }

    #[test]
    fn bmp_rejects_garbage() {
        assert!(decode_bmp(b"").is_none());
        assert!(decode_bmp(b"PNG not a bmp").is_none());
        let mut b = bmp(24, 2, 2, 0, &[], &[0; 16]);
        b.truncate(b.len() - 4);
        assert!(decode_bmp(&b).is_none(), "truncated pixel data");
        assert!(decode_bmp(&bmp(24, 2, 2, 1, &[], &[0; 16])).is_none(), "RLE not supported");
    }

    #[test]
    fn official_resources_decode_when_present() {
        let root = ao_formats::recursos_dir();
        if let Some(m) = load_minimap(&root, 562) {
            assert_eq!((m.w, m.h), (100, 100));
            assert!(m.data.chunks(4).any(|p| p[1] > p[0]), "a forest minimap has green");
        }
        let world = std::fs::read(root.join("interface").join("es_mapa1.bmp")).ok().and_then(|b| decode_bmp(&b));
        if let Some(w) = world {
            assert_eq!((w.w, w.h), (513, 594));
        }
        let data = WorldData::load(&root);
        if !data.worlds.is_empty() {
            assert_eq!((data.worlds[0].cols, data.worlds[0].rows), (19, 22));
            assert!(data.locate(562).is_some() || data.locate(1).is_some());
        }
    }

    #[test]
    fn world_data_locates_maps() {
        let ini = b"[INIT]\r\nTOTALWORLDS=2\r\n[WORLDMAP1]\r\nALTO=2\r\nANCHO=3\r\n1=10\r\n2=11\r\n3=12\r\n4=13\r\n5=14\r\n6=15\r\n[WORLDMAP2]\r\nALTO=1\r\nANCHO=2\r\n1=7\r\n2=8\r\n";
        let d = WorldData::parse(ini);
        assert_eq!(d.worlds.len(), 2);
        assert_eq!(d.locate(14), Some((0, 1, 1)));
        assert_eq!(d.locate(8), Some((1, 1, 0)));
        assert_eq!(d.locate(99), None);
        assert_eq!(d.locate(0), None);
        assert_eq!(d.worlds[0].map_at(2, 0), 12);
        assert_eq!(d.worlds[0].map_at(3, 0), 0, "outside the grid");
    }

    #[test]
    fn tile_position_inside_a_cell_matches_the_official_formula() {
        let (x, y) = tile_in_cell(14, 11);
        assert!((x - 0.5 / 74.0).abs() < 1e-6 && (y - 0.5 / 80.0).abs() < 1e-6);
        let (x, y) = tile_in_cell(100, 1);
        assert!(x < 1.0 && y > 0.0, "clamped into the usable area");
    }

    #[test]
    fn map_names_read_names_and_descriptions() {
        let t = b"[NameMapa]\r\nmapa133=Isla del Averno\r\nmapa133Desc=Ingreso a Dungeon\r\nmapa75=Cercan\xedas\r\n";
        let m = map_names(t);
        assert_eq!(m[&133], MapInfo { name: "Isla del Averno".into(), desc: "Ingreso a Dungeon".into() });
        assert_eq!(m[&75].name, "Cercanías");
    }

    #[test]
    fn spell_keys_default_follow_the_book_then_bind() {
        let book = [0, 5, 0, 7, 9];
        let mut k = SpellKeys::default();
        assert_eq!(k.spell_on(0, &book), 5);
        assert_eq!(k.spell_on(2, &book), 9);
        assert_eq!(k.spell_on(3, &book), 0);
        assert_eq!(k.key_of(7, &book), Some(1));
        k.bind(0, 9, &book);
        assert!(k.custom);
        assert_eq!(k.spell_on(0, &book), 9);
        assert_eq!(k.spell_on(2, &book), 0, "9 moved from key 3 to key 1");
        assert_eq!(k.spell_on(1, &book), 7, "the rest of the default survived");
        k.bind(1, 0, &book);
        assert_eq!(k.spell_on(1, &book), 0);
        assert_eq!(SpellKeys::parse(&k.to_text()), k);
    }

    #[test]
    fn options_roundtrip_and_clamp() {
        let o = Options { music: 0.25, sfx: 1.0, mute: true, show_fps: false };
        assert_eq!(Options::parse(&o.to_text()), o);
        assert_eq!(o.volumes(), (0.0, 0.0));
        let p = Options::parse("music=7\nsfx=abc\nmute=maybe\n");
        assert_eq!((p.music, p.sfx, p.mute), (1.0, Options::default().sfx, false));
    }

    #[test]
    fn options_and_keys_files_are_written_to_the_config_dir() {
        let dir = std::env::temp_dir().join(format!("ao_hud_cfg_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let path = dir.join("options.ini");
        assert!(write_config(&path, &Options { music: 0.5, ..Default::default() }.to_text()));
        assert_eq!(Options::parse(&std::fs::read_to_string(&path).unwrap()).music, 0.5);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn status_chips_and_names() {
        let c = status_chips(status::PARALYZED | status::MEDITATING);
        assert_eq!(c, vec![("Paralizado", Mood::Bad), ("Meditando", Mood::Good)]);
        assert!(status_chips(0).is_empty());
        assert_eq!(faction_name(1), "Ciudadano");
        assert!(is_criminal(0) && !is_criminal(1) && !is_criminal(3));
        assert_eq!(skill_name(1), "Magia");
        assert_eq!(skill_name(24), "Domar");
    }

    #[test]
    fn assign_skill_checks_budget_and_cap_and_updates_locally() {
        let mut g = Game::new("t".into(), "127.0.0.1:1".into(), PathBuf::new());
        g.local.free_skill_points = 3;
        g.local.skills[4] = 98;
        assert!(g.assign_skill(5, 5), "capped to what is left");
        assert_eq!((g.local.skills[4], g.local.free_skill_points), (100, 1));
        assert!(!g.assign_skill(5, 1), "already at the maximum");
        assert!(g.assign_skill(1, 9));
        assert_eq!((g.local.skills[0], g.local.free_skill_points), (1, 0));
        assert!(!g.assign_skill(1, 1), "no free points");
        assert!(!g.assign_skill(0, 1) && !g.assign_skill(25, 1), "out of range");
    }

    #[test]
    fn swapping_keys_materialises_the_book_default() {
        let book = [7u16, 0, 9, 11];
        let mut k = SpellKeys::default();
        k.swap(0, 2, &book);
        assert!(k.custom);
        assert_eq!((k.spell_on(0, &book), k.spell_on(1, &book), k.spell_on(2, &book)), (11, 9, 7));
        k.swap(0, 9, &book);
        assert_eq!(k.spell_on(0, &book), 11, "out of range is ignored");
    }

    #[test]
    fn short_spell_names_keep_the_telling_word() {
        assert_eq!(short_spell_name("Curar Heridas Leves", 8), "Heridas");
        assert_eq!(short_spell_name("Dardo Mágico", 8), "Dardo");
        assert_eq!(short_spell_name("Invisibilidad Menor", 8), "Invisib.");
        assert_eq!(short_spell_name("", 8), "");
    }

    #[test]
    fn trail_holds_then_slides_and_jumps_up() {
        let mut t = Trail::default();
        t.update(0.8, 0.0, 0.016);
        assert_eq!((t.value, t.trail), (0.8, 0.8), "first value: no flash");
        assert_eq!(t.flash(0.0, 0.3), 0.0);
        t.update(0.5, 1.0, 0.016);
        assert_eq!((t.value, t.trail), (0.5, 0.8));
        assert!(t.flash(1.0, 0.3) > 0.99);
        t.update(0.5, 1.2, 0.1);
        assert_eq!(t.trail, 0.8, "holds");
        t.update(0.5, 1.6, 0.1);
        assert!(t.trail < 0.8 && t.trail >= 0.5, "slides");
        for i in 0..40 {
            t.update(0.5, 2.0 + i as f64 * 0.1, 0.1);
        }
        assert_eq!(t.trail, 0.5);
        t.update(0.9, 9.0, 0.016);
        assert_eq!((t.value, t.trail), (0.9, 0.9), "healing raises both");
    }
}

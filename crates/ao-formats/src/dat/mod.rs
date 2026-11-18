//! Typed readers for the official AO20 data files in `Recursos/Dat/`:
//! `obj.dat` ([`obj`]), `npcs.dat` ([`npcs`]), `Hechizos.dat` ([`spells`]) and
//! `Balance.dat` ([`balance`]).
//!
//! Each reader mirrors the official VB6 server loader (named in the module docs) and keeps
//! the official numeric ids (the `123` of `[OBJ123]`). Reading never panics and never fails
//! on a bad field: a malformed value becomes the VB default (`Val` semantics, usually 0) and
//! a [`Warning`] is collected, so one typo in a 1 MB data file cannot take the server down.
//!
//! The INI dialect is parsed here instead of with [`crate::ini`] on purpose. The official
//! `clsIniManager` has no inline comments: everything after the first `=` is the value, so
//! `Name='Casper 01'` or `PalabrasMagicas=IrX' ÔA'l` contain apostrophes that
//! `ini::Ini` would cut off.
use std::collections::HashMap;
use std::fmt;
use std::path::{Path, PathBuf};

pub mod balance;
pub mod chests;
pub mod craft;
pub mod npcs;
pub mod obj;
pub mod quests;
pub mod spells;

/// A file could not be read at all (missing, no permission). Bad *contents* are never an
/// error: they become [`Warning`]s.
#[derive(Debug)]
pub enum Error {
    Io { path: PathBuf, source: std::io::Error },
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Io { path, source } => write!(f, "cannot read {}: {source}", path.display()),
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Error::Io { source, .. } => Some(source),
        }
    }
}

/// Something odd found in the data (a value that is not a clean number, an unknown class
/// name, a section the official server would ignore...). Loading continued with a default.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Warning {
    /// Data file name, e.g. `obj.dat`.
    pub file: &'static str,
    /// Section as written in the file, e.g. `OBJ123` (empty for file-level warnings).
    pub section: String,
    pub key: String,
    /// The offending value as found in the file.
    pub value: String,
    pub message: String,
}

impl fmt::Display for Warning {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} [{}] {}={:?}: {}", self.file, self.section, self.key, self.value, self.message)
    }
}

/// Result of a `load`: the data plus everything odd found while reading it.
#[derive(Clone, Debug)]
pub struct Loaded<T> {
    pub data: T,
    pub warnings: Vec<Warning>,
}

/// Entries keyed by their official id (the number in the section name). Ids are sparse
/// (`obj.dat` declares 6754 objects but defines 5199), so this is an id-indexed vector of
/// optional slots: `get(id)` is O(1) and iteration skips the holes.
#[derive(Clone, Debug)]
pub struct Table<T> {
    slots: Vec<Option<T>>,
    len: usize,
    /// The count the file itself declares in `[INIT]` (`NumOBJs`, `NumNPCs`,
    /// `NumeroHechizos`); the official server loops `1..=declared`, so entries with a
    /// larger id are ignored there.
    pub declared: u32,
}

impl<T> Default for Table<T> {
    fn default() -> Self {
        Table { slots: Vec::new(), len: 0, declared: 0 }
    }
}

impl<T> Table<T> {
    pub(crate) fn insert(&mut self, id: u32, value: T) -> bool {
        let i = id as usize;
        if self.slots.len() <= i {
            self.slots.resize_with(i + 1, || None);
        }
        let fresh = self.slots[i].is_none();
        if fresh {
            self.len += 1;
        }
        self.slots[i] = Some(value);
        fresh
    }

    pub fn get(&self, id: u32) -> Option<&T> {
        self.slots.get(id as usize).and_then(Option::as_ref)
    }

    /// Number of entries actually present.
    pub fn len(&self) -> usize {
        self.len
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Highest id present (0 when empty).
    pub fn max_id(&self) -> u32 {
        self.slots.iter().rposition(Option::is_some).unwrap_or(0) as u32
    }

    /// Entries in id order.
    pub fn iter(&self) -> impl Iterator<Item = (u32, &T)> {
        self.slots.iter().enumerate().filter_map(|(i, s)| s.as_ref().map(|v| (i as u32, v)))
    }
}

// ---------------------------------------------------------------------------------------
// Raw INI (faithful to the official clsIniManager)
// ---------------------------------------------------------------------------------------

/// Windows-1252 to Unicode for the 0x80..=0x9F block; the rest of the byte range matches
/// Latin-1.
const CP1252_HIGH: [char; 32] = [
    '€', '\u{81}', '‚', 'ƒ', '„', '…', '†', '‡', 'ˆ', '‰', 'Š', '‹', 'Œ', '\u{8d}', 'Ž', '\u{8f}', '\u{90}', '‘', '’', '“', '”', '•', '–',
    '—', '˜', '™', 'š', '›', 'œ', '\u{9d}', 'ž', 'Ÿ',
];

pub(crate) fn decode_cp1252(bytes: &[u8]) -> String {
    bytes
        .iter()
        .map(|&b| if (0x80..0xA0).contains(&b) { CP1252_HIGH[(b - 0x80) as usize] } else { b as char })
        .collect()
}

#[derive(Default)]
pub(crate) struct RawIni {
    sections: HashMap<String, HashMap<String, String>>,
    /// Section names as written, in file order (for diagnostics and id scanning).
    pub(crate) names: Vec<String>,
}

impl RawIni {
    /// Reads and parses `Recursos/Dat/<file>`. Parse-time oddities (duplicate sections or
    /// keys with different values) are appended to `warnings`.
    pub(crate) fn load(recursos: &Path, file: &'static str, warnings: &mut Vec<Warning>) -> Result<RawIni, Error> {
        let path = recursos.join("Dat").join(file);
        let bytes = std::fs::read(&path).map_err(|source| Error::Io { path, source })?;
        Ok(RawIni::parse(&decode_cp1252(&bytes), file, warnings))
    }

    pub(crate) fn parse(text: &str, file: &'static str, warnings: &mut Vec<Warning>) -> RawIni {
        let mut ini = RawIni::default();
        let mut current: Option<String> = None;
        for raw in text.split('\n') {
            let line = raw.trim_end_matches('\r').trim_start();
            // Comment lines (`'...`). The official reader would store them as odd keys that
            // nothing ever asks for, so skipping them is equivalent and avoids noise.
            if line.is_empty() || line.starts_with('\'') {
                continue;
            }
            if let Some(rest) = line.strip_prefix('[') {
                // Like VB: `[Name]` opens a section, anything after `]` is ignored, and a
                // `[` line without `]` is "nothing" (the current section stays open).
                if let Some(end) = rest.find(']') {
                    let name = rest[..end].trim();
                    let key = name.to_lowercase();
                    if ini.sections.contains_key(&key) {
                        warnings.push(Warning {
                            file,
                            section: name.to_string(),
                            key: String::new(),
                            value: String::new(),
                            message: "duplicate section; entries are merged, first value of each key wins".into(),
                        });
                    } else {
                        ini.names.push(name.to_string());
                        ini.sections.insert(key.clone(), HashMap::new());
                    }
                    current = Some(key);
                }
                continue;
            }
            // VB: `InStr(2, Text, "=")`, so the key has at least one character.
            let Some(eq) = line.char_indices().skip(1).find(|&(_, c)| c == '=').map(|(i, _)| i) else {
                continue;
            };
            let Some(section) = current.as_ref().and_then(|s| ini.sections.get_mut(s)) else {
                continue;
            };
            let key = line[..eq].trim().to_lowercase();
            let value = line[eq + 1..].trim().to_string();
            match section.get(&key) {
                None => {
                    section.insert(key, value);
                }
                Some(old) if *old != value => {
                    let name = current.clone().unwrap_or_default();
                    warnings.push(Warning {
                        file,
                        section: name,
                        key,
                        value,
                        message: format!("duplicate key with a different value (kept first: {old:?})"),
                    });
                }
                Some(_) => {}
            }
        }
        ini
    }

    pub(crate) fn has(&self, section: &str) -> bool {
        self.sections.contains_key(&section.to_lowercase())
    }

    /// Typed accessor for one section. A missing section reads as all-defaults, exactly
    /// like `GetValue` on an unknown section.
    pub(crate) fn sec<'a>(&'a self, file: &'static str, name: &'a str, warnings: &'a mut Vec<Warning>) -> Sec<'a> {
        Sec { file, name, vals: self.sections.get(&name.to_lowercase()), warnings }
    }

    /// `(id, section name)` of every section called `<prefix><digits>` (case-insensitive).
    /// Section names that start with the prefix but have no valid number are reported.
    pub(crate) fn numbered(&self, prefix: &str, file: &'static str, warnings: &mut Vec<Warning>) -> Vec<(u32, String)> {
        let mut out = Vec::new();
        for name in &self.names {
            let lower = name.to_lowercase();
            let Some(rest) = lower.strip_prefix(&prefix.to_lowercase()) else { continue };
            match rest.parse::<u32>() {
                Ok(id) if id > 0 => out.push((id, name.clone())),
                _ => warnings.push(Warning {
                    file,
                    section: name.clone(),
                    key: String::new(),
                    value: String::new(),
                    message: "section name looks like an entry but has no valid positive id; ignored".into(),
                }),
            }
        }
        out.sort_unstable_by_key(|(id, _)| *id);
        out
    }
}

/// Result of parsing a number the way VB6 `Val()` does.
struct Parsed {
    value: f64,
    /// `None` when clean; otherwise why the value is suspicious.
    issue: Option<&'static str>,
}

/// VB6 `Val`: optional sign, digits, optional fraction; anything after is ignored. The
/// data files contain a few decimal commas (`0,5`, which VB would read as 0): we read
/// them as decimals and flag them, like [`crate::ini::num`] does.
fn vb_val(s: &str) -> Parsed {
    let s = s.trim();
    if s.is_empty() {
        return Parsed { value: 0.0, issue: None };
    }
    let b = s.as_bytes();
    let mut i = 0;
    if b[i] == b'+' || b[i] == b'-' {
        i += 1;
    }
    let digits_start = i;
    while i < b.len() && b[i].is_ascii_digit() {
        i += 1;
    }
    let int_digits = i - digits_start;
    let mut frac_digits = 0;
    let mut comma = false;
    if i < b.len() && (b[i] == b'.' || b[i] == b',') {
        comma = b[i] == b',';
        let frac_start = i + 1;
        let mut j = frac_start;
        while j < b.len() && b[j].is_ascii_digit() {
            j += 1;
        }
        frac_digits = j - frac_start;
        if frac_digits > 0 {
            i = j;
        } else {
            comma = false;
        }
    }
    if int_digits + frac_digits == 0 {
        return Parsed { value: 0.0, issue: Some("not a number (read as 0)") };
    }
    let text = s[..i].replace(',', ".");
    let value = text.parse::<f64>().unwrap_or(0.0);
    let issue = if comma {
        Some("decimal comma")
    } else if i < b.len() && !s[i..].trim_start().starts_with('\'') {
        // `Map=399 'destination map`: an inline comment is normal in the files.
        Some("trailing text ignored")
    } else {
        None
    };
    Parsed { value, issue }
}

/// Typed view over one INI section that records a [`Warning`] for suspicious values.
/// A missing key is the default, silently (like VB); a present-but-malformed value warns.
pub(crate) struct Sec<'a> {
    file: &'static str,
    name: &'a str,
    vals: Option<&'a HashMap<String, String>>,
    warnings: &'a mut Vec<Warning>,
}

impl<'a> Sec<'a> {
    pub(crate) fn raw(&self, key: &str) -> Option<&'a str> {
        self.vals?.get(&key.to_lowercase()).map(String::as_str)
    }

    fn warn(&mut self, key: &str, value: &str, message: impl Into<String>) {
        self.warnings.push(Warning {
            file: self.file,
            section: self.name.to_string(),
            key: key.to_string(),
            value: value.to_string(),
            message: message.into(),
        });
    }

    pub(crate) fn warning(&mut self, key: &str, value: &str, message: impl Into<String>) {
        self.warn(key, value, message);
    }

    /// Free text (empty when missing).
    pub(crate) fn text(&self, key: &str) -> String {
        self.raw(key).unwrap_or("").to_string()
    }

    pub(crate) fn num_or(&mut self, key: &str, default: f64) -> f64 {
        let Some(raw) = self.raw(key) else { return default };
        let p = vb_val(raw);
        if let Some(issue) = p.issue {
            self.warn(key, raw, issue);
        }
        p.value
    }

    pub(crate) fn num(&mut self, key: &str) -> f64 {
        self.num_or(key, 0.0)
    }

    pub(crate) fn int_or(&mut self, key: &str, default: i32) -> i32 {
        let Some(raw) = self.raw(key) else { return default };
        let v = self.num(key);
        self.clamp_i32(key, raw, v)
    }

    pub(crate) fn int(&mut self, key: &str) -> i32 {
        self.int_or(key, 0)
    }

    fn clamp_i32(&mut self, key: &str, raw: &str, v: f64) -> i32 {
        let r = v.round();
        if r > i32::MAX as f64 || r < i32::MIN as f64 {
            self.warn(key, raw, "out of 32-bit range (clamped)");
            return if r > 0.0 { i32::MAX } else { i32::MIN };
        }
        r as i32
    }

    pub(crate) fn float(&mut self, key: &str) -> f32 {
        self.num(key) as f32
    }

    /// `Val(x) > 0` (the VB idiom for "flag is set").
    pub(crate) fn flag(&mut self, key: &str) -> bool {
        self.num(key) > 0.0
    }

    /// `Val(x) <> 0`.
    pub(crate) fn nonzero(&mut self, key: &str) -> bool {
        self.num(key) != 0.0
    }

    /// A count that drives an indexed list (`NumQuest` -> `QuestNumber1..n`), clamped to
    /// `0..=max`. Values above `max` warn: either the VB clamps them (`DropCount`) or the
    /// data is surely corrupt.
    pub(crate) fn count(&mut self, key: &str, max: usize) -> usize {
        let Some(raw) = self.raw(key) else { return 0 };
        let v = self.int(key);
        if v < 0 {
            self.warn(key, raw, "negative count (read as 0)");
            return 0;
        }
        let v = v as usize;
        if v > max {
            self.warn(key, raw, format!("count above the limit of {max} (clamped)"));
            return max;
        }
        v
    }

    /// Integer value of one field of `key` (`Val` semantics), warning when it is malformed.
    pub(crate) fn int_of(&mut self, key: &str, whole: &str, field: &str) -> i32 {
        let p = vb_val(field);
        if let Some(issue) = p.issue {
            self.warn(key, whole, format!("field {:?}: {issue}", field.trim()));
        }
        self.clamp_i32(key, whole, p.value)
    }

    /// `Val(ReadField(n, text, sep))` for each field of `key`, padded with zeros up to
    /// `min_len` (VB reads missing fields as 0).
    pub(crate) fn ints(&mut self, key: &str, sep: char, min_len: usize) -> Vec<i32> {
        let Some(raw) = self.raw(key) else { return vec![0; min_len] };
        let mut out: Vec<i32> = raw.split(sep).map(|f| self.int_of(key, raw, f)).collect();
        while out.len() < min_len {
            out.push(0);
        }
        out
    }

    /// Text fields of `key` split at `sep` (not trimmed beyond the whole value).
    pub(crate) fn fields(&self, key: &str, sep: char) -> Vec<String> {
        self.raw(key).map(|r| r.split(sep).map(|s| s.trim().to_string()).collect()).unwrap_or_default()
    }
}

/// VB `Tilde`: upper case with the Spanish acute accents removed. Used to compare class,
/// race and skill names.
pub(crate) fn tilde(s: &str) -> String {
    s.trim()
        .to_uppercase()
        .chars()
        .map(|c| match c {
            'Á' => 'A',
            'É' => 'E',
            'Í' => 'I',
            'Ó' => 'O',
            'Ú' => 'U',
            c => c,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn val_semantics() {
        assert_eq!(vb_val("12").value, 12.0);
        assert_eq!(vb_val("+3").value, 3.0);
        assert_eq!(vb_val("-2.5").value, -2.5);
        assert_eq!(vb_val("").issue, None);
        assert_eq!(vb_val("7 ").issue, None);
        assert_eq!(vb_val("7abc").value, 7.0);
        assert!(vb_val("7abc").issue.is_some());
        assert_eq!(vb_val("399 'mapa destino").issue, None);
        assert_eq!(vb_val("399 'mapa destino").value, 399.0);
        assert!(vb_val("abc").issue.is_some());
        assert_eq!(vb_val("0,555").value, 0.555);
        assert_eq!(vb_val("0,555").issue, Some("decimal comma"));
        assert_eq!(vb_val("1.").value, 1.0);
    }

    #[test]
    fn raw_ini_keeps_apostrophes_and_cp1252() {
        let mut w = Vec::new();
        let bytes = b"[INIT]\r\nName='Casper 01'\r\n[Hechizo1]\r\nPalabrasMagicas=IrX' \xd4A'l\r\n'comment=1\r\nApu\xf1ala=1\r\n";
        let ini = RawIni::parse(&decode_cp1252(bytes), "t.dat", &mut w);
        let mut ww = Vec::new();
        let s = ini.sec("t.dat", "INIT", &mut ww);
        assert_eq!(s.text("name"), "'Casper 01'");
        let s = ini.sec("t.dat", "hechizo1", &mut ww);
        assert_eq!(s.text("PalabrasMagicas"), "IrX' ÔA'l");
        assert_eq!(s.text("apuñala"), "1");
        assert!(w.is_empty() && ww.is_empty());
    }

    #[test]
    fn bad_values_warn_but_do_not_fail() {
        let mut w = Vec::new();
        let ini = RawIni::parse("[S]\nA=x\nB=5\nA=y\n", "t.dat", &mut w);
        assert_eq!(w.len(), 1, "duplicate key with a different value");
        let mut ww = Vec::new();
        let mut s = ini.sec("t.dat", "S", &mut ww);
        assert_eq!(s.int("a"), 0);
        assert_eq!(s.int("b"), 5);
        assert_eq!(s.int_or("zzz", 9), 9);
        assert_eq!(ww.len(), 1);
    }
}

//! INI reader with the quirks of the AO resource files: Windows-1252 text, inline `'`
//! comments, case-insensitive sections and keys, and decimal commas (`Speed=0,555`).
use std::collections::HashMap;
use std::path::Path;

pub struct Ini {
    sections: HashMap<String, HashMap<String, String>>,
}

impl Ini {
    pub fn load(path: &Path) -> Ini {
        let bytes = std::fs::read(path).unwrap_or_default();
        Ini::parse(&bytes)
    }

    pub fn parse(bytes: &[u8]) -> Ini {
        let text: String = bytes.iter().map(|&b| b as char).collect();
        let mut sections: HashMap<String, HashMap<String, String>> = HashMap::new();
        let mut section = String::new();
        for raw in text.lines() {
            let line = raw.split('\'').next().unwrap_or("").trim();
            if line.is_empty() {
                continue;
            }
            if line.starts_with('[') {
                section = line.trim_matches(|c| c == '[' || c == ']').to_ascii_lowercase();
                continue;
            }
            if let Some((k, v)) = line.split_once('=') {
                sections.entry(section.clone()).or_default().insert(k.trim().to_ascii_lowercase(), v.trim().to_string());
            }
        }
        Ini { sections }
    }

    pub fn section(&self, name: &str) -> Option<&HashMap<String, String>> {
        self.sections.get(&name.to_ascii_lowercase())
    }

    pub fn sections(&self) -> impl Iterator<Item = (&String, &HashMap<String, String>)> {
        self.sections.iter()
    }
}

/// Numeric value of a key, accepting decimal commas; 0 when missing or invalid.
pub fn num(section: Option<&HashMap<String, String>>, key: &str) -> f32 {
    section.and_then(|s| s.get(key)).map(|v| v.replace(',', ".")).and_then(|v| v.parse::<f32>().ok()).unwrap_or(0.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quirks() {
        let ini = Ini::parse(b"[BODY1]\nWalk1=41653 ' arriba\nSpeed=0,555\n");
        let s = ini.section("body1");
        assert_eq!(num(s, "walk1"), 41653.0);
        assert!((num(s, "speed") - 0.555).abs() < 1e-6);
        assert_eq!(num(s, "missing"), 0.0);
    }
}

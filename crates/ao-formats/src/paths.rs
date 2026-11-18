//! Locating the AO20 `Recursos` folder.
use std::path::PathBuf;

/// Root of the AO20 resources: `$AO_RECURSOS`, or the usual local checkouts next to the
/// project. Configuration files can override it explicitly.
pub fn recursos_dir() -> PathBuf {
    if let Ok(p) = std::env::var("AO_RECURSOS") {
        return PathBuf::from(p);
    }
    let here = env!("CARGO_MANIFEST_DIR");
    // The full set (public repo + official client, tools/ao20/build_full.cmd) first.
    let candidates = [
        "../ao20_full".to_string(),
        "../../ao20_full".to_string(),
        format!("{here}/../../../ao20_full"),
        "../ao20/Recursos".to_string(),
        "../../ao20/Recursos".to_string(),
        format!("{here}/../../../ao20/Recursos"),
    ];
    for c in candidates {
        let p = PathBuf::from(c);
        if p.join("Mapas").is_dir() {
            return p;
        }
    }
    PathBuf::from("Recursos")
}

/// The public ao-org/Recursos checkout (its maps match its own `Dat/npcs.dat` NPC numbers;
/// the official client's maps place some NPCs by newer numbers). None if absent.
pub fn public_recursos_dir() -> Option<PathBuf> {
    let here = env!("CARGO_MANIFEST_DIR");
    ["../ao20/Recursos".to_string(), "../../ao20/Recursos".to_string(), format!("{here}/../../../ao20/Recursos")].into_iter().map(PathBuf::from).find(|p| p.join("Mapas").is_dir())
}

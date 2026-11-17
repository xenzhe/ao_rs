//! Locating the AO20 `Recursos` folder.
use std::path::PathBuf;

/// Root of the AO20 resources: `$AO_RECURSOS`, or the usual local checkouts next to the
/// project. Configuration files can override it explicitly.
pub fn recursos_dir() -> PathBuf {
    if let Ok(p) = std::env::var("AO_RECURSOS") {
        return PathBuf::from(p);
    }
    let here = env!("CARGO_MANIFEST_DIR");
    let candidates = [
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

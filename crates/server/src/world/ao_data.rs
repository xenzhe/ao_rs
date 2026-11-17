//! Official AO20 game data (Recursos/Dat): objects, NPCs, spells, classes and races.
//! Parsed once per process and shared (tests start many servers).
use ao_formats::dat::balance::Balance;
use ao_formats::dat::npcs::Npc as NpcData;
use ao_formats::dat::obj::ObjData;
use ao_formats::dat::spells::Spell as SpellData;
use ao_formats::dat::Table;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};
use tracing::warn;

pub struct AoData {
    pub objs: Table<ObjData>,
    pub npcs: Table<NpcData>,
    pub spells: Table<SpellData>,
    pub balance: Balance,
}

impl AoData {
    /// Loads (or reuses) the official data of a `Recursos` folder.
    pub fn shared(recursos: &Path) -> Arc<AoData> {
        static CACHE: OnceLock<Mutex<HashMap<PathBuf, Arc<AoData>>>> = OnceLock::new();
        let cache = CACHE.get_or_init(|| Mutex::new(HashMap::new()));
        let mut cache = cache.lock().unwrap();
        cache.entry(recursos.to_path_buf()).or_insert_with(|| Arc::new(AoData::load(recursos))).clone()
    }

    fn load(recursos: &Path) -> AoData {
        macro_rules! read {
            ($m:ident) => {
                match ao_formats::dat::$m::load(recursos) {
                    Ok(l) => {
                        for w in &l.warnings {
                            warn!(?w, "dato oficial");
                        }
                        l.data
                    }
                    Err(e) => panic!("no se pudo leer {} de {}: {e:?}", stringify!($m), recursos.display()),
                }
            };
        }
        AoData { objs: read!(obj), npcs: read!(npcs), spells: read!(spells), balance: read!(balance) }
    }
}

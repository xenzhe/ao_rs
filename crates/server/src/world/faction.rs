//! Per-player faction state (`Player.faction`). OWNER: Opus C (docs/P2_CONTRATO.md).
use crate::clock::GameTime;
use ao_rules::faction::{self as rules, FactionTables};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};

/// What is saved of `t_Facciones` (CharacterPersistence.bas `SetupUserFactionInfo`).
#[derive(Debug, Clone, PartialEq)]
pub struct FactionRecord {
    /// `Status` (`e_Facciones`).
    pub status: u8,
    /// `ciudadanosMatados`.
    pub citizens_killed: u32,
    /// `CriminalesMatados`.
    pub criminals_killed: u32,
    /// `RecompensasReal`: current army rank.
    pub royal_rank: u8,
    /// `RecompensasCaos`: current legion rank.
    pub chaos_rank: u8,
    /// `RecibioArmaduraReal`: last army rank whose rewards were given.
    pub royal_rewarded: u8,
    /// `RecibioArmaduraCaos`.
    pub chaos_rewarded: u8,
    /// `Reenlistadas`.
    pub enlistments: u32,
    /// `NivelIngreso`.
    pub entry_level: u32,
    /// `MatadosIngreso`.
    pub entry_kills: u32,
    /// `FactionScore`.
    pub score: i32,
}

impl Default for FactionRecord {
    /// A new character is a citizen.
    fn default() -> Self {
        FactionRecord {
            status: rules::CITIZEN,
            citizens_killed: 0,
            criminals_killed: 0,
            royal_rank: 0,
            chaos_rank: 0,
            royal_rewarded: 0,
            chaos_rewarded: 0,
            enlistments: 0,
            entry_level: 0,
            entry_kills: 0,
            score: 0,
        }
    }
}

impl FactionRecord {
    /// Current rank of the faction the player is in (0 if none).
    pub fn rank(&self) -> u8 {
        if rules::is_royal(self.status) {
            self.royal_rank
        } else if rules::is_chaos(self.status) {
            self.chaos_rank
        } else {
            0
        }
    }
}

#[derive(Debug)]
pub struct FactionPiece {
    pub rec: FactionRecord,
    /// `flags.Seguro` (citizens). Off by default here: see `faction_on_enter`.
    pub safe: bool,
    /// `flags.LegionarySecure` (on at login).
    pub legion_safe: bool,
    /// `flags.RecentKillers`: who killed this player and when (by character name).
    pub recent_killers: Vec<(String, GameTime)>,
}

impl Default for FactionPiece {
    fn default() -> Self {
        FactionPiece { rec: FactionRecord::default(), safe: false, legion_safe: true, recent_killers: Vec::new() }
    }
}

/// `MaxRecentKillToStore` (Declares.bas).
pub const MAX_RECENT_KILLERS: usize = 5;

fn read_cp1252(path: &Path) -> String {
    // cp1252 maps 0xA0..=0xFF like Latin-1; the few 0x80..0x9F glyphs are not in the titles.
    std::fs::read(path).map(|b| b.iter().map(|&c| c as char).collect()).unwrap_or_default()
}

/// rangos_faccion.dat and recompensas_faccion.dat of `recursos/Dat`, loaded once per path.
pub fn tables(recursos: &Path) -> Arc<FactionTables> {
    static CACHE: OnceLock<Mutex<HashMap<PathBuf, Arc<FactionTables>>>> = OnceLock::new();
    let mut cache = CACHE.get_or_init(Default::default).lock().unwrap();
    cache
        .entry(recursos.to_path_buf())
        .or_insert_with(|| {
            let dat = recursos.join("Dat");
            let (royal, chaos) = rules::parse_ranks(&read_cp1252(&dat.join("rangos_faccion.dat")));
            let rewards = rules::parse_rewards(&read_cp1252(&dat.join("recompensas_faccion.dat")));
            Arc::new(FactionTables { royal, chaos, rewards })
        })
        .clone()
}

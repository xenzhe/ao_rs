//! Game masters: commands, roles, penalties (jail, silence, ban), logs. OWNER: Opus C (docs/P4_CONTRATO.md). Wrapped in `C2S::Gm` / `S2C::Gm`.
use serde::{Deserialize, Serialize};

/// `e_PlayerType` (Declares.bas) as an ordered level. RoleMaster is not ported.
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default, Hash)]
pub enum GmRank {
    #[default]
    User,
    Consejero,
    SemiDios,
    Dios,
    Admin,
}

impl GmRank {
    pub fn from_level(v: i64) -> GmRank {
        match v {
            1 => GmRank::Consejero,
            2 => GmRank::SemiDios,
            3 => GmRank::Dios,
            v if v >= 4 => GmRank::Admin,
            _ => GmRank::User,
        }
    }

    pub fn level(self) -> i64 {
        self as i64
    }

    pub fn parse(s: &str) -> Option<GmRank> {
        Some(match s.trim().to_lowercase().as_str() {
            "user" | "usuario" | "0" => GmRank::User,
            "consejero" | "1" => GmRank::Consejero,
            "semidios" | "semidiós" | "2" => GmRank::SemiDios,
            "dios" | "3" => GmRank::Dios,
            "admin" | "4" => GmRank::Admin,
            _ => return None,
        })
    }

    pub fn label(self) -> &'static str {
        match self {
            GmRank::User => "Usuario",
            GmRank::Consejero => "Consejero",
            GmRank::SemiDios => "Semidiós",
            GmRank::Dios => "Dios",
            GmRank::Admin => "Admin",
        }
    }

    pub fn is_gm(self) -> bool {
        self > GmRank::User
    }
}

/// One online player as the GM panel lists it.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct GmPlayerRow {
    pub id: u32,
    pub name: String,
    pub level: u32,
    pub map: u16,
    pub x: i32,
    pub y: i32,
    pub rank: GmRank,
    pub dead: bool,
    /// Seconds of jail / silence left (0 = none).
    pub jail_secs: u32,
    pub silence_secs: u32,
}

/// One row of the GM log (`LogGM`).
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct GmLogRow {
    /// Unix seconds.
    pub at: i64,
    pub gm: String,
    pub action: String,
    pub target: String,
    pub detail: String,
}

/// Client -> server. Only GMs get an answer; anyone else is ignored without a word.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum GmC2S {
    /// Online list and recent log for the panel (`HandleGMPanel`, `HandleRequestUserList`).
    Refresh,
    /// A command line from the panel (same path and permissions as typing it in the chat).
    Command { line: String },
}

/// Server -> client.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum GmS2C {
    /// Your rank. Sent on entering only to GMs (and when it changes).
    Rank { rank: GmRank },
    Online { players: Vec<GmPlayerRow> },
    Log { entries: Vec<GmLogRow> },
    /// Open help requests (`/GM` questions and `/DENUNCIAR` reports), oldest first.
    /// Sent with every panel refresh (`HandleSOSShowList` / `WriteShowSOSForm`).
    Sos { entries: Vec<GmSosRow> },
}

/// One open help request (`Ayuda` queue entry: name, text, kind, time).
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct GmSosRow {
    /// What `/ATENDER id` takes.
    pub id: i64,
    pub name: String,
    /// `TipoDeConsulta` ("Consulta regular", "Denuncia a X"...).
    pub kind: String,
    pub text: String,
    /// Unix seconds.
    pub at: i64,
    /// The author is online now.
    pub online: bool,
}

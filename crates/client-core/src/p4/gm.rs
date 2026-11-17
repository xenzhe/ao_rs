//! What the client knows about gm (from `S2C::Gm`) and its actions (`impl Game`).
//! OWNER: Opus C (docs/P4_CONTRATO.md).
//!
//! The rank only decides whether the panel shows; every command is checked again by the
//! server, so a tampered client gains nothing.
use crate::state::Game;
use ao_protocol::p4::gm::{GmLogRow, GmPlayerRow, GmRank};
use ao_protocol::{GmC2S, GmS2C, C2S};

#[derive(Debug, Default)]
pub struct GmState {
    /// Own rank as the server said (User until told otherwise).
    pub rank: GmRank,
    pub online: Vec<GmPlayerRow>,
    /// Newest first.
    pub log: Vec<GmLogRow>,
    /// Bumped on every list received (the panel uses it to refresh on its own).
    pub updates: u32,
}

impl GmState {
    pub fn apply(&mut self, msg: &GmS2C) {
        match msg {
            GmS2C::Rank { rank } => self.rank = *rank,
            GmS2C::Online { players } => {
                self.online = players.clone();
                self.updates = self.updates.wrapping_add(1);
            }
            GmS2C::Log { entries } => self.log = entries.clone(),
        }
    }

    pub fn is_gm(&self) -> bool {
        self.rank.is_gm()
    }

    /// Online players whose name contains `filter` (ignoring case), in the server order.
    pub fn filtered(&self, filter: &str) -> Vec<&GmPlayerRow> {
        let f = filter.trim().to_lowercase();
        self.online.iter().filter(|p| f.is_empty() || p.name.to_lowercase().contains(&f)).collect()
    }
}

/// The command line a panel action sends (same syntax as typing it).
pub fn action_line(action: &str, name: &str, minutes: u32, reason: &str) -> String {
    let reason = if reason.trim().is_empty() { "sin motivo" } else { reason.trim() };
    match action {
        "ir" => format!("/IRA {name}"),
        "traer" => format!("/SUM {name}"),
        "info" => format!("/INFO {name}"),
        "silenciar" => format!("/SILENCIAR {name}@{minutes}"),
        "carcel" => format!("/CARCEL {name}@{reason}@{minutes}"),
        "echar" => format!("/ECHAR {name}"),
        "revivir" => format!("/REVIVIR {name}"),
        "penas" => format!("/PENAS {name}"),
        _ => String::new(),
    }
}

impl Game {
    /// Asks the server for the online list and the recent log (GMs only get an answer).
    pub fn gm_refresh(&mut self) {
        self.send(&C2S::Gm(GmC2S::Refresh));
    }

    /// Sends a GM command line through the panel.
    pub fn gm_command(&mut self, line: impl Into<String>) {
        let line = line.into();
        if !line.is_empty() {
            self.send(&C2S::Gm(GmC2S::Command { line }));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(name: &str) -> GmPlayerRow {
        GmPlayerRow { id: 1, name: name.into(), level: 1, map: 1, x: 1, y: 1, rank: GmRank::User, dead: false, jail_secs: 0, silence_secs: 0 }
    }

    #[test]
    fn state_follows_the_server_and_filters() {
        let mut s = GmState::default();
        assert!(!s.is_gm());
        s.apply(&GmS2C::Rank { rank: GmRank::SemiDios });
        assert!(s.is_gm());
        s.apply(&GmS2C::Online { players: vec![row("Ana"), row("Beto"), row("Mariana")] });
        assert_eq!(s.filtered("ANA").iter().map(|p| p.name.as_str()).collect::<Vec<_>>(), ["Ana", "Mariana"]);
        assert_eq!(s.filtered("").len(), 3);
        assert_eq!(s.updates, 1);
    }

    #[test]
    fn panel_actions_are_the_typed_commands() {
        assert_eq!(action_line("carcel", "Beto", 10, ""), "/CARCEL Beto@sin motivo@10");
        assert_eq!(action_line("silenciar", "Beto", 5, "x"), "/SILENCIAR Beto@5");
        assert_eq!(action_line("ir", "Beto", 0, ""), "/IRA Beto");
        assert_eq!(action_line("nada", "Beto", 0, ""), "");
    }
}

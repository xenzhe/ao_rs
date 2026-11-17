//! Per-player party state (`Player.party`). OWNER: Opus C (docs/P2_CONTRATO.md).
use std::collections::HashMap;

/// `t_User.Grupo` (the member side) plus `flags.SeguroParty`.
#[derive(Debug)]
pub struct PartyPiece {
    /// `Grupo.Id`: the party this player is in (key of `World.parties`).
    pub id: Option<u64>,
    /// `Grupo.PropuestaDe`: who invited this player and is waiting for an answer.
    pub invited_by: Option<u32>,
    /// `flags.SeguroParty`: on at login; nobody can invite you while it is on.
    pub safe: bool,
}

impl Default for PartyPiece {
    fn default() -> Self {
        PartyPiece { id: None, invited_by: None, safe: true }
    }
}

/// One party (`Tgrupo` of the leader).
#[derive(Debug, Clone)]
pub struct Party {
    pub leader: u32,
    /// `Miembros(1 To 6)`: leader first, in joining order.
    pub members: Vec<u32>,
}

/// World-wide party registry (`World.parties`).
#[derive(Debug, Default)]
pub struct Parties {
    pub by_id: HashMap<u64, Party>,
    /// `GetNextId` (ModGrupos.bas).
    pub next_id: u64,
}

impl Parties {
    pub fn create(&mut self, leader: u32) -> u64 {
        self.next_id += 1;
        let id = self.next_id;
        self.by_id.insert(id, Party { leader, members: vec![leader] });
        id
    }
}

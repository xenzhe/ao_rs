//! Parties (grupos). OWNER: Opus C (docs/P2_CONTRATO.md). Wrapped in `C2S::Party` / `S2C::Party`.
use serde::{Deserialize, Serialize};

/// Client -> server.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum PartyC2S {
    /// Invite the player `target` (`WorkLeftClick` with `e_Skill.Grupo` + `InvitarMiembro`).
    /// The sender becomes the leader if it had no party.
    Invite { target: u32 },
    /// Answer the pending invitation (`HandleResponderPregunta`, question 1).
    Respond { accept: bool },
    /// `HandleAbandonarGrupo`: the leader disbands, a member leaves.
    Leave,
    /// `EcharMiembro` (leader only).
    Kick { member: u32 },
    /// `HandlePartyToggle`: the party "seguro"; while on nobody can invite you.
    ToggleSafe,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct PartyMember {
    pub id: u32,
    pub name: String,
}

/// Server -> client.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum PartyS2C {
    /// `WritePreguntaBox`: `name` invites you to its party.
    Invited { from: u32, name: String },
    /// `PrepareUpdateGroupInfo`: the party (leader first). Empty = not in a party.
    Members { leader: u32, members: Vec<PartyMember> },
    /// `WritePartySafeOn/Off`.
    Safe { on: bool },
}

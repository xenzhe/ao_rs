//! Vault (bank) state. OWNER: bank system (P1, Sonnet 2).
//! Persisted by `db` (slots and gold); add runtime-only fields freely.
use super::inventory::InvItem;

/// AO20 bank size (Declares.bas MAX_BANCOINVENTORY_SLOTS).
pub const MAX_BANK_SLOTS: usize = 42;

#[derive(Clone, Debug)]
pub struct BankState {
    /// Index 0 = slot 1.
    pub slots: Vec<Option<InvItem>>,
    pub gold: u32,
    /// Banker NPC currently open (deposits/withdrawals only while open and near).
    pub open_with: Option<u32>,
}

impl Default for BankState {
    fn default() -> Self {
        BankState { slots: vec![None; MAX_BANK_SLOTS], gold: 0, open_with: None }
    }
}

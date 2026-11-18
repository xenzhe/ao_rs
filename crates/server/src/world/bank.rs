//! Vault (bank) state. OWNER: bank system (P1, Sonnet 2).
//! Persisted by `db` (slots and gold); add runtime-only fields freely.
use super::inventory::InvItem;

/// AO20 bank size (Declares.bas MAX_BANCOINVENTORY_SLOTS).
pub const MAX_BANK_SLOTS: usize = 42;

/// Largest stack in one slot (`MaxInventoryObjs`, Example.Configuracion.ini; read by
/// InvUsuario.GetMaxInvOBJ). Applies to inventory and vault alike.
pub const MAX_STACK: u32 = 10_000;

/// Max Manhattan distance (VB `Distancia`) to a banker to open the vault
/// (Protocol.HandleBankStart).
pub const OPEN_RANGE: i32 = 6;
/// Max Manhattan distance for deposits/withdrawals (Protocol.HandleBankDeposit,
/// HandleBankExtractGold, HandleBankDepositGold).
pub const OPERATE_RANGE: i32 = 10;

#[derive(Clone, Debug)]
pub struct BankState {
    /// Index 0 = slot 1.
    pub slots: Vec<Option<InvItem>>,
    pub gold: u32,
    /// Banker NPC currently open (deposits/withdrawals only while open and near).
    pub open_with: Option<u32>,
    /// The open window is the vault of this clan (TSAO BoveClan), not the player's own.
    pub clan: Option<i64>,
}

impl Default for BankState {
    fn default() -> Self {
        BankState { slots: vec![None; MAX_BANK_SLOTS], gold: 0, open_with: None, clan: None }
    }
}

/// A clan vault in memory: shared by every member who has it open.
#[derive(Clone, Debug)]
pub struct ClanVault {
    pub slots: Vec<Option<InvItem>>,
    pub gold: u32,
}

/// Where `amount` units of `obj` go inside `slots`: the first slot already holding `obj`
/// with room for the whole amount, else the first empty one (never splits a stack), like
/// modBanco.UserDejaObj / UserReciveObj. Returns the 0-based index.
pub fn find_slot(slots: &[Option<InvItem>], obj: u16, amount: u32) -> Option<usize> {
    let stack = slots.iter().position(|s| s.is_some_and(|i| i.obj == obj && i.amount + amount <= MAX_STACK));
    stack.or_else(|| slots.iter().position(Option::is_none))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn it(obj: u16, amount: u32) -> Option<InvItem> {
        Some(InvItem { obj, amount })
    }

    #[test]
    fn stacks_before_using_an_empty_slot() {
        let slots = [None, it(7, 5), it(7, 2)];
        assert_eq!(find_slot(&slots, 7, 3), Some(1));
        assert_eq!(find_slot(&slots, 8, 3), Some(0));
    }

    #[test]
    fn full_stacks_are_skipped_and_never_split() {
        let slots = [it(7, MAX_STACK - 1), it(7, 10), None];
        assert_eq!(find_slot(&slots, 7, 2), Some(1));
        let full = [it(7, MAX_STACK), it(8, 1)];
        assert_eq!(find_slot(&full, 7, 1), None);
    }
}

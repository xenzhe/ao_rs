//! Vault over the wire, with the real server and a bot. The cases that need items, gold
//! and a banker NPC (stacking, free slots, equipped items, range, traps, persistence across
//! logout/login) are unit tests inside `systems/bank.rs`, where the world state can be
//! set up directly; here only the client-facing guarantees of the protocol.
use ao_protocol::{C2S, S2C};
use ao_testkit::TestServer;

fn bank_traffic(m: &S2C) -> bool {
    matches!(m, S2C::BankOpen { .. } | S2C::BankSlot { .. } | S2C::BankGold { .. } | S2C::InventorySlot { .. } | S2C::Vitals { .. })
}

/// A client that talks to the vault without having opened it (a late packet after
/// closing, or a cheat client) gets nothing back and is not punished for a single try.
#[test]
fn vault_packets_with_the_vault_closed_do_nothing() {
    let s = TestServer::start();
    let mut bot = s.bot("Curioso");
    bot.send(&C2S::BankDeposit { slot: 1, amount: 1 });
    bot.send(&C2S::BankWithdraw { slot: 1, amount: 1 });
    bot.send(&C2S::BankDepositGold { amount: 10 });
    bot.send(&C2S::BankWithdrawGold { amount: 10 });
    bot.send(&C2S::BankClose);
    bot.sync();
    assert_eq!(bot.count(bank_traffic), 0, "nothing is moved or announced: {:?}", bot.log());
    assert!(bot.game.local.bank.is_none());
    assert!(bot.kicked().is_none() && !bot.closed(), "a closed-vault packet is refused, not punished");
}

/// Without a banker nearby nothing can be opened by clicking on empty ground.
#[test]
fn clicking_empty_ground_does_not_open_the_vault() {
    let s = TestServer::start();
    let mut bot = s.bot("Clicker");
    let (x, y) = bot.pos();
    bot.send(&C2S::Interact { tx: x + 1, ty: y });
    bot.sync();
    assert!(bot.game.local.bank.is_none());
    assert_eq!(bot.count(|m| matches!(m, S2C::BankOpen { .. })), 0);
}

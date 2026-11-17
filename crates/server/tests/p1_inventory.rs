//! Inventory messages from real bots (ao-testkit, virtual clock): the traps that need no
//! objects. Everything that needs objects in the inventory lives in the crate tests of
//! `systems/inventory.rs` until `/dar` is hooked to the chat.
use ao_protocol::{C2S, S2C};
use ao_testkit::TestServer;

#[test]
fn login_sends_the_whole_inventory_and_the_purse() {
    let s = TestServer::start();
    let b = s.bot("Mochilero");
    assert_eq!(b.count(|m| matches!(m, S2C::InventorySlot { .. })), 42, "una casilla por slot");
    assert!(b.count(|m| matches!(m, S2C::Vitals { .. })) >= 1);
}

#[test]
fn honest_mistakes_are_messages() {
    let s = TestServer::start();
    let mut b = s.bot("Torpe");
    b.send(&C2S::PickUp);
    b.send(&C2S::DropGold { amount: 50 });
    b.send(&C2S::UseItem { slot: 3 });
    b.send(&C2S::EquipItem { slot: 3 });
    b.send(&C2S::DropItem { slot: 3, amount: 5 });
    b.send(&C2S::SwapSlots { a: 1, b: 30 });
    b.send(&C2S::DropGold { amount: u32::MAX });
    s.advance(100);
    b.sync();
    assert!(b.console().any(|t| t.contains("nada")), "recoger sin nada");
    assert!(b.console().any(|t| t.contains("oro")), "oro que no tiene");
    assert!(b.console().any(|t| t.contains("desbloqueado")), "casilla bloqueada sin mochila");
    assert!(!b.closed(), "nada de esto es trampa");
    assert_eq!(b.game.local.gold, 0, "oro intacto");
    assert!(b.game.local.ground.is_empty(), "no aparece nada en el suelo");
}

#[test]
fn impossible_slots_end_in_a_kick() {
    let s = TestServer::start();
    let mut b = s.bot("Hacker");
    for _ in 0..30 {
        b.send(&C2S::UseItem { slot: 0 });
        b.send(&C2S::DropItem { slot: 250, amount: 1 });
    }
    s.advance(100);
    b.wait_until("expulsado", |b| b.closed());
    assert!(b.kicked().is_some_and(|r| r.contains("casilla")));
}

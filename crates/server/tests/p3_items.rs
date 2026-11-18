//! P3 item windows: the client actions of `client_core::p3::items` (what the inventory, vault,
//! merchant and trade windows call) against a real server with bots. Each action is the
//! window's own `plan_*` (the checks the window makes before sending), and the resulting
//! message goes through `Bot::send` so the bot's barrier (`sync`) counts it.
use ao_formats::dat::npcs;
use ao_formats::{recursos_dir, AoMap};
use ao_protocol::{TradeC2S, C2S};
use ao_rules::trade::{buy_price, sale_price};
use ao_testkit::{Bot, TestServer};

/// Ordinary stackable objects of obj.dat (as in the P2 trade tests) and the gold coin.
const OBJ_A: u16 = 38;
const GOLD_OBJ: u16 = 12;
/// Krix, the banker of Ullathorpe.
const BANKER: (i32, i32) = (75, 47);

/// Runs the client action `$plan` of `$bot`: false if the client refused it, otherwise the
/// message is sent (counted by the bot) and the call returns true. No waiting.
macro_rules! tx {
    ($bot:expr, $plan:ident ( $($arg:expr),* )) => {{
        let p = $bot.game.$plan($($arg),*);
        match p {
            Ok(m) => {
                $bot.send(&m);
                true
            }
            Err(_) => false,
        }
    }};
}

/// `tx!` and wait until the server handled it and the bot got the answer.
macro_rules! act {
    ($s:expr, $bot:expr, $plan:ident ( $($arg:expr),* )) => {{
        let ok = tx!($bot, $plan($($arg),*));
        $s.advance(50);
        $bot.sync();
        ok
    }};
}

/// Two rounds: what one bot caused is on the other's wire only after the server handled
/// the first round, so the second round's barrier covers it.
fn settle(s: &TestServer, bots: &mut [&mut Bot]) {
    for _ in 0..2 {
        s.advance(50);
        for b in bots.iter_mut() {
            b.sync();
        }
    }
}

fn give(s: &TestServer, bot: &mut Bot, obj: u16, amount: u32) {
    bot.say(&format!("/dar {obj} {amount}"));
    s.advance(50);
    bot.sync();
}

fn count(bot: &Bot, obj: u16) -> u32 {
    bot.game.local.count_of(obj)
}

fn slot_of(bot: &Bot, obj: u16) -> u8 {
    bot.game.local.inventory.iter().position(|s| s.is_some_and(|i| i.obj == obj)).map(|i| i as u8 + 1).expect("in the inventory")
}

#[test]
fn inventory_actions_move_objects_and_refuse_nonsense() {
    let s = TestServer::start_at(1, 77, 49);
    let mut bot = s.bot("Mochilero");
    give(&s, &mut bot, OBJ_A, 10);
    give(&s, &mut bot, GOLD_OBJ, 100);
    assert_eq!(count(&bot, OBJ_A), 10);
    assert_eq!(bot.game.local.gold, 100);

    // Reorder: slot 1 -> slot 5.
    let first = slot_of(&bot, OBJ_A);
    assert!(act!(s, bot, plan_swap(first, 5)));
    assert_eq!(bot.game.local.slot(5).map(|i| i.obj), Some(OBJ_A));

    // Drop three on the floor, then pick them up again.
    assert!(act!(s, bot, plan_drop(5, 3)));
    assert_eq!(count(&bot, OBJ_A), 7);
    assert!(bot.game.local.ground.values().any(|g| *g == (OBJ_A, 3)), "{:?}", bot.game.local.ground);
    bot.send(&C2S::PickUp);
    s.advance(50);
    bot.sync();
    assert_eq!(count(&bot, OBJ_A), 10);
    assert!(bot.game.local.ground.is_empty());

    // More than the stack: clamped to what is there, not a cheat.
    assert!(act!(s, bot, plan_drop(5, 9999)));
    assert_eq!(count(&bot, OBJ_A), 0);
    bot.send(&C2S::PickUp);
    s.advance(50);
    bot.sync();
    assert_eq!(count(&bot, OBJ_A), 10);

    // Gold.
    assert!(act!(s, bot, plan_drop_gold(40)));
    assert_eq!(bot.game.local.gold, 60);
    assert!(bot.game.local.ground.values().any(|g| *g == (GOLD_OBJ, 40)), "the gold lies near: {:?}", bot.game.local.ground);

    // Refused on the client: nothing is sent, nobody is punished.
    for _ in 0..30 {
        assert!(!bot.game.inv_drop(30, 1), "empty slot");
        assert!(!bot.game.inv_swap(5, 5));
        assert!(!bot.game.inv_swap(1, 43));
        assert!(!bot.game.inv_use(40));
        assert!(!bot.game.inv_drop_gold(0));
    }
    s.advance(50);
    bot.sync();
    assert!(!bot.closed() && bot.kicked().is_none());
    assert!(bot.console().any(|t| t.contains("vacía")));
}

#[test]
fn equipping_and_unequipping_through_the_window_action() {
    let s = TestServer::start_at(1, 77, 49);
    let mut bot = s.bot("Guerrero");
    let objs = ao_formats::dat::obj::load(&recursos_dir()).unwrap().data;
    let (weapon, _) = objs
        .iter()
        .find(|(_, o)| o.obj_type == ao_formats::dat::obj::ObjType::Weapon && o.weapon_anim > 0 && o.min_level <= 1 && o.forbidden_classes.is_empty() && o.forbidden_races.is_empty() && !o.male_only && !o.female_only && o.royal == 0 && o.chaos == 0 && o.two_handed == 0)
        .expect("a free weapon");
    give(&s, &mut bot, weapon as u16, 1);
    let slot = slot_of(&bot, weapon as u16);
    assert!(act!(s, bot, plan_equip(slot)));
    assert!(bot.game.local.slot(slot).is_some_and(|i| i.equipped), "{:?}", bot.console().collect::<Vec<_>>());
    assert!(act!(s, bot, plan_equip(slot)));
    assert!(!bot.game.local.slot(slot).is_some_and(|i| i.equipped), "the same action takes it off");
}

#[test]
fn vault_deposit_and_withdraw_through_the_client() {
    let s = TestServer::start_at(1, BANKER.0, BANKER.1 + 2);
    let mut bot = s.bot("Ahorrador");
    give(&s, &mut bot, OBJ_A, 10);
    give(&s, &mut bot, GOLD_OBJ, 500);
    // Closed vault: refused locally.
    assert!(!bot.game.bank_deposit(slot_of(&bot, OBJ_A), 1));
    assert!(!bot.game.bank_deposit_gold(10));
    bot.send(&C2S::Interact { tx: BANKER.0, ty: BANKER.1 });
    s.advance(50);
    bot.sync();
    assert!(bot.game.local.bank.is_some(), "{:?}", bot.console().collect::<Vec<_>>());

    let inv_slot = slot_of(&bot, OBJ_A);
    assert!(tx!(bot, plan_deposit(inv_slot, 4)));
    assert!(act!(s, bot, plan_deposit_gold(200)));
    assert_eq!(count(&bot, OBJ_A), 6);
    assert_eq!(bot.game.local.gold, 300);
    let b = bot.game.local.bank.as_ref().unwrap();
    assert_eq!(b.gold, 200);
    assert_eq!(b.slots.iter().flatten().map(|i| (i.obj, i.amount)).collect::<Vec<_>>(), vec![(OBJ_A, 4)]);

    // Ask for more than there is: clamped, the vault empties.
    assert!(tx!(bot, plan_withdraw(1, 99)));
    assert!(act!(s, bot, plan_withdraw_gold(80)));
    assert_eq!(count(&bot, OBJ_A), 10);
    assert_eq!(bot.game.local.gold, 380);
    assert!(bot.game.local.bank.as_ref().unwrap().slots.iter().all(|i| i.is_none()));
    assert!(!bot.game.bank_withdraw(1, 1), "empty vault slot");

    bot.send(&C2S::BankClose);
    s.advance(50);
    bot.sync();
    assert!(bot.game.local.bank.is_none());
    assert!(!bot.closed());
}

#[test]
fn merchant_buy_and_sell_through_the_client() {
    let dat = npcs::load(&recursos_dir()).unwrap().data;
    let map = AoMap::load(&recursos_dir(), 1).unwrap();
    let m = map.npcs.iter().find(|e| dat.get(e.npc as u32).is_some_and(|d| d.trades == 1 && !d.name.is_empty() && !d.inventory.is_empty())).copied().expect("a merchant");
    let s = TestServer::start_at(1, m.x, m.y + 2);
    let mut bot = s.bot("Cliente");
    give(&s, &mut bot, GOLD_OBJ, 10_000);
    assert!(!bot.game.shop_buy(1, 1), "no window, no purchase");
    bot.send(&C2S::Interact { tx: m.x, ty: m.y });
    s.advance(50);
    bot.sync();
    let shop = bot.game.local.social.trade.shop.clone().unwrap_or_else(|| panic!("shop: {:?}", bot.console().collect::<Vec<_>>()));
    let objs = ao_formats::dat::obj::load(&recursos_dir()).unwrap().data;
    let (&slot, item) = shop.items.iter().find(|(_, i)| i.price > 0 && i.amount >= 2).expect("stock");
    let value = objs.get(item.obj as u32).unwrap().value;
    assert!(bot.game.shop_max_buy(slot) >= 2);

    let gold = bot.game.local.gold;
    assert!(act!(s, bot, plan_buy(slot, 2)));
    assert_eq!(count(&bot, item.obj), 2);
    assert_eq!(bot.game.local.gold, gold - buy_price(value, 0, 2) as u32);

    // Sell them back for a third.
    let o = objs.get(item.obj as u32).unwrap();
    if o.newbie == 0 && o.non_transferable == 0 && o.destroys == 0 && !o.no_tradeable_to_npc {
        let gold = bot.game.local.gold;
        let inv = slot_of(&bot, item.obj);
        assert!(act!(s, bot, plan_sell(inv, 99)));
        assert_eq!(count(&bot, item.obj), 0);
        assert_eq!(bot.game.local.gold, gold + sale_price(value, false, 2) as u32);
    }

    // Too poor: refused before sending.
    bot.game.local.gold = 0;
    assert!(!bot.game.shop_buy(slot, 1));
    assert!(bot.console().any(|t| t.contains("oro")));
    bot.send(&C2S::Trade(TradeC2S::CloseShop));
    s.advance(50);
    bot.sync();
    assert!(bot.game.local.social.trade.shop.is_none());
    assert!(!bot.closed());
}

/// Two bots next to each other, already trading.
fn trading_pair(s: &TestServer, a_name: &str, b_name: &str) -> (Bot, Bot) {
    let mut a = s.bot(a_name);
    let mut b = s.bot(b_name);
    give(s, &mut a, OBJ_A, 10);
    give(s, &mut b, GOLD_OBJ, 1000);
    settle(s, &mut [&mut a, &mut b]);
    let (ia, ib) = (a.id(), b.id());
    assert!(a.game.nearby_players().iter().any(|(id, _)| *id == ib), "next to each other: {:?}", a.game.nearby_players());
    assert!(tx!(a, plan_propose(ib)));
    settle(s, &mut [&mut a, &mut b]);
    assert_eq!(b.game.local.social.trade.invited_by.as_ref().map(|i| i.0), Some(ia));
    assert!(tx!(b, plan_accept_invite()));
    settle(s, &mut [&mut a, &mut b]);
    assert!(a.game.local.social.trade.player.is_some() && b.game.local.social.trade.player.is_some());
    (a, b)
}

#[test]
fn players_trade_through_the_client_with_double_confirmation() {
    let s = TestServer::start_at(1, 77, 49);
    let (mut a, mut b) = trading_pair(&s, "Ana", "Beto");
    assert!(!b.game.trade_propose(a.id()), "already trading");

    // Offers; an empty offer is not confirmable until something is on the table.
    assert!(!a.game.trade_confirm());
    let slot = slot_of(&a, OBJ_A);
    assert!(tx!(a, plan_offer(slot, 4)));
    assert!(tx!(b, plan_offer_gold(300)));
    settle(&s, &mut [&mut a, &mut b]);
    let t = a.game.local.social.trade.player.clone().unwrap();
    assert_eq!((t.mine.items.clone(), t.theirs.gold), (vec![(OBJ_A, 4)], 300));
    assert!(!a.game.trade_accept(), "accepting needs both confirmations");

    // First confirmation of both, then the second.
    assert!(tx!(a, plan_confirm()));
    assert!(tx!(b, plan_confirm()));
    settle(&s, &mut [&mut a, &mut b]);
    assert!(a.game.local.social.trade.player.as_ref().is_some_and(|t| t.i_confirmed && t.they_confirmed));
    assert!(!a.game.trade_confirm(), "no second Confirm");
    assert!(tx!(a, plan_accept()));
    assert!(tx!(b, plan_accept()));
    settle(&s, &mut [&mut a, &mut b]);
    assert!(a.game.local.social.trade.player.is_none() && b.game.local.social.trade.player.is_none());
    assert_eq!(count(&a, OBJ_A), 6);
    assert_eq!(count(&b, OBJ_A), 4);
    assert_eq!(a.game.local.gold, 300);
    assert_eq!(b.game.local.gold, 700);
}

#[test]
fn changing_an_offer_clears_the_confirmations_and_cancel_ends_it() {
    let s = TestServer::start_at(1, 77, 49);
    let (mut a, mut b) = trading_pair(&s, "Cris", "Dani");
    let slot = slot_of(&a, OBJ_A);
    assert!(tx!(a, plan_offer(slot, 2)));
    settle(&s, &mut [&mut a, &mut b]);
    assert!(tx!(a, plan_confirm()));
    assert!(tx!(b, plan_confirm()));
    settle(&s, &mut [&mut a, &mut b]);
    assert!(a.game.local.social.trade.player.as_ref().unwrap().i_confirmed);
    // More on the table: everything is open again.
    assert!(tx!(a, plan_offer(slot, 1)));
    settle(&s, &mut [&mut a, &mut b]);
    let t = a.game.local.social.trade.player.clone().unwrap();
    assert!(!t.i_confirmed && !t.they_confirmed, "{t:?}");
    assert_eq!(t.mine.items, vec![(OBJ_A, 3)]);
    // Take it back out, then walk away from the deal.
    assert!(tx!(a, plan_retract(0)));
    settle(&s, &mut [&mut a, &mut b]);
    assert!(a.game.local.social.trade.player.as_ref().unwrap().mine.items.is_empty());
    a.send(&C2S::Trade(TradeC2S::Cancel));
    settle(&s, &mut [&mut a, &mut b]);
    assert!(a.game.local.social.trade.player.is_none() && b.game.local.social.trade.player.is_none());
    assert_eq!(count(&a, OBJ_A), 10, "nothing was lost");
}

#[test]
fn declining_an_invitation_and_clicking_a_player_tile() {
    let s = TestServer::start_at(1, 77, 49);
    let mut a = s.bot("Eli");
    let mut b = s.bot("Fran");
    settle(&s, &mut [&mut a, &mut b]);
    let pos = b.pos();
    // The world hook: a click on the tile of a player finds them and asks.
    let id = a.game.player_at(pos.0, pos.1).expect("Fran is there");
    assert_eq!(id, b.id());
    assert!(a.game.player_at(pos.0 + 30, pos.1).is_none(), "nobody there");
    assert!(tx!(a, plan_propose(id)));
    settle(&s, &mut [&mut a, &mut b]);
    assert!(b.game.local.social.trade.invited_by.is_some());
    assert!(tx!(b, plan_decline()));
    settle(&s, &mut [&mut a, &mut b]);
    assert!(a.game.local.social.trade.player.is_none() && b.game.local.social.trade.player.is_none());
}

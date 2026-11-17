//! What the client knows about trade (filled from `S2C::Trade` only). OWNER: Opus B.
use ao_protocol::TradeS2C;
use std::collections::BTreeMap;

/// A merchant slot: object, stock and unit price for this player.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ShopItem {
    pub obj: u16,
    pub amount: u32,
    pub price: u32,
}

#[derive(Debug, Default, Clone)]
pub struct Shop {
    pub npc: u32,
    /// By merchant slot (1-based).
    pub items: BTreeMap<u8, ShopItem>,
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Offer {
    pub items: Vec<(u16, u32)>,
    pub gold: u32,
}

#[derive(Debug, Default, Clone)]
pub struct PlayerTrade {
    pub with: u32,
    pub name: String,
    pub mine: Offer,
    pub theirs: Offer,
    /// Offer revision: send it back with `Confirm` / `Accept`.
    pub rev: u32,
    pub i_confirmed: bool,
    pub they_confirmed: bool,
    pub i_accepted: bool,
    pub they_accepted: bool,
}

#[derive(Debug, Default)]
pub struct TradeState {
    pub shop: Option<Shop>,
    pub player: Option<PlayerTrade>,
    /// Pending invitation: (entity id, name).
    pub invited_by: Option<(u32, String)>,
    /// Result of the last player trade (`Some(true)` = exchanged).
    pub last_done: Option<bool>,
}

impl TradeState {
    pub fn apply(&mut self, msg: &TradeS2C) {
        match msg {
            TradeS2C::ShopOpen { npc, items } => {
                let items = items.iter().map(|&(slot, obj, amount, price)| (slot, ShopItem { obj, amount, price })).collect();
                self.shop = Some(Shop { npc: *npc, items });
            }
            TradeS2C::ShopSlot { slot, obj, amount, price } => {
                if let Some(shop) = &mut self.shop {
                    if *obj == 0 || *amount == 0 {
                        shop.items.remove(slot);
                    } else {
                        shop.items.insert(*slot, ShopItem { obj: *obj, amount: *amount, price: *price });
                    }
                }
            }
            TradeS2C::ShopClosed => self.shop = None,
            TradeS2C::Invited { from, name } => self.invited_by = Some((*from, name.clone())),
            TradeS2C::Opened { with, name } => {
                self.invited_by = None;
                self.player = Some(PlayerTrade { with: *with, name: name.clone(), ..Default::default() });
            }
            TradeS2C::Offer { mine, items, gold, rev } => {
                if let Some(t) = &mut self.player {
                    let o = Offer { items: items.clone(), gold: *gold };
                    if *mine {
                        t.mine = o;
                    } else {
                        t.theirs = o;
                    }
                    t.rev = *rev;
                }
            }
            TradeS2C::Status { rev, i_confirmed, they_confirmed, i_accepted, they_accepted } => {
                if let Some(t) = &mut self.player {
                    t.rev = *rev;
                    t.i_confirmed = *i_confirmed;
                    t.they_confirmed = *they_confirmed;
                    t.i_accepted = *i_accepted;
                    t.they_accepted = *they_accepted;
                }
            }
            TradeS2C::Closed { done } => {
                self.player = None;
                self.invited_by = None;
                self.last_done = Some(*done);
            }
        }
    }
}

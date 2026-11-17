//! P2 client state, one file per system owner (docs/P2_CONTRATO.md).
pub mod trade;
pub mod guild;
pub mod party;
pub mod faction;
pub mod quest;
pub mod channel;

use ao_protocol::S2C;

#[derive(Debug, Default)]
pub struct Social {
    pub trade: trade::TradeState,
    pub guild: guild::GuildState,
    pub party: party::PartyState,
    pub faction: faction::FactionState,
    pub quest: quest::QuestState,
    pub channel: channel::ChannelState,
}

impl Social {
    /// Applies a P2 message. Returns true if it was one.
    pub fn apply(&mut self, msg: &S2C) -> bool {
        match msg {
            S2C::Trade(m) => self.trade.apply(m),
            S2C::Guild(m) => self.guild.apply(m),
            S2C::Party(m) => self.party.apply(m),
            S2C::Faction(m) => self.faction.apply(m),
            S2C::Quest(m) => self.quest.apply(m),
            S2C::Channel(m) => self.channel.apply(m),
            _ => return false,
        }
        true
    }
}

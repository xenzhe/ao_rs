//! What the client knows about trade (filled from `S2C::Trade` only). OWNER: Opus B.
use ao_protocol::TradeS2C;

#[derive(Debug, Default)]
pub struct TradeState {}

impl TradeState {
    pub fn apply(&mut self, msg: &TradeS2C) {
        match *msg {}
    }
}

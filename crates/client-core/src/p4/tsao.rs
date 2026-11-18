//! TSAO progression state (from `S2C::Tsao`): points, the exchange window, the pending
//! class bonus and the poll; and their actions.
use crate::Game;
use ao_protocol::p4::tsao::{PackView, Points, PrizeView, TsView, TsaoC2S, TsaoS2C};
use ao_protocol::C2S;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Exchange {
    pub prizes: Vec<PrizeView>,
    pub ts: Vec<TsView>,
    pub packs: Vec<PackView>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Poll {
    pub question: String,
    pub options: Vec<String>,
    pub results: Option<Vec<u32>>,
}

#[derive(Debug, Default)]
pub struct TsaoState {
    pub points: Points,
    pub exchange: Option<Exchange>,
    /// `(tier, level, options)` waiting for a choice.
    pub bonus_offer: Option<(u8, u32, [String; 2])>,
    pub bonuses: Vec<String>,
    pub poll: Option<Poll>,
    /// The Octarine gem window: `(index, object you have, what it becomes)`.
    pub upgrades: Option<Vec<(u16, u16, PrizeView)>>,
}

impl TsaoState {
    pub fn apply(&mut self, msg: &TsaoS2C) {
        match msg {
            TsaoS2C::Points(p) => self.points = *p,
            TsaoS2C::Exchange { prizes, ts, packs } => self.exchange = Some(Exchange { prizes: prizes.clone(), ts: ts.clone(), packs: packs.clone() }),
            TsaoS2C::BonusOffer { tier, level, options } => self.bonus_offer = Some((*tier, *level, options.clone())),
            TsaoS2C::Bonuses(b) => self.bonuses = b.clone(),
            TsaoS2C::Poll { question, options, results } => self.poll = Some(Poll { question: question.clone(), options: options.clone(), results: results.clone() }),
            TsaoS2C::CloseExchange => self.exchange = None,
            TsaoS2C::Upgrades(list) => self.upgrades = (!list.is_empty()).then(|| list.clone()),
        }
    }
}

impl Game {
    pub fn tsao_send(&mut self, m: TsaoC2S) {
        self.send(&C2S::Tsao(m));
    }

    pub fn exchange_close(&mut self) {
        self.local.p4.tsao.exchange = None;
    }

    pub fn upgrades_close(&mut self) {
        self.local.p4.tsao.upgrades = None;
    }

    pub fn poll_close(&mut self) {
        self.local.p4.tsao.poll = None;
    }

    /// Answers the pending class bonus.
    pub fn bonus_choose(&mut self, option: u8) {
        if let Some((tier, ..)) = self.local.p4.tsao.bonus_offer.take() {
            self.send(&C2S::Tsao(TsaoC2S::ChooseBonus { tier, option }));
        }
    }

    /// Closes the bonus window without choosing (offered again with /bonus or on entering).
    pub fn bonus_later(&mut self) {
        self.local.p4.tsao.bonus_offer = None;
    }
}

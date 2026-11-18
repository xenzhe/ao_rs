//! Collectible cards (`ModCollectibleCards.bas`): using a card adds it to the account's
//! collection (bronze +1, silver +3, gold +5, at most 10 of each); the cards of a creature
//! give more experience, damage, protection and loot against it. Values of the official
//! Configuracion.ini; `collectible_cards` turns it on (the feature toggle).
use crate::world::World;
use ao_protocol::Tone;

/// (cards needed, multiplier) steps of each bonus (`Card*BonusLvlN` / `Card*BonusN`).
const EXP_STEPS: [(u8, f32); 4] = [(1, 1.1), (5, 1.2), (8, 1.4), (10, 1.8)];
const DAMAGE_STEPS: [(u8, f32); 3] = [(2, 1.2), (6, 1.5), (10, 1.5)];
const REDUCTION_STEPS: [(u8, f32); 3] = [(3, 0.8), (7, 0.5), (10, 0.5)];
const DROP_STEPS: [(u8, f32); 2] = [(4, 1.5), (10, 2.0)];
/// `CardBronceQuantity`, `CardSilverQuantity`, `CardGoldQuantity` by `CollectibleCardRarity`.
const RARITY_QUANTITY: [u8; 3] = [1, 3, 5];
const MAX_CARDS: u8 = 10;

fn step(steps: &[(u8, f32)], have: u8) -> f32 {
    steps.iter().filter(|(need, _)| have >= *need).map(|(_, m)| *m).last().unwrap_or(1.0)
}

/// Which bonus of a creature's cards.
#[derive(Clone, Copy)]
pub enum CardBonus {
    Exp,
    Damage,
    Reduction,
    Drop,
}

impl World {
    fn account_id(&self, id: u32) -> Option<i64> {
        self.sessions.get(&id).and_then(|s| s.account.as_ref()).map(|a| a.id)
    }

    /// Loads the account's collection when a character enters the world.
    pub(crate) fn cards_on_enter(&mut self, id: u32) {
        let Some(acc) = self.account_id(id) else { return };
        if let Ok(list) = self.db.cards_of(acc) {
            self.player_mut(id).cards = list.into_iter().collect();
        }
    }

    /// `AddCollectibleCardToUser` from `UseInvItem`.
    pub(crate) fn use_card(&mut self, id: u32, slot: u8, o: &ao_formats::dat::obj::ObjData) {
        if !self.cfg.world.collectible_cards {
            return self.console(id, "Las cartas coleccionables están desactivadas.", Tone::Info);
        }
        let Ok(card) = u16::try_from(o.card_index).map(|c| c.max(1)) else { return };
        let have = self.player(id).map_or(0, |p| p.cards.get(&card).copied().unwrap_or(0));
        if have >= MAX_CARDS {
            return self.console(id, "Ya tenés la colección completa de esta carta.", Tone::Info);
        }
        let inc = RARITY_QUANTITY.get((o.card_rarity - 1).max(0) as usize).copied().unwrap_or(0);
        if inc == 0 {
            return;
        }
        let new = (have + inc).min(MAX_CARDS);
        self.player_mut(id).cards.insert(card, new);
        if let Some(acc) = self.account_id(id) {
            if let Err(e) = self.db.card_set(acc, card, new) {
                tracing::warn!(?e, "no se pudo guardar la carta");
            }
        }
        self.remove_from_slot(id, slot, 1);
        self.console(id, format!("Agregaste la carta a tu colección ({new}/{MAX_CARDS})."), Tone::Good);
    }

    /// The multiplier `id`'s cards of the creature `npc_def` give (1 = none).
    pub(crate) fn card_bonus(&self, id: u32, npc_def: u16, kind: CardBonus) -> f32 {
        if !self.cfg.world.collectible_cards {
            return 1.0;
        }
        let Some(card) = self.ao.npcs.get(npc_def as u32).map(|d| d.collectible_card_index).filter(|c| *c > 0) else { return 1.0 };
        let have = self.player(id).and_then(|p| p.cards.get(&(card as u16)).copied()).unwrap_or(0);
        match kind {
            CardBonus::Exp => step(&EXP_STEPS, have),
            CardBonus::Damage => step(&DAMAGE_STEPS, have),
            CardBonus::Reduction => step(&REDUCTION_STEPS, have),
            CardBonus::Drop => step(&DROP_STEPS, have),
        }
    }

    /// `/cartas`: the collection.
    pub(crate) fn cards_list(&self, id: u32) {
        let Some(p) = self.player(id) else { return };
        if p.cards.is_empty() {
            return self.console(id, "Tu colección de cartas está vacía.", Tone::Info);
        }
        let mut names: Vec<String> = p
            .cards
            .iter()
            .map(|(card, q)| {
                let npc = self.ao.npcs.iter().find(|(_, d)| d.collectible_card_index == *card as i32).map(|(_, d)| d.name.clone()).unwrap_or_else(|| format!("carta {card}"));
                format!("{npc}: {q}/{MAX_CARDS}")
            })
            .collect();
        names.sort();
        self.console(id, format!("Colección de cartas: {}", names.join(", ")), Tone::Info);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn official_steps() {
        assert_eq!(step(&EXP_STEPS, 0), 1.0);
        assert_eq!(step(&EXP_STEPS, 1), 1.1);
        assert_eq!(step(&EXP_STEPS, 10), 1.8);
        assert_eq!(step(&REDUCTION_STEPS, 7), 0.5);
        assert_eq!(step(&DROP_STEPS, 3), 1.0);
    }
}

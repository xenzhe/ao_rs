-- Collectible cards (ModCollectibleCards.bas): a collection per account, 0..10 of each card.
CREATE TABLE IF NOT EXISTS account_card (
    account_id INTEGER NOT NULL,
    card_index INTEGER NOT NULL,
    quantity   INTEGER NOT NULL,
    PRIMARY KEY (account_id, card_index)
);

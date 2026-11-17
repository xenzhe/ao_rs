-- P4 events (docs/P4_CONTRATO.md, OWNER: Opus D).
-- Open auction lots (ModSubasta.bas t_Subastas): the item and the best bid are held by the
-- server until the lot closes, so they survive a restart.
CREATE TABLE event_auction (
    id         INTEGER PRIMARY KEY,
    seller     TEXT    NOT NULL COLLATE NOCASE,
    obj        INTEGER NOT NULL,
    amount     INTEGER NOT NULL,
    base_price INTEGER NOT NULL,
    best       INTEGER NOT NULL DEFAULT 0,
    bidder     TEXT COLLATE NOCASE,
    secs_left  INTEGER NOT NULL
);
-- Gold and items owed to a character that was offline (outbid refunds, auction payments,
-- returned lots). AO20 wrote them into the offline .chr (bank / MENSAJEINFORMACION);
-- here they wait in this mailbox and are delivered on the next login.
CREATE TABLE event_mailbox (
    id     INTEGER PRIMARY KEY AUTOINCREMENT,
    name   TEXT    NOT NULL COLLATE NOCASE,
    gold   INTEGER NOT NULL DEFAULT 0,
    obj    INTEGER NOT NULL DEFAULT 0,
    amount INTEGER NOT NULL DEFAULT 0,
    note   TEXT    NOT NULL DEFAULT ''
);
CREATE INDEX event_mailbox_name ON event_mailbox(name);

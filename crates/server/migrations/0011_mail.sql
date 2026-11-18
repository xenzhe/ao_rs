-- Player mail (TSAO Correos): letters with an optional parcel (gold + item stacks).
-- `items` is "obj-amount,obj-amount" (at most a few stacks).
CREATE TABLE mail (
    id INTEGER PRIMARY KEY,
    recipient TEXT NOT NULL COLLATE NOCASE,
    sender TEXT NOT NULL,
    subject TEXT NOT NULL,
    body TEXT NOT NULL,
    gold INTEGER NOT NULL DEFAULT 0,
    items TEXT NOT NULL DEFAULT '',
    sent_at INTEGER NOT NULL,
    unread INTEGER NOT NULL DEFAULT 1
);
CREATE INDEX mail_recipient ON mail (recipient, id);

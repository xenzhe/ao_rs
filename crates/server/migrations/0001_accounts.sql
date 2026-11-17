-- Accounts with several characters. Names are unique ignoring case.
CREATE TABLE account (
    id          INTEGER PRIMARY KEY,
    user        TEXT NOT NULL UNIQUE COLLATE NOCASE,
    pass_hash   TEXT NOT NULL,
    created_at  INTEGER NOT NULL,
    last_login  INTEGER,
    banned      INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE character (
    id          INTEGER PRIMARY KEY,
    account_id  INTEGER NOT NULL REFERENCES account(id),
    name        TEXT NOT NULL UNIQUE COLLATE NOCASE,
    race        INTEGER NOT NULL,
    class       INTEGER NOT NULL,
    gender      INTEGER NOT NULL,
    body        INTEGER NOT NULL,
    head        INTEGER NOT NULL,
    level       INTEGER NOT NULL DEFAULT 1,
    exp         INTEGER NOT NULL DEFAULT 0,
    hp          INTEGER NOT NULL,
    max_hp      INTEGER NOT NULL,
    mana        INTEGER NOT NULL,
    max_mana    INTEGER NOT NULL,
    map         INTEGER NOT NULL,
    x           INTEGER NOT NULL,
    y           INTEGER NOT NULL,
    created_at  INTEGER NOT NULL,
    last_played INTEGER,
    -- Soft delete: the row stays for history; the name is freed by renaming it.
    deleted     INTEGER NOT NULL DEFAULT 0
);

CREATE INDEX character_account ON character(account_id, deleted);

-- Phase 6 (events): global quests and the emperor castles.

-- ModGlobalQuests.bas: global_quest_desc + global_quest_user_contribution.
CREATE TABLE global_quest_desc (
    event_id    INTEGER PRIMARY KEY,
    name        TEXT NOT NULL,
    obj_id      INTEGER NOT NULL,
    threshold   INTEGER NOT NULL,
    start_date  INTEGER NOT NULL,
    end_date    INTEGER NOT NULL,
    is_active   INTEGER NOT NULL DEFAULT 0
);
CREATE TABLE global_quest_contribution (
    id          INTEGER PRIMARY KEY,
    event_id    INTEGER NOT NULL,
    char_name   TEXT NOT NULL COLLATE NOCASE,
    at          INTEGER NOT NULL,
    amount      INTEGER NOT NULL
);
CREATE INDEX global_quest_contribution_event ON global_quest_contribution(event_id);

-- ModCastle.bas: castle + castle_coordinates (one row here) + castle_whitelist, seeded
-- with the official ScriptsDB rows (20 castles, triggers 21-40, relics 6362-6381; only
-- castles 1-15 have an interior, maps 758-772).
CREATE TABLE castle (
    id                  INTEGER PRIMARY KEY,
    trigger             INTEGER NOT NULL,
    owner_account_id    INTEGER,
    owner_char_name     TEXT COLLATE NOCASE,
    spawner_obj_id      INTEGER NOT NULL,
    inside_key_obj_id   INTEGER NOT NULL,
    foundation_date     INTEGER,
    is_active           INTEGER NOT NULL DEFAULT 0,
    name                TEXT,
    outside_map         INTEGER,
    outside_x           INTEGER,
    outside_y           INTEGER,
    inside_map          INTEGER,
    inside_x            INTEGER,
    inside_y            INTEGER
);
INSERT INTO castle (id, trigger, spawner_obj_id, inside_key_obj_id) VALUES
 (1,21,6362,6383),(2,22,6363,6384),(3,23,6364,6385),(4,24,6365,6386),(5,25,6366,6387),
 (6,26,6367,6388),(7,27,6368,6389),(8,28,6369,6390),(9,29,6370,6391),(10,30,6371,6392),
 (11,31,6372,6393),(12,32,6373,6394),(13,33,6374,6395),(14,34,6375,6396),(15,35,6376,6397),
 (16,36,6377,6398),(17,37,6378,6399),(18,38,6379,6400),(19,39,6380,6401),(20,40,6381,6402);
UPDATE castle SET inside_map = 757 + id, inside_x = 50, inside_y = 72 WHERE id <= 15;
CREATE TABLE castle_whitelist (
    castle_id       INTEGER NOT NULL,
    character_name  TEXT NOT NULL COLLATE NOCASE,
    UNIQUE (castle_id, character_name)
);

-- P2 factions (t_Facciones, CharacterPersistence.bas SetupUserFactionInfo). One row per
-- character; a missing row means a fresh citizen.
CREATE TABLE faction (
    character_id     INTEGER PRIMARY KEY REFERENCES character(id),
    status           INTEGER NOT NULL DEFAULT 1,
    citizens_killed  INTEGER NOT NULL DEFAULT 0,
    criminals_killed INTEGER NOT NULL DEFAULT 0,
    royal_rank       INTEGER NOT NULL DEFAULT 0,
    chaos_rank       INTEGER NOT NULL DEFAULT 0,
    royal_rewarded   INTEGER NOT NULL DEFAULT 0,
    chaos_rewarded   INTEGER NOT NULL DEFAULT 0,
    enlistments      INTEGER NOT NULL DEFAULT 0,
    entry_level      INTEGER NOT NULL DEFAULT 0,
    entry_kills      INTEGER NOT NULL DEFAULT 0,
    score            INTEGER NOT NULL DEFAULT 0
);

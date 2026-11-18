-- Phase 5 (GM and moderation).

-- BanTemporal (Admin.bas, baneos.dat): when a character ban ends, unix seconds.
-- NULL = permanent (BanPJ).
ALTER TABLE gm_ban ADD COLUMN until INTEGER;

-- The help queue (`Ayuda`, ModCola.cls): QuestionGM (/GM) and Denounce (/DENUNCIAR).
-- The VB kept it in memory only; here it survives a restart and remembers who attended.
CREATE TABLE gm_sos (
    id          INTEGER PRIMARY KEY,
    name        TEXT NOT NULL COLLATE NOCASE,
    kind        TEXT NOT NULL,
    text        TEXT NOT NULL,
    at          INTEGER NOT NULL,
    done_by     TEXT,
    done_at     INTEGER
);
CREATE INDEX gm_sos_open ON gm_sos(done_by, id);

-- Server settings GMs change in game (Motd.ini, ChatGlobal).
CREATE TABLE gm_setting (
    key         TEXT PRIMARY KEY,
    value       TEXT NOT NULL
);

-- P4 GM (Opus C, docs/P4_CONTRATO.md). Keyed by character name like AO20 does
-- (Server.ini [Admines]/[Dioses]/[SemiDioses]/[Consejeros] list character names).

-- e_PlayerType as a level: 0 user, 1 consejero, 2 semidios, 3 dios, 4 admin.
-- `char_id` binds the rank to that exact character: a deleted character frees its name,
-- and whoever creates it again must not inherit the rank. NULL = development login only.
CREATE TABLE gm_role (
    name        TEXT PRIMARY KEY COLLATE NOCASE,
    char_id     INTEGER,
    rank        INTEGER NOT NULL,
    granted_by  TEXT NOT NULL,
    granted_at  INTEGER NOT NULL
);

-- Penalties still running (Counters.Pena, flags.MinutosRestantes), in milliseconds of
-- online time, so they survive reconnecting.
CREATE TABLE gm_sentence (
    name        TEXT PRIMARY KEY COLLATE NOCASE,
    jail_ms     INTEGER NOT NULL DEFAULT 0,
    silence_ms  INTEGER NOT NULL DEFAULT 0
);

-- Character bans (BanPJ / flag Ban); account bans use account.banned.
CREATE TABLE gm_ban (
    name        TEXT PRIMARY KEY COLLATE NOCASE,
    reason      TEXT NOT NULL,
    gm          TEXT NOT NULL,
    at          INTEGER NOT NULL
);

-- BanIPs.
CREATE TABLE gm_ip_ban (
    ip          TEXT PRIMARY KEY,
    reason      TEXT NOT NULL,
    gm          TEXT NOT NULL,
    at          INTEGER NOT NULL
);

-- SavePenaDatabase: the penalty history of a character (/PENAS).
CREATE TABLE gm_penalty (
    id          INTEGER PRIMARY KEY,
    target      TEXT NOT NULL COLLATE NOCASE,
    gm          TEXT NOT NULL,
    kind        TEXT NOT NULL,
    minutes     INTEGER NOT NULL DEFAULT 0,
    reason      TEXT NOT NULL,
    at          INTEGER NOT NULL
);
CREATE INDEX gm_penalty_target ON gm_penalty(target, at);

-- LogGM: every GM action (who, what, to whom, when).
CREATE TABLE gm_log (
    id          INTEGER PRIMARY KEY,
    at          INTEGER NOT NULL,
    gm          TEXT NOT NULL,
    action      TEXT NOT NULL,
    target      TEXT NOT NULL,
    detail      TEXT NOT NULL
);
CREATE INDEX gm_log_gm ON gm_log(gm, at);

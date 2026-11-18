-- TSAO progression: tournament points, TS points and donation credits (Stats.PuntosTorneo,
-- TSPoints, PuntosDonacion), the level milestones already paid (flags llegolvl50 /
-- Llegolvlmax), the class bonus choices (Bon1..Bon3), marriages (flags.Pareja) and the
-- votes of the popular polls (one per account, like TSAO's one per e-mail).
CREATE TABLE character_points (
    character_id INTEGER PRIMARY KEY REFERENCES character (id) ON DELETE CASCADE,
    tournament INTEGER NOT NULL DEFAULT 0,
    ts INTEGER NOT NULL DEFAULT 0,
    credits INTEGER NOT NULL DEFAULT 0,
    -- Levels whose points were paid, "50,60".
    levels_paid TEXT NOT NULL DEFAULT ''
);
CREATE TABLE class_bonus (
    character_id INTEGER NOT NULL REFERENCES character (id) ON DELETE CASCADE,
    tier INTEGER NOT NULL,
    option INTEGER NOT NULL,
    PRIMARY KEY (character_id, tier)
);
CREATE TABLE marriage (
    character_id INTEGER PRIMARY KEY REFERENCES character (id) ON DELETE CASCADE,
    spouse_id INTEGER NOT NULL UNIQUE REFERENCES character (id) ON DELETE CASCADE
);
CREATE TABLE poll_vote (
    poll TEXT NOT NULL,
    account_id INTEGER NOT NULL REFERENCES account (id) ON DELETE CASCADE,
    option INTEGER NOT NULL,
    PRIMARY KEY (poll, account_id)
);

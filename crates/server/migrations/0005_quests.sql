-- P2 quests: quests in progress (with the kill / visit counters, comma separated, one
-- number per goal of the quest) and the ones already completed.
CREATE TABLE quest_active (
    character_id INTEGER NOT NULL REFERENCES character(id),
    slot         INTEGER NOT NULL,
    quest        INTEGER NOT NULL,
    killed       TEXT NOT NULL DEFAULT '',
    targeted     TEXT NOT NULL DEFAULT '',
    PRIMARY KEY (character_id, slot)
);

CREATE TABLE quest_done (
    character_id INTEGER NOT NULL REFERENCES character(id),
    quest        INTEGER NOT NULL,
    PRIMARY KEY (character_id, quest)
);

-- Full character state for P1: attributes, skills, vitals, gold, inventory, vault, spells.
ALTER TABLE character ADD COLUMN attributes TEXT NOT NULL DEFAULT '18,18,18,18,18';
ALTER TABLE character ADD COLUMN skills TEXT NOT NULL DEFAULT '';
ALTER TABLE character ADD COLUMN skill_points INTEGER NOT NULL DEFAULT 0;
ALTER TABLE character ADD COLUMN sta INTEGER NOT NULL DEFAULT 40;
ALTER TABLE character ADD COLUMN max_sta INTEGER NOT NULL DEFAULT 40;
ALTER TABLE character ADD COLUMN food INTEGER NOT NULL DEFAULT 100;
ALTER TABLE character ADD COLUMN water INTEGER NOT NULL DEFAULT 100;
ALTER TABLE character ADD COLUMN gold INTEGER NOT NULL DEFAULT 0;
ALTER TABLE character ADD COLUMN bank_gold INTEGER NOT NULL DEFAULT 0;

CREATE TABLE inventory_item (
    character_id INTEGER NOT NULL REFERENCES character(id),
    slot         INTEGER NOT NULL,
    obj          INTEGER NOT NULL,
    amount       INTEGER NOT NULL,
    equipped     INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (character_id, slot)
);

CREATE TABLE bank_item (
    character_id INTEGER NOT NULL REFERENCES character(id),
    slot         INTEGER NOT NULL,
    obj          INTEGER NOT NULL,
    amount       INTEGER NOT NULL,
    PRIMARY KEY (character_id, slot)
);

CREATE TABLE spell (
    character_id INTEGER NOT NULL REFERENCES character(id),
    slot         INTEGER NOT NULL,
    spell        INTEGER NOT NULL,
    PRIMARY KEY (character_id, slot)
);

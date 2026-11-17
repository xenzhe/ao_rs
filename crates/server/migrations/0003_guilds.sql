-- P2 guilds (clsClan / modGuilds.bas: guilds, guild_members, guild_request).
CREATE TABLE guild (
    id          INTEGER PRIMARY KEY,
    name        TEXT NOT NULL UNIQUE COLLATE NOCASE,
    alignment   INTEGER NOT NULL,
    founder_id  INTEGER NOT NULL REFERENCES character(id),
    leader_id   INTEGER NOT NULL REFERENCES character(id),
    description TEXT NOT NULL DEFAULT '',
    news        TEXT NOT NULL DEFAULT '',
    level       INTEGER NOT NULL DEFAULT 1,
    exp         INTEGER NOT NULL DEFAULT 0,
    created_at  INTEGER NOT NULL
);

-- One guild per character.
CREATE TABLE guild_member (
    character_id INTEGER PRIMARY KEY REFERENCES character(id) ON DELETE CASCADE,
    guild_id     INTEGER NOT NULL REFERENCES guild(id) ON DELETE CASCADE
);

-- One pending application per character (a_NuevoAspirante drops the old one).
CREATE TABLE guild_request (
    character_id INTEGER PRIMARY KEY REFERENCES character(id) ON DELETE CASCADE,
    guild_id     INTEGER NOT NULL REFERENCES guild(id) ON DELETE CASCADE,
    application  TEXT NOT NULL DEFAULT '',
    created_at   INTEGER NOT NULL
);

-- Rejection reason for an applicant that was offline (SaveUserGuildRejectionReason).
CREATE TABLE guild_rejection (
    character_id INTEGER PRIMARY KEY REFERENCES character(id) ON DELETE CASCADE,
    text         TEXT NOT NULL
);

-- Relation between two guilds, stored once with guild_a < guild_b (-1 war, 1 alliance).
CREATE TABLE guild_relation (
    guild_a  INTEGER NOT NULL REFERENCES guild(id) ON DELETE CASCADE,
    guild_b  INTEGER NOT NULL REFERENCES guild(id) ON DELETE CASCADE,
    relation INTEGER NOT NULL,
    PRIMARY KEY (guild_a, guild_b)
);

-- Pending peace / alliance proposals.
CREATE TABLE guild_proposal (
    from_guild INTEGER NOT NULL REFERENCES guild(id) ON DELETE CASCADE,
    to_guild   INTEGER NOT NULL REFERENCES guild(id) ON DELETE CASCADE,
    relation   INTEGER NOT NULL,
    text       TEXT NOT NULL DEFAULT '',
    PRIMARY KEY (from_guild, to_guild)
);

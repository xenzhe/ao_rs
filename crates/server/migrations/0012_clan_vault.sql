-- Clan vault (TSAO BoveClan): items and gold shared by the members of a guild.
CREATE TABLE guild_vault (
    guild_id INTEGER NOT NULL REFERENCES guild (id) ON DELETE CASCADE,
    slot INTEGER NOT NULL,
    obj INTEGER NOT NULL,
    amount INTEGER NOT NULL,
    PRIMARY KEY (guild_id, slot)
);
CREATE TABLE guild_vault_gold (
    guild_id INTEGER PRIMARY KEY REFERENCES guild (id) ON DELETE CASCADE,
    gold INTEGER NOT NULL
);

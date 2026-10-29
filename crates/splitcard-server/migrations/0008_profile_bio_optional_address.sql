-- no-transaction
-- New members sign up with a name only and add their Lightning Address
-- when they first create or join a team, so the address becomes optional.
-- Profiles also gain an optional bio.
--
-- SQLite cannot drop a NOT NULL constraint, so profiles is rebuilt with
-- SQLite's documented procedure: foreign keys off (only possible outside a
-- transaction, hence no-transaction), the rebuild in one transaction, then
-- foreign keys back on. Every row is copied with its key, so nothing that
-- references a profile is left dangling. participants and prizes reference
-- profiles by name, so they point at the rebuilt table.

PRAGMA foreign_keys = OFF;

BEGIN;

CREATE TABLE profiles_rebuilt (
    pubkey TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    -- Required before creating or joining a team; never cleared once set.
    lightning_address TEXT,
    bio TEXT,
    updated_at INTEGER NOT NULL
);

INSERT INTO profiles_rebuilt (pubkey, name, lightning_address, bio, updated_at)
SELECT pubkey, name, lightning_address, NULL, updated_at FROM profiles;

DROP TABLE profiles;
ALTER TABLE profiles_rebuilt RENAME TO profiles;

COMMIT;

PRAGMA foreign_keys = ON;

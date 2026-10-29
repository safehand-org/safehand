-- Organizers go away. Teams stand alone, carrying the free-text event label
-- their cards name, and members keep their name and Lightning Address in a
-- profile instead of per team. The organizer payout tables are dropped.
--
-- Each team's new slug is derived in Rust from its name before this runs
-- (see `db::migrate`), and handed over in `temp.migration_team_slugs`.
--
-- SQLite cannot drop columns that carry constraints, so teams and
-- participants are rebuilt. Dropping `teams` orphans the rows of `cards` that
-- reference it until the rebuilt table is filled, so foreign keys are
-- deferred to the end of this migration's transaction, then checked.

PRAGMA defer_foreign_keys = ON;

CREATE TABLE profiles (
    pubkey TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    lightning_address TEXT NOT NULL,
    updated_at INTEGER NOT NULL
);

-- One profile per participant, from the details they joined with. Someone on
-- several teams keeps the details of their latest join.
INSERT INTO profiles (pubkey, name, lightning_address, updated_at)
SELECT p.pubkey, p.name, p.lightning_address, p.joined_at
FROM participants p
WHERE NOT EXISTS (
    SELECT 1 FROM participants later
    WHERE later.pubkey = p.pubkey
      AND (later.joined_at > p.joined_at
           OR (later.joined_at = p.joined_at AND later.rowid > p.rowid))
);

-- Cards were checked against the event slug, so it becomes the label and
-- existing cards stay valid. A team without a slug or event fails the
-- NOT NULL constraints below rather than being dropped.
CREATE TEMP TABLE migration_teams AS
SELECT t.id, s.slug, t.name, e.slug AS event_label, t.join_code, t.submission_url,
       t.active_card_hash, t.created_at
FROM teams t
LEFT JOIN events e ON e.id = t.event_id
LEFT JOIN migration_team_slugs s ON s.team_id = t.id;

-- Rowids are kept: participants are listed in join order, then rowid.
CREATE TEMP TABLE migration_participants AS
SELECT rowid AS old_rowid, team_id, pubkey, joined_at FROM participants;

DROP TABLE payout_items;
DROP TABLE payouts;
DROP TABLE participants;
DROP TABLE teams;
DROP TABLE events;

CREATE TABLE teams (
    id INTEGER PRIMARY KEY,
    -- Unique across the server: it becomes the team's Lightning Address
    -- username.
    slug TEXT NOT NULL UNIQUE,
    -- Cards name it in their `team` field.
    name TEXT NOT NULL,
    -- Free text, such as the hackathon's name; cards name it in their
    -- `event` field.
    event_label TEXT NOT NULL,
    join_code TEXT NOT NULL UNIQUE,
    submission_url TEXT,
    active_card_hash TEXT REFERENCES cards (hash),
    created_at INTEGER NOT NULL
);

INSERT INTO teams (id, slug, name, event_label, join_code, submission_url,
                   active_card_hash, created_at)
SELECT id, slug, name, event_label, join_code, submission_url, active_card_hash, created_at
FROM migration_teams;

-- Someone who joined a team. Their name and Lightning Address are their
-- profile's.
CREATE TABLE participants (
    team_id INTEGER NOT NULL REFERENCES teams (id),
    pubkey TEXT NOT NULL REFERENCES profiles (pubkey),
    joined_at INTEGER NOT NULL,
    PRIMARY KEY (team_id, pubkey)
);

CREATE INDEX participants_pubkey ON participants (pubkey);

INSERT INTO participants (rowid, team_id, pubkey, joined_at)
SELECT old_rowid, team_id, pubkey, joined_at FROM migration_participants;

DROP TABLE migration_teams;
DROP TABLE migration_participants;

-- Fail the migration, rolling it back, if any reference is left dangling.
CREATE TEMP TABLE migration_foreign_key_check (
    violations INTEGER NOT NULL CHECK (violations = 0)
);
INSERT INTO migration_foreign_key_check SELECT count(*) FROM pragma_foreign_key_check;
DROP TABLE migration_foreign_key_check;

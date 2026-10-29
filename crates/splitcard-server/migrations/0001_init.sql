-- Split Card server schema. All timestamps are Unix seconds.

CREATE TABLE events (
    id INTEGER PRIMARY KEY,
    slug TEXT NOT NULL UNIQUE,
    name TEXT NOT NULL,
    created_by TEXT NOT NULL,
    created_at INTEGER NOT NULL
);

CREATE TABLE teams (
    id INTEGER PRIMARY KEY,
    event_id INTEGER NOT NULL REFERENCES events (id),
    name TEXT NOT NULL,
    join_code TEXT NOT NULL UNIQUE,
    submission_url TEXT,
    active_card_hash TEXT REFERENCES cards (hash),
    created_at INTEGER NOT NULL,
    -- A card names its team, so names must be unique within an event.
    UNIQUE (event_id, name)
);

-- Someone who joined a team. A card's member list is the agreed team.
CREATE TABLE participants (
    team_id INTEGER NOT NULL REFERENCES teams (id),
    pubkey TEXT NOT NULL,
    name TEXT NOT NULL,
    lightning_address TEXT NOT NULL,
    joined_at INTEGER NOT NULL,
    PRIMARY KEY (team_id, pubkey)
);

CREATE TABLE cards (
    hash TEXT PRIMARY KEY,
    team_id INTEGER NOT NULL REFERENCES teams (id),
    canonical_json TEXT NOT NULL,
    supersedes TEXT REFERENCES cards (hash),
    status TEXT NOT NULL CHECK (status IN ('draft', 'active', 'superseded')),
    created_by TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    activated_at INTEGER
);

CREATE INDEX cards_team ON cards (team_id);

CREATE TABLE card_signatures (
    card_hash TEXT NOT NULL REFERENCES cards (hash),
    pubkey TEXT NOT NULL,
    -- The signature event exactly as received.
    event_json TEXT NOT NULL,
    received_at INTEGER NOT NULL,
    PRIMARY KEY (card_hash, pubkey)
);

CREATE TABLE relay_outbox (
    id INTEGER PRIMARY KEY,
    event_json TEXT NOT NULL,
    status TEXT NOT NULL CHECK (status IN ('pending', 'sent', 'failed')),
    attempts INTEGER NOT NULL DEFAULT 0,
    last_error TEXT,
    next_attempt_at INTEGER NOT NULL
);

CREATE INDEX relay_outbox_due ON relay_outbox (status, next_attempt_at);

-- NIP-98 event ids already accepted, for replay protection.
CREATE TABLE used_auth_events (
    event_id TEXT PRIMARY KEY,
    seen_at INTEGER NOT NULL
);

CREATE INDEX used_auth_events_seen ON used_auth_events (seen_at);

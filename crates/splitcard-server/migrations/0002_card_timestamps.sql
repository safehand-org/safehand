-- OpenTimestamps proofs for activated cards. All timestamps are Unix seconds.
-- `complete` means a calendar supplied a Bitcoin block attestation; the
-- server does not check block headers itself.
CREATE TABLE card_timestamps (
    card_hash TEXT PRIMARY KEY REFERENCES cards (hash),
    -- Detached .ots file for the canonical card JSON; NULL until a calendar accepts.
    ots_proof BLOB,
    status TEXT NOT NULL CHECK (status IN ('queued', 'pending', 'complete', 'failed')),
    calendars_accepted INTEGER NOT NULL DEFAULT 0,
    bitcoin_block_height INTEGER,
    -- Submit attempts; upgrades are not counted.
    attempts INTEGER NOT NULL DEFAULT 0,
    last_error TEXT,
    -- When the next submit attempt is due.
    next_attempt_at INTEGER NOT NULL,
    -- Newline-separated calendars to resubmit to after a partial success.
    retry_calendars TEXT,
    submitted_at INTEGER,
    completed_at INTEGER
);

CREATE INDEX card_timestamps_due ON card_timestamps (status, next_attempt_at);

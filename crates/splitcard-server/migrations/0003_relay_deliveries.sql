-- Per-relay delivery of relay_outbox entries. An entry is 'sent' (published)
-- once at least one relay accepts it; relays that have not accepted keep
-- retrying on their own schedule. It is 'failed' only if no relay accepted
-- after all attempts.

CREATE TABLE relay_deliveries (
    outbox_id INTEGER NOT NULL REFERENCES relay_outbox (id),
    relay TEXT NOT NULL,
    status TEXT NOT NULL CHECK (status IN ('pending', 'accepted', 'failed')),
    attempts INTEGER NOT NULL DEFAULT 0,
    last_error TEXT,
    next_attempt_at INTEGER NOT NULL,
    PRIMARY KEY (outbox_id, relay)
);

CREATE INDEX relay_deliveries_due ON relay_deliveries (status, next_attempt_at);

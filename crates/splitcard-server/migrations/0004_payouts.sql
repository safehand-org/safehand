-- Prize payouts. The server never pays: the organizer's browser pays each
-- item over NWC, and the server records a payment only once its preimage
-- verifies. Amounts are sats, timestamps Unix seconds.

CREATE TABLE payouts (
    id INTEGER PRIMARY KEY,
    -- One payout per team: the main guard against paying a team twice.
    team_id INTEGER NOT NULL UNIQUE REFERENCES teams (id),
    card_hash TEXT NOT NULL REFERENCES cards (hash),
    prize_sats INTEGER NOT NULL CHECK (prize_sats > 0),
    status TEXT NOT NULL
        CHECK (status IN ('blocked', 'ready', 'paying', 'partial', 'complete')),
    created_by TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    preflight_at INTEGER NOT NULL
);

CREATE TABLE payout_items (
    payout_id INTEGER NOT NULL REFERENCES payouts (id),
    member_index INTEGER NOT NULL,
    pubkey TEXT NOT NULL,
    -- The member's name on the card, for receipts and error messages.
    name TEXT NOT NULL,
    lightning_address TEXT NOT NULL,
    amount_sats INTEGER NOT NULL CHECK (amount_sats > 0),
    status TEXT NOT NULL
        CHECK (status IN ('pending', 'invoiced', 'paying', 'paid', 'failed', 'unknown')),
    bolt11 TEXT,
    payment_hash TEXT,
    invoice_expires_at INTEGER,
    verify_url TEXT,
    preimage TEXT,
    -- The latest problem as { code, message, details } JSON.
    error TEXT,
    -- When the browser said it was sending the payment, for the 5-minute
    -- timeout to unknown.
    attempted_at INTEGER,
    paid_at INTEGER,
    PRIMARY KEY (payout_id, member_index)
);

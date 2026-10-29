-- Prizes and payouts for standalone teams. Every member of the active card
-- agrees the prize amount; the payer's browser pays each member and the
-- server records a payment only once its preimage verifies. The server never
-- pays. Amounts are sats, timestamps Unix seconds.

-- One prize per team, for the card that was active when it was proposed.
-- `agreed_at` is set once every card member confirmed `prize_sats`; until
-- then a member may propose a different amount, which starts over.
CREATE TABLE prizes (
    team_id INTEGER PRIMARY KEY REFERENCES teams (id),
    card_hash TEXT NOT NULL REFERENCES cards (hash),
    prize_sats INTEGER NOT NULL CHECK (prize_sats > 0),
    proposed_by TEXT NOT NULL REFERENCES profiles (pubkey),
    proposed_at INTEGER NOT NULL,
    agreed_at INTEGER
);

-- Card members who agreed the current proposal.
CREATE TABLE prize_confirmations (
    team_id INTEGER NOT NULL REFERENCES prizes (team_id),
    pubkey TEXT NOT NULL,
    confirmed_at INTEGER NOT NULL,
    PRIMARY KEY (team_id, pubkey)
);

-- Each card member's share of the prize, from `compute_payouts`, in card
-- order. A paid item keeps the invoice and preimage that prove it.
CREATE TABLE payout_items (
    team_id INTEGER NOT NULL REFERENCES prizes (team_id),
    member_index INTEGER NOT NULL,
    pubkey TEXT NOT NULL,
    -- The member's name and Lightning Address on the card.
    name TEXT NOT NULL,
    lightning_address TEXT NOT NULL,
    amount_sats INTEGER NOT NULL CHECK (amount_sats > 0),
    status TEXT NOT NULL CHECK (status IN ('pending', 'paid')),
    bolt11 TEXT,
    -- One payment can never count for two items.
    payment_hash TEXT UNIQUE,
    preimage TEXT,
    paid_by TEXT,
    paid_at INTEGER,
    PRIMARY KEY (team_id, member_index),
    CHECK (
        (status = 'pending' AND bolt11 IS NULL AND payment_hash IS NULL
            AND preimage IS NULL AND paid_by IS NULL AND paid_at IS NULL)
        OR (status = 'paid' AND bolt11 IS NOT NULL AND payment_hash IS NOT NULL
            AND preimage IS NOT NULL AND paid_by IS NOT NULL AND paid_at IS NOT NULL)
    )
);

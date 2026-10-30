-- Version 2 cards list members by public key, who need no account to be
-- listed. Every card's members are indexed so the server can find the cards
-- naming a given key, and members can decline a draft.

CREATE TABLE card_members (
    card_hash TEXT NOT NULL REFERENCES cards (hash),
    member_index INTEGER NOT NULL,
    pubkey TEXT NOT NULL,
    -- The card's name for the member: a version 1 `name`, or a version 2
    -- `label`, which is optional.
    label TEXT,
    weight INTEGER NOT NULL CHECK (weight > 0),
    PRIMARY KEY (card_hash, pubkey)
);
CREATE INDEX card_members_pubkey ON card_members (pubkey);

-- Cards stored before this migration are all version 1.
INSERT INTO card_members (card_hash, member_index, pubkey, label, weight)
SELECT cards.hash, members.key, json_extract(members.value, '$.pubkey'),
       json_extract(members.value, '$.name'), json_extract(members.value, '$.weight')
FROM cards, json_each(cards.canonical_json, '$.members') AS members;

-- The payout address from a version 2 signature's `lud16` tag, as verified
-- by splitcard-core; NULL for version 1, whose addresses are in the card.
ALTER TABLE card_signatures ADD COLUMN lightning_address TEXT;

-- A member who will not sign a draft. A declined draft can never activate.
CREATE TABLE card_declines (
    card_hash TEXT NOT NULL REFERENCES cards (hash),
    pubkey TEXT NOT NULL,
    declined_at INTEGER NOT NULL,
    PRIMARY KEY (card_hash, pubkey)
);

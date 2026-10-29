PRAGMA foreign_keys=OFF;
BEGIN TRANSACTION;
CREATE TABLE _sqlx_migrations (
    version BIGINT PRIMARY KEY,
    description TEXT NOT NULL,
    installed_on TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
    success BOOLEAN NOT NULL,
    checksum BLOB NOT NULL,
    execution_time BIGINT NOT NULL
);
INSERT INTO _sqlx_migrations VALUES(1,'init','2026-10-27 00:35:19',1,x'34b9b9951978050511140a086bf61f7745238c10d3e9a744eb7a69b6937672d2d62f0949e9017bda468b27b87e1675f8',3329500);
INSERT INTO _sqlx_migrations VALUES(2,'card timestamps','2026-10-27 00:35:19',1,x'4c03733d0554e475e76cd3b834f835f50bf16f197e4bc37121f9e4747e788ce68c3b49e17f93b9e8fc5ffda7c008e8df',1105417);
INSERT INTO _sqlx_migrations VALUES(3,'relay deliveries','2026-10-27 00:35:19',1,x'fea3d16f966d413aefa9afaddc942eb040f150ba1781fd4f89d1de63de197d937d35257ec1b5619a59fd93c9ea245f21',1245542);
INSERT INTO _sqlx_migrations VALUES(4,'payouts','2026-10-27 00:35:19',1,x'245a6535a8ed3cec62aa4b2530b21ef450518f6587f00642c1e91e3346c93c3b7f7e04b2a0757635a0731d90cdba39f7',1462125);
INSERT INTO _sqlx_migrations VALUES(5,'card event signer','2026-10-27 00:35:19',1,x'12fc16eac2d44cc0071ffcb14b5318bbb1465a4e9201b9f3bfeed85d1513810da81b940cc8b28a34e379acf979105b44',581292);
CREATE TABLE events (
    id INTEGER PRIMARY KEY,
    slug TEXT NOT NULL UNIQUE,
    name TEXT NOT NULL,
    created_by TEXT NOT NULL,
    created_at INTEGER NOT NULL
);
INSERT INTO events VALUES(1,'nostr-hack-2026','Nostr Hack','56b328b30c8bf5839e24058747879408bdb36241dc9c2e7c619faa12b2920967',1780000000);
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
INSERT INTO teams VALUES(1,1,'Lightning Llamas','b9e70b1ac75492a65cb470a819653ec2',NULL,'f59e526ba0df549ea89caa3a5a05ce9d163ce594e4f5ca1056b4827eafa4cfff',1780000000);
CREATE TABLE participants (
    team_id INTEGER NOT NULL REFERENCES teams (id),
    pubkey TEXT NOT NULL,
    name TEXT NOT NULL,
    lightning_address TEXT NOT NULL,
    joined_at INTEGER NOT NULL,
    PRIMARY KEY (team_id, pubkey)
);
INSERT INTO participants VALUES(1,'1b84c5567b126440995d3ed5aaba0565d71e1834604819ff9c17f5e9d5dd078f','Alice','alice@blink.sv',1780000000);
INSERT INTO participants VALUES(1,'4d4b6cd1361032ca9bd2aeb9d900aa4d45d9ead80ac9423374c451a7254d0766','Bob','bob@getalby.com',1780000000);
INSERT INTO participants VALUES(1,'531fe6068134503d2723133227c867ac8fa6c83c537e9a44c3c5bdbdcb1fe337','Chidi','chidi@walletofsatoshi.com',1780000000);
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
INSERT INTO cards VALUES('f59e526ba0df549ea89caa3a5a05ce9d163ce594e4f5ca1056b4827eafa4cfff',1,'{"card_id":"llamas-split","event":"nostr-hack-2026","members":[{"name":"Alice","payout":{"lightning":{"address":"alice@blink.sv"}},"pubkey":"1b84c5567b126440995d3ed5aaba0565d71e1834604819ff9c17f5e9d5dd078f","weight":36},{"name":"Bob","payout":{"lightning":{"address":"bob@getalby.com"}},"pubkey":"4d4b6cd1361032ca9bd2aeb9d900aa4d45d9ead80ac9423374c451a7254d0766","weight":35},{"name":"Chidi","payout":{"lightning":{"address":"chidi@walletofsatoshi.com"}},"pubkey":"531fe6068134503d2723133227c867ac8fa6c83c537e9a44c3c5bdbdcb1fe337","weight":29}],"rules":{"peer_review":null},"supersedes":null,"team":"Lightning Llamas","version":1}',NULL,'active','1b84c5567b126440995d3ed5aaba0565d71e1834604819ff9c17f5e9d5dd078f',1780000000,1780000000);
CREATE TABLE card_signatures (
    card_hash TEXT NOT NULL REFERENCES cards (hash),
    pubkey TEXT NOT NULL,
    -- The signature event exactly as received.
    event_json TEXT NOT NULL,
    received_at INTEGER NOT NULL,
    PRIMARY KEY (card_hash, pubkey)
);
INSERT INTO card_signatures VALUES('f59e526ba0df549ea89caa3a5a05ce9d163ce594e4f5ca1056b4827eafa4cfff','1b84c5567b126440995d3ed5aaba0565d71e1834604819ff9c17f5e9d5dd078f','{"id":"ed6fe90ab875e5302b3a503b8ca3734002f16ad4102562c2b0c158ce973b7e15","pubkey":"1b84c5567b126440995d3ed5aaba0565d71e1834604819ff9c17f5e9d5dd078f","created_at":1780000000,"kind":30078,"tags":[["d","f59e526ba0df549ea89caa3a5a05ce9d163ce594e4f5ca1056b4827eafa4cfff"],["splitcard","f59e526ba0df549ea89caa3a5a05ce9d163ce594e4f5ca1056b4827eafa4cfff"]],"content":"","sig":"109ccf364b2bf6e6661384c443ea44a6fc55eda3a992d9fe7ac742e87dc115b811cb9b4cb4382b86af370f62b157778d4ecab8a80e207d4794449c96b79d610d"}',1780000000);
INSERT INTO card_signatures VALUES('f59e526ba0df549ea89caa3a5a05ce9d163ce594e4f5ca1056b4827eafa4cfff','4d4b6cd1361032ca9bd2aeb9d900aa4d45d9ead80ac9423374c451a7254d0766','{"id":"5d71c46df2e68bf7231a4fe0aed9a6922defc06a35efe6cd1001c3e1a4361c7a","pubkey":"4d4b6cd1361032ca9bd2aeb9d900aa4d45d9ead80ac9423374c451a7254d0766","created_at":1780000000,"kind":30078,"tags":[["d","f59e526ba0df549ea89caa3a5a05ce9d163ce594e4f5ca1056b4827eafa4cfff"],["splitcard","f59e526ba0df549ea89caa3a5a05ce9d163ce594e4f5ca1056b4827eafa4cfff"]],"content":"","sig":"4fe18b578f39a90e09acf50b1988bf0be38b9907b4da72e8ec9a9812ed0fd4aeb6c686af7ed71b7983d50de27388a229c99d95a9d8ca86396673316c455de71a"}',1780000000);
INSERT INTO card_signatures VALUES('f59e526ba0df549ea89caa3a5a05ce9d163ce594e4f5ca1056b4827eafa4cfff','531fe6068134503d2723133227c867ac8fa6c83c537e9a44c3c5bdbdcb1fe337','{"id":"d6cfb099b9746d5ca5ba8645fe26299c97e3bac96a8f460cb081ad235f8f0427","pubkey":"531fe6068134503d2723133227c867ac8fa6c83c537e9a44c3c5bdbdcb1fe337","created_at":1780000000,"kind":30078,"tags":[["d","f59e526ba0df549ea89caa3a5a05ce9d163ce594e4f5ca1056b4827eafa4cfff"],["splitcard","f59e526ba0df549ea89caa3a5a05ce9d163ce594e4f5ca1056b4827eafa4cfff"]],"content":"","sig":"a862a7c19ce0895c3d4ba537bf92b4bcf0d8318396f0410f1897b4b0977fc60c7ee8d9e0beabc5793ffd8e50fd4564642196548610073d27d36a2ff38bd07384"}',1780000000);
CREATE TABLE relay_outbox (
    id INTEGER PRIMARY KEY,
    event_json TEXT NOT NULL,
    status TEXT NOT NULL CHECK (status IN ('pending', 'sent', 'failed')),
    attempts INTEGER NOT NULL DEFAULT 0,
    last_error TEXT,
    next_attempt_at INTEGER NOT NULL
, server_pubkey TEXT);
INSERT INTO relay_outbox VALUES(1,'{"id":"ed6fe90ab875e5302b3a503b8ca3734002f16ad4102562c2b0c158ce973b7e15","pubkey":"1b84c5567b126440995d3ed5aaba0565d71e1834604819ff9c17f5e9d5dd078f","created_at":1780000000,"kind":30078,"tags":[["d","f59e526ba0df549ea89caa3a5a05ce9d163ce594e4f5ca1056b4827eafa4cfff"],["splitcard","f59e526ba0df549ea89caa3a5a05ce9d163ce594e4f5ca1056b4827eafa4cfff"]],"content":"","sig":"109ccf364b2bf6e6661384c443ea44a6fc55eda3a992d9fe7ac742e87dc115b811cb9b4cb4382b86af370f62b157778d4ecab8a80e207d4794449c96b79d610d"}','sent',1,NULL,1780000000,NULL);
INSERT INTO relay_outbox VALUES(2,'{"id":"5d71c46df2e68bf7231a4fe0aed9a6922defc06a35efe6cd1001c3e1a4361c7a","pubkey":"4d4b6cd1361032ca9bd2aeb9d900aa4d45d9ead80ac9423374c451a7254d0766","created_at":1780000000,"kind":30078,"tags":[["d","f59e526ba0df549ea89caa3a5a05ce9d163ce594e4f5ca1056b4827eafa4cfff"],["splitcard","f59e526ba0df549ea89caa3a5a05ce9d163ce594e4f5ca1056b4827eafa4cfff"]],"content":"","sig":"4fe18b578f39a90e09acf50b1988bf0be38b9907b4da72e8ec9a9812ed0fd4aeb6c686af7ed71b7983d50de27388a229c99d95a9d8ca86396673316c455de71a"}','sent',1,NULL,1780000000,NULL);
INSERT INTO relay_outbox VALUES(3,'{"id":"d6cfb099b9746d5ca5ba8645fe26299c97e3bac96a8f460cb081ad235f8f0427","pubkey":"531fe6068134503d2723133227c867ac8fa6c83c537e9a44c3c5bdbdcb1fe337","created_at":1780000000,"kind":30078,"tags":[["d","f59e526ba0df549ea89caa3a5a05ce9d163ce594e4f5ca1056b4827eafa4cfff"],["splitcard","f59e526ba0df549ea89caa3a5a05ce9d163ce594e4f5ca1056b4827eafa4cfff"]],"content":"","sig":"a862a7c19ce0895c3d4ba537bf92b4bcf0d8318396f0410f1897b4b0977fc60c7ee8d9e0beabc5793ffd8e50fd4564642196548610073d27d36a2ff38bd07384"}','sent',1,NULL,1780000000,NULL);
INSERT INTO relay_outbox VALUES(4,'{"id":"56fcceab26af69d15486ca47e8ad645200d6aef98186d1ed8600bbfd2188e3b9","pubkey":"5f64993ccb4044a005e82ba07e78b2815a6a17aa7d5ad62fd59d519778072483","created_at":1780000000,"kind":30078,"tags":[["d","f59e526ba0df549ea89caa3a5a05ce9d163ce594e4f5ca1056b4827eafa4cfff"],["splitcard","f59e526ba0df549ea89caa3a5a05ce9d163ce594e4f5ca1056b4827eafa4cfff"]],"content":"{\"card_id\":\"llamas-split\",\"event\":\"nostr-hack-2026\",\"members\":[{\"name\":\"Alice\",\"payout\":{\"lightning\":{\"address\":\"alice@blink.sv\"}},\"pubkey\":\"1b84c5567b126440995d3ed5aaba0565d71e1834604819ff9c17f5e9d5dd078f\",\"weight\":36},{\"name\":\"Bob\",\"payout\":{\"lightning\":{\"address\":\"bob@getalby.com\"}},\"pubkey\":\"4d4b6cd1361032ca9bd2aeb9d900aa4d45d9ead80ac9423374c451a7254d0766\",\"weight\":35},{\"name\":\"Chidi\",\"payout\":{\"lightning\":{\"address\":\"chidi@walletofsatoshi.com\"}},\"pubkey\":\"531fe6068134503d2723133227c867ac8fa6c83c537e9a44c3c5bdbdcb1fe337\",\"weight\":29}],\"rules\":{\"peer_review\":null},\"supersedes\":null,\"team\":\"Lightning Llamas\",\"version\":1}","sig":"b06578be756600cf10e42dc2f313ba11bcb656a01d98f388043529c2247d39f4982b3f03a242db75aafdd71f9a7de66e9ee4f617c76e24e959c7a4ec25c33c93"}','sent',1,NULL,1780000000,'5f64993ccb4044a005e82ba07e78b2815a6a17aa7d5ad62fd59d519778072483');
CREATE TABLE used_auth_events (
    event_id TEXT PRIMARY KEY,
    seen_at INTEGER NOT NULL
);
INSERT INTO used_auth_events VALUES('3d1dfff66d7d5e878f63e2ab569661cd93c02a4f7dfb250b87325fe6c2f01300',1780000000);
INSERT INTO used_auth_events VALUES('813a05412099c22dc156e6a26f4db0fe4657ed24b2eefbe81e0792a0ee35d823',1780000000);
INSERT INTO used_auth_events VALUES('4f66b883da25fc6c651413cae82a8a9a5299677a101d037a19d864e525721c6a',1780000000);
INSERT INTO used_auth_events VALUES('846a7530cc8d1e5bd89e27d95d9fc93cc1a01fb55c75246bfc00961a0b1d0199',1780000000);
INSERT INTO used_auth_events VALUES('089ba468a92cfaf20ed4077f8bdf1c5a460072244dd7d211c8d0d5e009b3deaf',1780000000);
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
INSERT INTO card_timestamps VALUES('f59e526ba0df549ea89caa3a5a05ce9d163ce594e4f5ca1056b4827eafa4cfff',x'004f70656e54696d657374616d7073000050726f6f6600bf89e2e884e892940108f59e526ba0df549ea89caa3a5a05ce9d163ce594e4f5ca1056b4827eafa4cfff000588960d73d7190103d9d737','complete',0,912345,0,NULL,1780000000,NULL,NULL,NULL);
CREATE TABLE relay_deliveries (
    outbox_id INTEGER NOT NULL REFERENCES relay_outbox (id),
    relay TEXT NOT NULL,
    status TEXT NOT NULL CHECK (status IN ('pending', 'accepted', 'failed')),
    attempts INTEGER NOT NULL DEFAULT 0,
    last_error TEXT,
    next_attempt_at INTEGER NOT NULL,
    PRIMARY KEY (outbox_id, relay)
);
INSERT INTO relay_deliveries VALUES(1,'wss://relay.one.test/','accepted',1,NULL,1780000000);
INSERT INTO relay_deliveries VALUES(2,'wss://relay.one.test/','accepted',1,NULL,1780000000);
INSERT INTO relay_deliveries VALUES(3,'wss://relay.one.test/','accepted',1,NULL,1780000000);
INSERT INTO relay_deliveries VALUES(4,'wss://relay.one.test/','accepted',1,NULL,1780000000);
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
CREATE INDEX cards_team ON cards (team_id);
CREATE INDEX relay_outbox_due ON relay_outbox (status, next_attempt_at);
CREATE INDEX used_auth_events_seen ON used_auth_events (seen_at);
CREATE INDEX card_timestamps_due ON card_timestamps (status, next_attempt_at);
CREATE INDEX relay_deliveries_due ON relay_deliveries (status, next_attempt_at);
COMMIT;

-- The key that signed each server card event, so verification checks what was
-- actually signed rather than the server's current key. NULL for member
-- signature events, which are queued alongside card events.

ALTER TABLE relay_outbox ADD COLUMN server_pubkey TEXT;

-- Every outbox entry is a card's signature events, copied from
-- card_signatures, or the server's card event. Record the signer of the rest.
UPDATE relay_outbox
SET server_pubkey = lower(json_extract(event_json, '$.pubkey'))
WHERE NOT EXISTS (
    SELECT 1 FROM card_signatures s
    WHERE json_extract(s.event_json, '$.id') = json_extract(relay_outbox.event_json, '$.id')
);

-- rubic receipt log: append-only, hash-chained.
--
-- Each row stores the verbatim signed JSON receipt plus indexable hash
-- columns so verify endpoints can replay the chain without re-parsing JSON.

CREATE TABLE IF NOT EXISTS receipts (
    id           INTEGER PRIMARY KEY AUTOINCREMENT,
    receipt_json TEXT    NOT NULL,
    -- BLAKE3 over (canonical_bytes || sig); becomes the next row's prev_hash.
    this_hash    BLOB    NOT NULL,
    -- Predecessor's `this_hash`; NULL only for the very first receipt.
    prev_hash    BLOB,
    created_at   TEXT    NOT NULL  -- RFC3339 UTC
);

CREATE INDEX IF NOT EXISTS idx_receipts_this_hash ON receipts (this_hash);
CREATE INDEX IF NOT EXISTS idx_receipts_created_at ON receipts (created_at);

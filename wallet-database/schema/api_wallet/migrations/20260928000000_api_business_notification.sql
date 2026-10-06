CREATE TABLE api_business_notification (
    id TEXT PRIMARY KEY,
    uid TEXT NOT NULL CHECK (length(uid) > 0),
    kind TEXT NOT NULL,
    business_key TEXT NOT NULL,
    payload TEXT NOT NULL,
    is_read INTEGER NOT NULL DEFAULT 0 CHECK (is_read IN (0, 1)),
    resolved_at TIMESTAMP,
    created_at TIMESTAMP NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    updated_at TIMESTAMP NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    UNIQUE (uid, kind, business_key)
);

CREATE INDEX api_business_notification_owner_time_idx
    ON api_business_notification (uid, created_at DESC);

CREATE INDEX api_business_notification_owner_active_idx
    ON api_business_notification (uid, resolved_at, created_at DESC);

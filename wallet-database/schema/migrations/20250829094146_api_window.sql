-- Add migration script here

CREATE TABLE api_window
(
    id               INTEGER PRIMARY KEY,
    offset  INTEGER,
    created_at       TIMESTAMP               NOT NULL,
    updated_at       TIMESTAMP
);


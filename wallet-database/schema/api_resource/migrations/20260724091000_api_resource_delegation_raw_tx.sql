-- Persist signed resource transactions before broadcast.

ALTER TABLE api_resource_delegation
    ADD COLUMN raw_tx TEXT NULL;

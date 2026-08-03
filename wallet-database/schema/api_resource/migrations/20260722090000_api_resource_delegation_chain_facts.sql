-- Align resource delegation chain facts with collect/withdraw transaction facts.

ALTER TABLE api_resource_delegation
    ADD COLUMN last_broadcast_at TIMESTAMP NULL;

ALTER TABLE api_resource_delegation
    ADD COLUMN chain_success_at TIMESTAMP NULL;

ALTER TABLE api_resource_delegation
    ADD COLUMN chain_failed_at TIMESTAMP NULL;

ALTER TABLE api_resource_delegation
    ADD COLUMN transaction_time TIMESTAMP NULL;


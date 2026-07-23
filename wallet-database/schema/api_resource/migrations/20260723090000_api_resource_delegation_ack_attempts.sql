-- Align resource ACK facts with collect/withdraw: attempt count plus sent fact.

ALTER TABLE api_resource_delegation
    ADD COLUMN task_ack_send_count INTEGER NOT NULL DEFAULT 0;

ALTER TABLE api_resource_delegation
    ADD COLUMN result_ack_send_count INTEGER NOT NULL DEFAULT 0;

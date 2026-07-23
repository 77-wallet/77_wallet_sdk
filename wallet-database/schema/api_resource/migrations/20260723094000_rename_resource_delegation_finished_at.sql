-- Align resource terminal facts with collect and withdraw terminology.

ALTER TABLE api_resource_delegation
    RENAME COLUMN result_received_at TO finished_at;

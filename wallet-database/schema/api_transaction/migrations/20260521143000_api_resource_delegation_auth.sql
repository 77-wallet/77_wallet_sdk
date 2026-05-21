-- Temporary development migration: keep the original resource delegation
-- migration checksum stable for existing local test databases.

ALTER TABLE api_resource_delegation
    ADD COLUMN delegation_mode INTEGER NOT NULL DEFAULT 1; -- 1=平台出款地址代理；2=授权地址代理

ALTER TABLE api_resource_delegation
    ADD COLUMN permission_id TEXT NULL; -- 授权地址代理时使用的 TRON active permission id

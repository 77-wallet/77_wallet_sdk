-- Add migration script here
ALTER TABLE coin ADD COLUMN swappable integer DEFAULT 1 NOT NULL;
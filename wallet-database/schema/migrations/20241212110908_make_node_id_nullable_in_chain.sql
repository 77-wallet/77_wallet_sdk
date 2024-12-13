-- Step 1: Rename the original table
ALTER TABLE chain RENAME TO chain_old;

-- Step 2: Create a new table with the desired schema
CREATE TABLE chain (
    name VARCHAR(64) NOT NULL,
    chain_code VARCHAR(32) NOT NULL,
    node_id VARCHAR(64),
    protocols TEXT NOT NULL,
    main_symbol VARCHAR(16) NOT NULL,
    status INTEGER NOT NULL,
    created_at TIMESTAMP NOT NULL,
    updated_at TIMESTAMP,
    PRIMARY KEY (chain_code)
);

-- Step 3: Copy data from the old table to the new table
INSERT INTO chain (name, chain_code, node_id, protocols, main_symbol, status, created_at, updated_at)
SELECT name, chain_code, node_id, protocols, main_symbol, status, created_at, updated_at
FROM chain_old;

-- Step 4: Drop the old table
DROP TABLE chain_old;

-- Step 5: Recreate the index
CREATE INDEX chain_chain_code_idx ON chain (chain_code);

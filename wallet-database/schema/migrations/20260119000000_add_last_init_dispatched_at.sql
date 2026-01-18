-- Add last_init_dispatched_at column to expand_batch_item table
ALTER TABLE expand_batch_item
ADD COLUMN last_init_dispatched_at TIMESTAMP NULL;

-- Create index for better query performance on last_init_dispatched_at
CREATE INDEX IF NOT EXISTS idx_expand_batch_item_last_init_dispatched ON expand_batch_item(last_init_dispatched_at);

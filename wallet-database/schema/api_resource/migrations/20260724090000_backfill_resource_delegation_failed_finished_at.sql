-- Backfill terminal failures created before mark_failed_if_unfinished recorded finished_at.

UPDATE api_resource_delegation
SET finished_at = COALESCE(chain_failed_at, updated_at, created_at)
WHERE finished_at IS NULL
  AND (
        chain_failed_at IS NOT NULL
        OR err_code IS NOT NULL
        OR status = 3
      );

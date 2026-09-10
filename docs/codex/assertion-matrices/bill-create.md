# Bill creation

Scope: `BillRepo::create` / `BillDao::create`, including the existing upsert.

| Scenario | Required result | Regression |
| --- | --- | --- |
| Ordinary bill | Persist and retrieve the bill | `bill_repo_create_and_get_by_hash_opt_success` |
| Text fields contain apostrophes | Preserve the exact address and note text as data | `bill_repo_create_preserves_apostrophes_in_text_fields` |
| Transaction rolls back | Bill remains absent | `bill_repo_tx_rollback_keeps_bill_absent` |
| Owner contains an apostrophe | Latest matching bill is returned | `bill_repo_last_kind_bill_accepts_apostrophe_in_owner` |
| Owner and hash contain apostrophes | Matching hash list is returned | `bill_repo_lists_by_hashs_accepts_apostrophes` |

This coverage includes bill insertion and two non-pagination lookups. Dynamic
bill filters and pagination remain outside its scope.

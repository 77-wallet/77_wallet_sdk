# API Wallet Resource Assertion Matrix

Scope: `wallet-api` API wallet foreground resource and governance operations.

## Resource Delegation Retry Backoff

- Test:
  `wallet-api/src/infrastructure/api_trans/resource_delegation.rs`
  `retry_wait_secs_follows_exp_backoff_with_cap`
- Entry point: shared resource delegation retry wait calculation.
- Runtime facts: retry waits are `60s`, `120s`, `240s`, then capped at `300s`.
- Invariant: transient RPC failures must not push resource delegation fallback
  into long waits approaching one hour.

## Resource Delegation ACK Claim

- Test:
  `wallet-database/src/repositories/api_wallet/resource_delegation.rs`
  `resource_delegation_ack_claim_blocks_duplicate_task_ack_requests`
- Test:
  `wallet-database/src/repositories/api_wallet/resource_delegation.rs`
  `resource_delegation_ack_claim_blocks_duplicate_result_ack_requests`
- Entry point: resource delegation task/result ACK claim helpers used before
  backend `trans_event_ack` calls.
- DB facts: claiming an ACK sets `next_retry_at` into a short future window
  without marking `task_ack_sent_at` or `result_ack_sent_at`.
- Retry: a competing scanner/worker for the same resource trade no observes
  zero claimed rows and does not send the backend request; a failed owner can
  still be retried after the claim/retry wait expires.
- Invariant: collect/withdraw shadow scanners and platform resource scanners
  must not concurrently send duplicate backend ACK requests for the same
  resource delegation row.

## Platform Resource Reclaim TX_RES Gate

- Test:
  `wallet-database/src/repositories/api_wallet/resource_delegation.rs`
  `platform_undelegation_result_ack_requires_tx_res_received_at`
- Test:
  `wallet-api/src/infrastructure/api_trans/resource_reclaim/platform_shadow.rs`
  `platform_undelegation_recover_does_not_send_result_ack_before_mqtt_result`
- Entry point: platform resource reclaim result ACK scan and worker.
- DB facts: SDK chain recovery writes `result_received_at/result_payload`, but
  leaves `tx_res_received_at` empty; MQTT final result writes
  `tx_res_received_at`.
- UI/API facts: `TX_RES` ACK is sent only after backend final-result MQTT is
  received, keeping the flow display in backend-result-before-ACK order.
- Invariant: platform resource reclaim must not ACK `TX_RES` from SDK chain
  recovery facts alone.

## Platform Resource Delegation Confirmation Gate

- Test:
  `wallet-database/src/repositories/api_wallet/resource_delegation.rs`
  `resource_delegation_broadcast_mark_is_pending_until_chain_success`
- Test:
  `wallet-api/src/infrastructure/api_trans/resource_chain_facts.rs`
  `broadcast_only_resource_is_not_success_uploadable`
- Test:
  `wallet-api/src/infrastructure/api_trans/resource_chain_facts.rs`
  `legacy_tx_status_success_is_not_a_chain_success_fact`
- Entry point: platform resource delegation broadcast and receipt-upload scans.
- DB facts: broadcasting a resource delegation records `tx_hash` and
  `last_broadcast_at` but leaves `tx_status` empty; chain recovery must write
  `chain_success_at` / `transaction_time` before the row becomes eligible for
  backend SUCCESS execution-receipt upload.
- Shared helper: resource side-effect workers use `resource_chain_facts` for
  broadcast, success, failure, and receipt-upload eligibility.
- Uncertain fact: resource recover returning no final chain result records
  `broadcast_uncertain_since_at`, increments `broadcast_uncertain_retry_count`,
  increments `broadcast_uncertain_rebroadcast_count` when uncertain timeout
  invalidates the old broadcast lifecycle for rebuild, and clears the uncertain
  lifecycle after chain success or failure.
- Invariant: `WD_RSC_DL` / `COL_RSC_DL` SUCCESS receipts must not be uploaded
  from broadcast-only hash facts.
- Test:
  `wallet-database/src/repositories/api_wallet/resource_delegation.rs`
  `broadcast_fact_cannot_be_overwritten_by_local_failure`
- Test:
  `wallet-database/src/repositories/api_wallet/resource_delegation.rs`
  `deterministic_broadcast_hash_reconciles_after_build_claim_reset`
- Test:
  `wallet-database/src/repositories/api_wallet/resource_delegation.rs`
  `recovered_chain_success_and_result_are_committed_together`
- Test:
  `wallet-database/src/repositories/api_wallet/resource_delegation.rs`
  `recovered_chain_success_rejects_conflicting_hash_without_finishing`
- Invariant: local execution failure must not overwrite a persisted broadcast
  hash or any prior broadcast lifecycle; rebuild preserves
  `last_broadcast_at` as an irreversible broadcast fact. Confirmed recovery
  must restore its deterministic hash and atomically persist the chain-success
  fact and terminal SUCCESS result, even if an uncertain rebuild cleared the
  prior pending-broadcast snapshot.
- Invariant: cross-database resource-gate projection must complete before the
  resource result ACK fact is marked locally, so a projection failure remains
  retryable instead of leaving the origin order permanently blocked.

## API Withdraw Wallet Unfreeze Extraction

- Test:
  `wallet-api/src/service/api_wallet/transaction.rs`
  `api_resource_withdraw_unfreeze_operation_maps_to_withdraw_unfreeze_bill_detail_entity`
- Entry point: API wallet resource operation detail conversion.
- DB facts: client resource operation uses
  `ApiResourceOperationType::WithdrawUnfreeze`.
- UI/API facts: bill detail maps to `BillKind::WithdrawUnFreeze`.
- Invariant: API withdraw-wallet unfreeze extraction must not be represented as
  normal-wallet stake history or as vote reward withdrawal.

- Test:
  `wallet-api/src/service/api_wallet/transaction.rs`
  `api_resource_withdraw_unfreeze_operation_stays_confirming_after_broadcast_only`
- Entry point: API wallet resource operation detail conversion.
- DB facts: broadcast facts can exist without `tx_status` or `result_status`.
- UI/API facts: broadcast-only extraction detail returns confirming status.
- Invariant: client-broadcasted API resource operations do not become success
  until chain/result confirmation is recorded.

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

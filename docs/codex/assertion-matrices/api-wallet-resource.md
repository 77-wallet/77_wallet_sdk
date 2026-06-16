# API Wallet Resource Assertion Matrix

Scope: `wallet-api` API wallet foreground resource and governance operations.

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

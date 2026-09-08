# API Wallet Transfer Assertion Matrix

Scope: `wallet-api` API wallet transfer flow.

Rules source: `docs/codex/testing.md` and `docs/codex/testing-strategy.md`.

## Current Standard Tests

### Transfer Nonce Lock Serializes Same Address

- Layer: integration.
- Entrypoint:

  ```text
  tests/integration/api_wallet/transfer_nonce/mod.rs
  api_wallet_transfer_nonce_lock_keeps_same_address_requests_serial
  ```

- Backend: none; the chain path uses a fake BNB transaction adapter.
- DB facts: BNB nonce floor starts at `0` for the fixture wallet.
- Chain calls: first transfer records nonce `1`; second transfer stays blocked
  until the first transfer is released, then records nonce `2`.
- Result: returned transaction hashes encode nonce `1` and nonce `2`.
- Invariant: transfers from the same address must reserve and use nonces
  serially under concurrency.

### Transfer Failure Keeps Reserved Nonce

- Layer: integration.
- Entrypoint:

  ```text
  tests/integration/api_wallet/transfer_nonce/mod.rs
  api_wallet_transfer_nonce_failure_keeps_reserved_nonce
  ```

- Backend: none; the chain path uses a fake BNB transaction adapter that fails
  transfer.
- DB facts: after the simulated transfer failure, nonce floor remains advanced
  to `1`.
- Chain calls: fake adapter records exactly nonce `1`.
- Invariant: a failed transfer must not rewind the reserved nonce and make it
  reusable.

## Template Contract

`transfer_nonce/mod.rs` uses the role-trait Given-When-Then shape:

1. Keep transfer nonce cases in `transfer_nonce/mod.rs`.
2. Keep fake chain adapter state, fixture address, and adapter guard in
   `TransferNonceScenario`.
3. Expose `scenario.given()`, `scenario.when()`, and `scenario.then()` as the
   fixed read-first integration roles using generic containers from
   `tests/harness`.
4. Keep BNB fixture setup, fake chain adapter behavior, and cached wallet
   password in the flow-local `TransferNonceGiven` trait.
5. Keep transfer entrypoints and explicit release of the blocked first transfer
   in the flow-local `TransferNonceWhen` trait.
6. Keep recorded nonce order, result hashes, error text, and persisted nonce
   floor assertions in the flow-local `TransferNonceThen` trait.
7. Keep seed/load/assert internals behind harness role containers and below the
   Given-When-Then test body.
8. Keep adapter override, repository setup, and transfer request construction
   details below the scenario layer in `transfer_nonce/support`.

`tests/harness` remains reserved for cross-flow environment and fake
capabilities. `src/testkit` remains reserved for crate-private worker or
scanner entrypoints.

## Gaps To Close Next

- Transfer nonce multi-address concurrency:
  different sender addresses should not block each other.
- Transfer idempotency:
  retry behavior should not double-send a transaction after a successful chain
  response.

## Nonce alignment reservation safety (2026-09-07)

- `nonce_alignment_preserves_reserved_nonces`: reserve 9, 10, 11; an RPC next
  nonce of 9 must not rewind the floor or reissue 9. Alignment reports
  `nonce_gap_requires_recovery`; subsequent allocation returns 12.
- `nonce_alignment_and_allocation_preserve_reservations`: concurrent alignment
  and allocation preserve outstanding reservation 9 and allocate 10, then 11.
- `nonce_alignment_advances_floor_without_reserving`: upward chain progress
  advances the last-used floor to next-1, leaving the next nonce available.
- `nonce_alignment_rpc_failure_preserves_floor`: failed observation does not
  mutate allocation state or send a transaction.
- `nonce_alignment_frozen_address_does_not_query_or_mutate`: frozen alignment
  fails before RPC/DB mutation. The test releases its manual freeze afterward.
- All cases use the existing fake transaction adapter and real nonce repository;
  assert persisted floor and RPC count. Signed-transaction recovery and automatic
  gap filling are not claimed by these allocator tests.

## Ordinary EVM transfer chain-aware allocation (batch 2a)

The ordinary ETH/BNB transfer entrypoint now queries chain next nonce before
atomic allocation, within its existing per-address transfer gate.
- `ordinary_transfer_nonce_syncs_external_chain_progress`: with local last=0,
  real transfer orchestration passes nonce 9 then 15 to the fake chain sender as
  external chain state advances; verifies returned hashes, sender calls and DB floor.
- `ordinary_transfer_nonce_rpc_failure_recovers_without_sending`: nonce RPC error
  leaves the floor unchanged and sends nothing; a later successful observation
  permits the same transfer path to send with nonce 9, proving the gate releases.
- Existing same-address serialization and post-allocation transfer-failure tests
  remain enabled. This batch does not change withdrawal/collect-fee entrypoints,
  address normalization, replacement policy or reservation release semantics.


## Ordinary transfer gap protection (2026-09-08, batch 1)

- `ordinary_transfer_uncertain_nonce_blocks_next_allocation`: fake submission of
  nonce 4 fails, chain pending next stays 4. A repeated ordinary call returns
  `nonce_gap_requires_recovery`; nonce floor remains 4 and there is only one send.
  When observation advances to 5, another submission succeeds with nonce 5.
  This is allocation recovery, not proof that the original transaction succeeded.
- `ordinary_transfer_timeout_blocks_next_allocation`: adapter timeout preserves
  nonce reservation 4 and prevents another send/allocation while node stays at 4.
- Existing same-address serialization test now models node pending advancing on
  accepted sends. Both nonce 1 and nonce 2 are still returned in order.
- Existing collect/withdraw/fee queue tests retain their allocation policy and
  unique reservations. The gap guard applies only to ordinary ETH/BNB transfers.
- A concurrent reservation between observation and allocation also fails closed
  if the resulting nonce differs from the observation; its reservation is kept.

Validation: `cargo test -p wallet-api --test mod --features integration-tests transfer_nonce`.
Offline fake chain and real SQLite; no production transaction or migration.
A stale node may conservatively block a legitimate next transfer. Signed raw/hash
persistence, restart recovery, identical-raw retry and atomic cross-flow allocation
are separate batches. This change does not repair an existing nonce gap.


## SelfWithdraw durable facts (2026-09-08, database batch 2)

Component entrypoint: `wallet-database/src/repositories/api_wallet/withdraw/self_transfer_tests.rs`.
The test module is attached to `ApiWithdrawRepo`. `SignedSelfTransfer` lives in
`entities/api_withdraw.rs`; SQL lives in `ApiWithdrawDao`;
`persist_signed_self_transfer`, `mark_self_transfer_accepted` and
`pending_self_transfers_for_sender` are repository entrypoints. Transaction
coordination and identity validation remain in the repository. These methods
do not invoke the backend withdrawal status recomputation. There is no separate
`self_transfer` production module.
Real temporary SQLite with migrations, no backend or RPC calls.

- `self_transfer_survives_reopen_and_acceptance_is_not_confirmation`: signed
  nonce/hash/raw survive pool close and reopen; pre-send uncertainty exists.
  Backend withdrawal broadcast/recovery scans exclude the ordinary record.
  Only matching-hash acceptance clears uncertainty, without confirmation or ACK.
  Repeating persistence after acceptance does not restore uncertainty.
- `self_transfer_conflicting_retry_preserves_original_and_nonce_floor`: changed
  bytes/nonce under the same trade number are rejected. Identical retry preserves
  a higher nonce floor instead of rewinding it.
- `self_transfer_nonce_write_failure_rolls_back_signed_record`: failure injected
  after the record insert rolls back the complete transaction and leaves the old
  counter intact. A later retry succeeds after the failure is removed.
- `self_transfer_isolates_sender_chain_trade_type_and_terminal_rows`: pending
  lookup and acceptance cannot advance backend withdrawals or terminal records;
  sender and chain filters isolate ordinary work.
- `self_transfer_bnb_zero_nonce_and_invalid_input`: BNB nonce zero is valid;
  non-EVM/negative nonce/empty raw are rejected without reserving a nonce.
- `self_transfer_concurrent_identical_retries_write_one_record`: duplicate
  concurrent persistence creates one durable intent and does not increment nonce.

Command: `cargo test -p wallet-database --lib self_transfer`.
No migration and no API wiring in this batch. The caller must provide a stable
trade number and validated signed payload before broadcast. The early uncertainty
marker is conservative: it does not assert that an RPC was sent. Pending lookup
returns signed, unfinished records; reconciliation policy remains a later batch.


## Ordinary EVM API durable broadcast (batch 3)

The ordinary ETH/BNB entrypoint now builds signed raw, calls
`ApiWithdrawRepo::persist_signed_self_transfer`, then broadcasts that same raw.
It marks node acceptance only after visibility verification and matching hash.
The existing signed builder stores the EIP-1559 maximum fee budget; this is not
an actual receipt fee. Non-EVM transfer paths keep their existing implementation.

- Existing same-address serialization case reads hash/nonce/raw/uncertainty via
  a separately opened SQLite pool while the first fake broadcast is blocked.
  Successful calls retain raw, clear uncertainty and set only node acceptance.
- Existing failure/recovery and timeout cases now also assert exact durable raw
  and hash. Chain nonce progress allows a new send without marking the earlier
  uncertain transaction confirmed.
- `ordinary_transfer_signed_persistence_failure_never_sends`: failed pre-send DB
  insert leaves no signed row and no broadcast. Reservation remains; retry at
  the unchanged node nonce fails closed.
- `ordinary_transfer_invisible_submission_stays_uncertain`: a returned hash
  without node visibility preserves uncertainty rather than reporting success.
- `ordinary_transfer_wrong_response_hash_cannot_mark_accepted`: mismatched hash
  does not replace the local hash or mark node acceptance.
- `ordinary_transfer_acceptance_write_failure_preserves_signed_transaction`:
  a DB failure after node acceptance retains raw and uncertainty for reconciliation.

Validation: `cargo test -p wallet-api --test mod --features integration-tests transfer_nonce`.
No automatic rebroadcast or recovery worker yet. A failure before signed facts
are persisted can still leave a reserved nonce with no recoverable raw, and must
not be auto-released. Tests use a fake BNB adapter and real SQLite; full process
restart and real ETH/BNB RPC acceptance remain separate acceptance work.


## SelfWithdraw observed receipt completion (database batch 4)

`SelfTransferReceipt` belongs to entities; `ApiWithdrawDao` applies the guarded
receipt update; `ApiWithdrawRepo::confirm_self_transfer_receipt` wraps receipt
and existing `mark_chain_finished` writes in one transaction. No backend status
recomputation or ACK is performed. Only signed SelfWithdraw ETH/BNB rows matching
trade number, hash and chain with no prior receipt/terminal/error fact qualify.
The caller must supply an actually observed receipt; absence/RPC failure is not
an input to this method.

SQLite component cases in `repositories/api_wallet/withdraw/self_transfer_tests.rs`:
- `self_transfer_receipt_finishes_success_and_failure_without_nonce_reuse`:
  receipt success/failure records the matching terminal flag, block/time and
  actual fee, clears uncertainty and leaves raw/hash/nonce intact. Pending lookup
  no longer includes the record; backend ACK fields remain empty.
- `self_transfer_receipt_rejects_wrong_identity_and_backend_withdrawal`:
  mismatched hash, chain, trade number and backend trade type cannot be updated.
- `self_transfer_receipt_concurrent_retry_and_opposite_result_cannot_overwrite`:
  one concurrent caller writes; duplicate/opposite results preserve terminal facts.
- `self_transfer_receipt_terminal_write_failure_rolls_back_then_recovers`:
  injected finish-write failure rolls back receipt/status/fee changes. Removing
  the failure permits retry without losing signed data or rewinding the nonce.

Validation: `cargo test -p wallet-database --lib self_transfer` (10 passing cases).
This batch supplies persistence only. API query/recovery integration, receipt
observation policy and identical-raw rebroadcast remain subsequent work.


## Background receipt discovery

`self_transfer_scan_cursor_survives_earlier_completion` verifies bounded id-cursor
pages do not skip a later signed record when an earlier row finishes. Zero limit
returns nothing and backend withdrawals are excluded. Database scan caps pages
at 100. Database checkpoint: all 11 self_transfer component cases pass.


## Automatic ordinary EVM receipt observer (API batch B)

Withdrawal actor startup owns one observer handle and stop signals/joins it.
The application observer uses cancellable phase jitter (0–15 seconds initially),
a production interval of 30 seconds and bounded id-cursor pages of 32. Each
observation has a 35-second outer timeout; shutdown cancels the round immediately.
No nonce query/allocation, signing, broadcast or backend ACK occurs in observation.
Only an observed matching hash with status 2/3, valid mined block/time and finite
nonnegative fee is passed to the dedicated SelfWithdraw receipt transaction.
Missing/invalid/RPC-error results preserve the durable pending record.

Integration cases (fake BNB adapter, real SQLite, existing transfer_nonce suite):
- `ordinary_transfer_observer_preserves_uncertainty_until_valid_receipt`: missing,
  RPC error, nonterminal status and hash mismatch remain uncertain; later receipt
  finishes with actual fee. Repeated rounds skip terminal rows and never resend.
- `ordinary_transfer_observer_records_onchain_failure_without_resending`: a mined
  failed receipt sets failure without releasing/reallocating nonce or broadcasting.
- `ordinary_transfer_background_observer_cancels_restarts_and_retries`: cancel a
  hung query, restart observation from saved DB records, see initial missing result,
  then finish on a later timer tick without a manual query. Test interval is 20ms;
  production actor supplies 30s. This tests observer restart, not a full OS process
  restart. Pending raw/hash and nonce remain available across cancellation.

Validation: transfer_nonce integration target; no real RPC or automatic rebroadcast.
App shutdown/suspension stops execution; observation resumes when its actor starts.

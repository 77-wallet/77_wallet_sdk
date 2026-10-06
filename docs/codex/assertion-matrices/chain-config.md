# Chain configuration precedence

Scope: `wallet-api` default chain selection for Core and API Wallet.

| Invariant | Offline test |
| --- | --- |
| A chain returned disabled by the backend stays disabled in dev | `dev_does_not_restore_backend_disabled_local_chain` |
| A locally active chain absent from the backend response remains available for early dev testing | `dev_restores_local_chain_missing_from_backend` |
| Test and prod use only the backend-enabled chain selection | `test_and_prod_keep_backend_chain_selection` |
| Five EVM chain descriptors/testnet defaults stay shared; dev also supplies shared mainnet nodes/native coins. Fantom has no local seed | `added_evm_chains_are_testnet_only`, `added_evm_chains_have_native_testnet_coins`, `added_evm_chains_have_active_testnet_nodes` |
| An enabled chain missing from any wallet/account mapping requests chain synchronization | `enabled_chain_is_missing_when_any_wallet_account_lacks_it` |
| An enabled chain already present for every wallet/account mapping does not request redundant synchronization | `enabled_chain_is_complete_when_every_wallet_account_has_it` |

Test layer: unit. The tests use static local configuration and synthetic backend chain-code lists;
they perform no backend calls, database writes, or network requests.

Failure invariant: local dev fallback may supplement a missing backend chain, but must never
override an explicit backend disable. Core and API Wallet pass both the complete returned chain
set and the enabled subset through the same selection rule.
Fantom remains a recognized chain code for compatibility when supplied by the backend, but is
not locally auto-enabled as part of 2.2.

Known coverage gap: repository-level status persistence is covered by existing chain repository
tests rather than repeated here.

Run:

- `cargo test -p wallet-api --lib --no-default-features default_data::chain::tests`
- `cargo test -p wallet-api --lib --no-default-features domain::chain::tests`

## B: approved shared dev defaults (2026-10-05 simplification)

Current policy: Polygon/POL, Avalanche/AVAX, Arbitrum/ETH, Base/ETH, Scroll/ETH.
Ordinary/API wallets share static metadata and Core node storage; chain/coin tables remain separate.
Profiles are compile-time only (prod wins over dev). Cross-profile installs retaining a DB are unsupported developer misuse, not an acceptance requirement.

Fresh dev initialization creates five shared mainnet nodes through NodeDomain's existing initializer and deterministic node IDs.
The five testnet defaults remain in the shared catalog and are initialized in test builds; existing dev testnet nodes and bindings are preserved.
Existing selector ordering stays backend first, newest updated_at, then node_id; valid manual bindings stay, valid local automatic bindings stay until a backend arrives.
New node rows use insert-only defaults so reinitialization cannot revive a disabled node or replace its RPC metadata.
The redundant global visibility refresh after initialization was removed: ordinary defaults already receive profile status when written, and insert-only five-chain rows retain saved status.
API chain defaults insert missing rows, and default status refresh excludes the five dev rows to preserve backend disable/version/missing-master decisions.
API native defaults require an enabled API chain, active bound mainnet node and matching symbol; insert-only SQL rechecks chain eligibility.
API derivation resolves the API chain's node. Existing coin protocol failures remain visible.
Ordinary network coin synchronization does not disable a key present in the selected network catalog (shared native keys regression).

Removed: API-only node prefix/creation, ordinary-wallet dev exclusion, cross-profile node filtering/getter guards/CAS unbinding and their helper methods/tests.
No general replacement framework, runtime environment switch, Core/plugin changes, or transaction changes.

| Invariant | Offline evidence |
| --- | --- |
| Ordinary/API five-chain mainnet defaults share node IDs and both have native coins | `dev_both_wallets_share_five_mainnet_defaults_without_duplicate_nodes` |
| API derivation works without ordinary chain rows; completed registration is not repeated | `api_only_chain_derives_without_a_standard_chain_and_retries_unregistered_address` |
| Backend disable, version gate and missing master remain authoritative | `backend_disable_version_gate_and_missing_master_are_not_restored_by_dev_defaults`, `missing_master_backend_response_before_initialization_stays_disabled`, fake backend integration |
| Manual testnet binding and disabled API coin metadata remain intact | `dev_defaults_preserve_manual_testnet_binding_and_disabled_backend_coin` |
| Same dev DB reopen/reinitialize adds no duplicate nodes and preserves disabled node/RPC, chain, coin and ordinary/API manual binding | `same_dev_upgrade_preserves_disabled_nodes_chains_coins_and_manual_bindings` |
| Fresh test/prod/dev+prod do not inject five dev mainnet nodes or API native coins | `non_dev_builds_do_not_inject_api_mainnet_nodes_or_native_coins`, `compiled_feature_priority_and_fresh_default_injection_match_policy` |
| Ordinary default coin synchronization supports the five mainnet defaults in dev | `ordinary_coin_sync_uses_five_mainnet_defaults_in_dev` |
| Fallback coin writes recheck current chain enable/master/binding and do not replace existing rows | API-chain/API-coin/Node SQLite repository tests |
| Protocol failure writes no fallback; valid empty response can fill native defaults | `coin_protocol_failure_is_visible_and_valid_empty_response_can_fill_dev_native_coins` |

Run targeted lib config/ensurer tests with `--no-default-features --features dev`, `test`, `prod`, and `dev,prod`; database repository tests; `--test mod --features dev,integration-tests` chain_sync tests.
Current command/status evidence is in root target/api-config-simplified-results.json; logs use the api-config-simplified prefix.
The former api-config-only candidate (27 files, da098e... hash) and results are historical; they do not validate this simplified candidate.


Review boundary: configuration-only B candidate; account expansion/recovery A remains separately in the original worktree, including recorded P2. No commit/push.

Actual base-to-new dev upgrade: runner creates a SQLite fixture with base2dd production code, then the new dev binary fills five mainnet nodes and eligible API native coins while preserving disabled chain/node/coin metadata and existing testnet bindings. `actual_base_dev_database_upgrade_fills_defaults_and_preserves_saved_state` is ignored and requires the runner-owned offline fixture. Existing five-chain node timestamps are controlled to model old rows deterministically.

Approved five-chain ordinary default guard: only five-chain records use atomic insert-missing. Existing records remain byte-for-byte equivalent (including backend disable, manual binding and metadata); absent rows are filled. Other default chains, generic upsert and backend updates retain legacy behavior. Regression drives actual CHAIN_LIST Domain entry before defaults; DAO/Repo race component checks backend disable vs default insert and duplicate concurrent inserts. Actual base upgrade red is retained and rerun on a new disposable fixture after the fix.

## Current develop integration (2026-10-06)

Integrate only committed first-stage ef84a2e27-file delta onto2cc1a2b5. Same fixed candidate must pass five-chain dev/default disable/manual-binding guards and existing withdrawal_message_center/global_business_query service+repository regressions. Original A pagination/worker/index/price changes are excluded. Fourfeature profiles(dev/test/prod/dev+prod), bounded affected SQLite filters, localfake chain_sync andfmt; preserve current message-center/dual-business implementation hashes outside B scope. This local dependency integration does not assert App/live or50万-address capacity.

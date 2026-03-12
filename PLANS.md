# PLANS

Current task execution plan.
Refs: `docs/codex/testing.md`, `docs/codex/checklists/pr-definition-of-done.md`.

## Task

- Name: wallet-api remove api-sync symbol dependency
- Goal:
  - 在 API wallet 资产同步链路中移除 `symbol` 参数依赖
  - API wallet 同步仅按 `chain_code + token_address`（空 token 归一为 `""`）筛选资产
  - 普通钱包 `AssetsDomain` 行为保持不变

## Batch Scope

### In

- `wallet-api/src/domain/api_wallet/assets.rs`
- `wallet-api/src/infrastructure/inner_event.rs`
- `PLANS.md`

### Out

- 普通钱包 `wallet-api/src/domain/assets/mod.rs` 逻辑改造
- 数据库 schema / migration / entity 变更
- acct_change repair / coin 创建 / frontend notify 语义调整

## Constraints

- Keep this round within one crate and one flow
- Keep `SyncTarget::Assets` symbol path unchanged
- Keep API wallet filtering token-only

## Plan

1. Remove `symbol` parameters from API wallet sync interfaces in `ApiAssetsDomain`
2. Update `InnerEvent` API-wallet branch call path to stop passing `symbol`
3. Keep token-key regression coverage and add one interface-focused no-symbol regression

## Validation Commands

- `cargo test -p wallet-api --lib api_wallet_acct_change_syncs_sol_usdc_by_token_address_when_symbol_differs -- --nocapture`
- `cargo test -p wallet-api --lib api_wallet_acct_change_syncs_native_asset_by_empty_token_without_symbol_matching -- --nocapture`
- `cargo test -p wallet-api --lib api_wallet_acct_change_does_not_sync_other_assets_with_different_token_address -- --nocapture`
- `cargo test -p wallet-api --lib api_wallet_sync_filter_ignores_symbol_dimension -- --nocapture`

## Stop Condition

- Stop after API wallet sync interfaces no longer accept `symbol` and all 4 targeted tests pass
- Do not expand changes into normal-wallet sync path in this round

## Progress Checklist

- [x] Update plan for this batch
- [x] Remove API-wallet symbol dependency in sync interfaces
- [x] Add/keep focused token-key regression tests
- [x] Run focused validation

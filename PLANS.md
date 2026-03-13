# PLANS

Current task execution plan.
Refs: `docs/codex/testing.md`, `docs/codex/checklists/pr-definition-of-done.md`.

## Task

- Name: asset-token-key wallet-api batch2
- Goal:
  - 将 `wallet-api` 的 ACCT_CHANGE 资产同步主链路切到 `AssetTokenKey`
  - 覆盖 `InnerEvent -> AssetsDomain -> ApiAssetsDomain`
  - 保留手动 `sync_assets_by_wallet(..., symbol)` 兼容语义，不扩到协议层

## Batch Scope

### In

- `wallet-api/src/infrastructure/inner_event.rs`
- `wallet-api/src/domain/assets/mod.rs`
- `wallet-api/src/domain/api_wallet/assets.rs`
- 这条 flow 上被迫联动的最小调用点
- `PLANS.md`

### Out

- request / response / MQTT / API 参数全量切换
- `wallet-api` 其他交易、通知、聚合 VO 的 token 类型清理
- 数据库 schema 变更

## Constraints

- 本轮只改一个模块一条 flow：ACCT_CHANGE 驱动的资产同步
- 手动 `sync_assets_by_wallet` 继续按 `symbol` 语义
- 不在这轮清理全部 `Option<String>` 边界类型

## Plan

1. 将 `SyncAssetsData`、`AssetKey`、批量分组键切到 `AssetTokenKey`
2. 将普通钱包 `AssetsDomain` 的事件同步路径切到 `AssetTokenKey`，保留手动 symbol 模式
3. 将 `ApiAssetsDomain` 的同步和重试路径切到 `AssetTokenKey`
4. 补最小单测，验证 `Native/Contract` 命中与手动兼容语义

## Validation Commands

- `cargo test -p wallet-api --lib normal_assets_manual_sync_keeps_symbol_filter_when_token_missing -- --nocapture`
- `cargo test -p wallet-api --lib normal_assets_sync_filter_matches_by_token_when_symbol_differs -- --nocapture`
- `cargo test -p wallet-api --lib api_wallet_sync_filter_matches_native_and_contract_token_keys -- --nocapture`
- `cargo test -p wallet-api --test mod acct_change_normal_wallet_syncs_by_token_when_symbol_mismatch -- --nocapture`
- `cargo test -p wallet-api --test mod acct_change_syncs_sol_usdc_with_symbol_mismatch_by_token_address -- --nocapture`

## Stop Condition

- ACCT_CHANGE 资产同步主链路只依赖 `chain_code + AssetTokenKey`
- `wallet-api` 业务层不再在这条链路里手写 `None` / `""` 归一化
- 手动 symbol 同步入口仍保持兼容

## Progress Checklist

- [x] Update plan for this batch
- [x] Switch `InnerEvent` sync key to `AssetTokenKey`
- [x] Switch normal wallet asset sync flow to `AssetTokenKey`
- [x] Switch API wallet asset sync flow to `AssetTokenKey`
- [x] Add focused wallet-api regressions
- [x] Run focused validation

## Validation Notes

- Passed:
  - `cargo test -p wallet-api --lib normal_assets_sync_filter_matches_by_token_when_symbol_differs -- --nocapture`
  - `cargo test -p wallet-api --lib normal_assets_manual_sync_keeps_symbol_filter_when_token_missing -- --nocapture`
  - `cargo test -p wallet-api --lib api_wallet_acct_change_syncs_native_asset_by_empty_token_without_symbol_matching -- --nocapture`
  - `cargo test -p wallet-api --lib api_wallet_acct_change_does_not_sync_other_assets_with_different_token_address -- --nocapture`
  - `cargo test -p wallet-api --test mod acct_change_syncs_sol_usdc_with_symbol_mismatch_by_token_address -- --nocapture`
  - `cargo test -p wallet-api --test mod acct_change_normal_wallet_syncs_by_token_when_symbol_mismatch -- --nocapture`
  - `cargo fmt --all`

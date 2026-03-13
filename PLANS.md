# PLANS

Current task execution plan.
Refs: `docs/codex/testing.md`, `docs/codex/checklists/pr-definition-of-done.md`.

## Task

- Name: asset-token-key wallet-database assets-entity batch
- Goal:
  - 将 `wallet-database` 的 `AssetsEntity/ApiAssetsEntity` 及关联实体的 `token_address` 字段类型化为 `AssetTokenKey`
  - 减少实体层 `String`/`Option<String>` token 混用，统一由 `AssetTokenKey` 承载主币/合约币语义
  - 保持 `sync_assets_by_wallet(wallet_address, account_id, symbol)` 兼容语义不变（不改接口签名）

## Batch Scope

### In

- `wallet-database/src/entities/assets.rs`
- `wallet-database/src/entities/api_assets.rs`
- `wallet-database/src/dao/assets.rs`
- `wallet-database/src/dao/api_assets.rs`
- `wallet-database/src/repositories/assets.rs`
- `wallet-api` 中受该实体类型变化影响的最小编译修复点
- `PLANS.md`

### Out

- 普通钱包/Api 钱包 ACCT_CHANGE 语义变更（本轮只做类型收敛，不改行为）
- `wallet-api` 大规模重构（仅修必要编译断点）
- 数据库 schema 迁移

## Constraints

- 本轮主改一个模块：`wallet-database` 资产实体流
- 遵守最小联动原则；`wallet-api` 仅做适配，不引入新接口
- 保持现有手动接口 `sync_assets_by_wallet` 签名与行为不变

## Plan

1. 将 `AssetsEntity/ApiAssetsEntity/WithAddressType` 的 `token_address` 字段改为 `AssetTokenKey`
2. DAO/Repo 层 bind/query 继续沿用 DB TEXT 兼容语义，移除实体层重复 `from_db_value` 转换
3. 修复 `wallet-api` 受影响调用点，统一使用 `as_db_str()`/`to_option_string_for_api()`
4. 运行 `wallet-database` + `wallet-api` 最小验证，确认无行为回退

## Validation Commands

- `cargo test -p wallet-database assets -- --nocapture`
- `cargo check -p wallet-api --message-format short`
- `cargo test -p wallet-api --test mod acct_change_normal_wallet_syncs_by_token_when_symbol_mismatch -- --nocapture`
- `cargo test -p wallet-api --test mod acct_change_normal_wallet_syncs_native_by_empty_token_when_token_missing -- --nocapture`
- `cargo test -p wallet-api --test mod acct_change_syncs_sol_usdc_with_symbol_mismatch_by_token_address -- --nocapture`

## Stop Condition

- `wallet-database` 资产实体主路径的 `token_address` 已类型化为 `AssetTokenKey`
- `wallet-api` 在此变更下可通过最小编译
- 普通钱包与 API 钱包已存在的 symbol mismatch 回归继续通过

## Progress Checklist

- [x] Update plan for this batch
- [x] Type `assets/api_assets` entity token fields to `AssetTokenKey`
- [x] Apply minimal wallet-api compatibility fixes
- [x] Run focused validation commands

## Validation Notes

- 已通过（上一批）:
  - `cargo check -p wallet-api --message-format short`
  - `cargo test -p wallet-api --lib api_wallet_acct_change_syncs_sol_usdc_by_token_address_when_symbol_differs -- --nocapture`
  - `cargo test -p wallet-api --lib api_wallet_acct_change_syncs_native_asset_by_empty_token_without_symbol_matching -- --nocapture`
  - `cargo test -p wallet-api --lib api_wallet_acct_change_does_not_sync_other_assets_with_different_token_address -- --nocapture`
  - `cargo test -p wallet-api --test mod acct_change_normal_wallet_syncs_by_token_when_symbol_mismatch -- --nocapture`
  - `cargo test -p wallet-api --test mod acct_change_normal_wallet_syncs_native_by_empty_token_when_token_missing -- --nocapture`
  - `cargo test -p wallet-api --test mod acct_change_syncs_sol_usdc_with_symbol_mismatch_by_token_address -- --nocapture`
- 已通过（本批）:
  - `cargo test -p wallet-database assets -- --nocapture`
  - `cargo check -p wallet-database --message-format short`
  - `cargo check -p wallet-api --message-format short`
  - `cargo test -p wallet-api --test mod acct_change_normal_wallet_syncs_by_token_when_symbol_mismatch -- --nocapture`
  - `cargo test -p wallet-api --test mod acct_change_normal_wallet_syncs_native_by_empty_token_when_token_missing -- --nocapture`
  - `cargo test -p wallet-api --test mod acct_change_syncs_sol_usdc_with_symbol_mismatch_by_token_address -- --nocapture`

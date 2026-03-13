# PLANS

Current task execution plan.
Refs: `docs/codex/testing.md`, `docs/codex/checklists/pr-definition-of-done.md`.

## Task

- Name: asset-token-key typed foundation
- Goal:
  - 引入 `AssetTokenKey` 统一表达主币/合约币
  - 先在 `wallet-database` 的身份键类型落地，避免继续在键值层混用 `None` / `""`
  - 保持现有 DB schema 和 `wallet-api` 业务行为兼容

## Batch Scope

### In

- `wallet-database/src/entities/*` 中与资产/币身份键直接相关的类型
- `wallet-database/src/dao/*` 中消费 `AssetsId` / `CoinId` 的读写路径
- `wallet-database/src/repositories/*` 的最小兼容适配
- `PLANS.md`

### Out

- `wallet-api` domain/orchestration 全量切换到 `AssetTokenKey`
- `AssetsEntity` / `CoinEntity` 全字段类型迁移
- request / response / MQTT 边界清理

## Constraints

- 本轮必须控制在“基础类型 + 身份键”子批次，避免跨两个 crate 大面积重构
- 数据库 schema 不变，SQLite 仍以 `TEXT` 存储 token
- 旧接口语义必须保持兼容

## Plan

1. 新增 `AssetTokenKey`，实现 `sqlx`/`serde` 适配以及 `from_raw`、`as_db_str` 等核心 helper
2. 将 `AssetsId` / `AssetsIdVo` / `CoinId` 迁移到 `AssetTokenKey`，并为现有调用点保留兼容构造方式
3. 在资产/币实体上新增 `token_key()` 过渡方法，DAO/Repo 改用新键类型做 bind 或查询
4. 补 `wallet-database` 最小回归：主币、合约币、空白串归一化与 ID 读写一致性

## Validation Commands

- `cargo test -p wallet-database asset_token_key -- --nocapture`
- `cargo test -p wallet-database assets_upsert_update_and_query_consistent -- --nocapture`
- `cargo test -p wallet-database coin_repo_upsert_and_get_success -- --nocapture`
- `cargo test -p wallet-api --lib normal_assets_manual_sync_keeps_symbol_filter_when_token_missing -- --nocapture`
- `cargo test -p wallet-api --test mod acct_change_normal_wallet_syncs_by_token_when_symbol_mismatch -- --nocapture`
- `cargo test -p wallet-api --test mod acct_change_syncs_sol_usdc_with_symbol_mismatch_by_token_address -- --nocapture`

## Stop Condition

- `AssetTokenKey` 已进入 `wallet-database` 身份键主路径
- 现有 DB 读写保持兼容
- 不扩展到 `wallet-api` 业务字段全量迁移

## Progress Checklist

- [x] Update plan for this batch
- [x] Add `AssetTokenKey` foundation
- [x] Migrate DB identity types
- [x] Add focused database regressions
- [x] Run focused validation

## Validation Notes

- Passed:
  - `cargo test -p wallet-database asset_token_key -- --nocapture`
  - `cargo test -p wallet-database assets_upsert_update_and_query_consistent -- --nocapture`
  - `cargo test -p wallet-database coin_repo_upsert_and_get_success -- --nocapture`
  - `cargo test -p wallet-api --lib normal_assets_manual_sync_keeps_symbol_filter_when_token_missing -- --nocapture`
- Blocked by existing test DB migration state, not this batch's type changes:
  - `cargo test -p wallet-api --test mod acct_change_normal_wallet_syncs_by_token_when_symbol_mismatch -- --nocapture`
  - `cargo test -p wallet-api --test mod acct_change_syncs_sol_usdc_with_symbol_mismatch_by_token_address -- --nocapture`
  - failure: `migration 20250829094146 was previously applied but is missing in the resolved migrations`

# PLANS

Current task execution plan.
Refs: `docs/codex/testing.md`, `docs/codex/checklists/pr-definition-of-done.md`.

## Task

- Name: asset-token-key strict convergence (entity accessor cleanup batch)
- Goal:
  - 移除 `wallet-database` 资产实体 `token_address() -> Option<String>` 兼容访问器
  - `wallet-api` 侧显式使用 `AssetTokenKey`（`token_key()` / `as_db_str()`）做边界转换
  - 保持 `sync_assets_by_wallet(wallet_address, account_id, symbol)` 兼容语义不变（不改接口签名）

## Batch Scope

### In

- `wallet-database/src/entities/assets.rs`
- `wallet-database/src/entities/api_assets.rs`
- `wallet-api` 中直接调用上述实体 `token_address()` 的最小联动修复
- `PLANS.md`

### Out

- 普通钱包/Api 钱包 ACCT_CHANGE 语义变更（本轮不改行为）
- `wallet-api` 大规模重构（仅修必要编译断点）
- 数据库 schema 迁移

## Constraints

- 本轮主改一个模块：`wallet-database` 资产实体访问器收敛
- 遵守最小联动原则；`wallet-api` 仅做边界适配，不引入新接口
- 保持现有手动接口 `sync_assets_by_wallet` 签名与行为不变

## Plan

1. 移除 `AssetsEntity/ApiAssetsEntity` 及 `WithAddressType` 的 `token_address()` 兼容方法
2. 修复 `wallet-api` 直接调用点，按场景改为：
   - 需要 `String`：`token_key().as_db_str().to_string()`
   - 需要 `Option<String>`（协议边界）：`token_key().to_option_string_for_api()`
3. 运行 `wallet-api` 最小验证，确认编译和关键回归无回退

## Validation Commands

- `cargo check -p wallet-database --message-format short`
- `cargo check -p wallet-api --message-format short`
- `cargo test -p wallet-api --test mod acct_change_normal_wallet_syncs_by_token_when_symbol_mismatch -- --nocapture`
- `cargo test -p wallet-api --test mod acct_change_normal_wallet_syncs_native_by_empty_token_when_token_missing -- --nocapture`
- `cargo test -p wallet-api --test mod acct_change_syncs_sol_usdc_with_symbol_mismatch_by_token_address -- --nocapture`

## Stop Condition

- `wallet-database` 资产实体不再暴露 `token_address() -> Option<String>` 兼容访问器
- `wallet-api` 在此变更下可通过最小编译
- 普通钱包与 API 钱包已存在的 symbol mismatch 回归继续通过

## Progress Checklist

- [x] Update plan for this batch
- [x] Remove entity Option-style token accessors
- [x] Apply minimal wallet-api compatibility fixes
- [x] Run focused validation commands

## Additional Convergence (This Round)

- 将 `CoinData::new` / `ApiCoinData::new` / `CoinId::new` 的 `token_address` 参数从 `Option<String>` 收敛为 `AssetTokenKey`
- 调用点改为显式 `AssetTokenKey` 构造（`.into()` 或 `AssetTokenKey::from_raw(...)`）
- 修复 `wallet-api/src/response_vo/standard_wallet/coin.rs` 中因移除实体访问器导致的 `token_address()` 递归实现
- 合并 `AssetsId` / `AssetsIdVo`：删除 `AssetsIdVo`，统一 `ApiAssetsDao::assets_by_id` / `ApiAssetsRepo::find_by_id` 使用 `AssetsId`
- `AssetsId::new` 入参改为 `AssetTokenKey` 后，修复普通钱包与 API 钱包资产路径及回归测试调用点

## Validation Notes

- 已通过（上一批）:
  - `cargo check -p wallet-api --message-format short`
  - `cargo test -p wallet-api --lib api_wallet_acct_change_syncs_sol_usdc_by_token_address_when_symbol_differs -- --nocapture`
  - `cargo test -p wallet-api --lib api_wallet_acct_change_syncs_native_asset_by_empty_token_without_symbol_matching -- --nocapture`
  - `cargo test -p wallet-api --lib api_wallet_acct_change_does_not_sync_other_assets_with_different_token_address -- --nocapture`
  - `cargo test -p wallet-api --test mod acct_change_normal_wallet_syncs_by_token_when_symbol_mismatch -- --nocapture`
  - `cargo test -p wallet-api --test mod acct_change_normal_wallet_syncs_native_by_empty_token_when_token_missing -- --nocapture`
  - `cargo test -p wallet-api --test mod acct_change_syncs_sol_usdc_with_symbol_mismatch_by_token_address -- --nocapture`
- 已通过（本轮新增）:
  - `cargo check -p wallet-api --message-format short`
  - `cargo test -p wallet-api --test mod acct_change_normal_wallet_syncs_by_token_when_symbol_mismatch -- --nocapture`
  - `cargo test -p wallet-api --test mod acct_change_normal_wallet_syncs_native_by_empty_token_when_token_missing -- --nocapture`
  - `cargo test -p wallet-api --test mod acct_change_syncs_sol_usdc_with_symbol_mismatch_by_token_address -- --nocapture`

---

## Task

- Name: remove symbol from `AssetsId` and keep symbol only in create payloads
- Goal:
  - `AssetsId` 只表达资产定位键（`address + chain_code + token_address`）
  - `CreateAssetsVo` / `ApiCreateAssetsVo` 显式携带 `symbol` 用于 insert/upsert
  - 不改变 `sync_assets_by_wallet(wallet_address, account_id, symbol)` 对外兼容语义

## Batch Scope

### In

- `wallet-database/src/entities/assets.rs`
- `wallet-database/src/entities/api_assets.rs`
- `wallet-database/src/dao/assets.rs`
- `wallet-database/src/dao/api_assets.rs`
- `wallet-api` 中 `AssetsId::new` / 结构体字面量及 `CreateAssetsVo::new` / `ApiCreateAssetsVo::new` 最小联动修复

### Out

- schema migration（本轮不改表结构/索引）
- 普通钱包手动同步接口语义改造
- 非资产主路径的 symbol 业务语义调整

## Plan

1. `AssetsId` 移除 `symbol` 字段与构造参数，统一 key 仅保留 `address/chain_code/token_address`
2. `CreateAssetsVo` / `ApiCreateAssetsVo` 新增 `symbol` 字段并改 `new(...)` 入参
3. DAO 查询与更新条件改为 key-only；insert/upsert 从 create vo 读 symbol
4. 修复 `wallet-api` 调用点与测试构造，保持行为不变

## Validation Commands

- `cargo check -p wallet-database --message-format short`
- `cargo check -p wallet-api --message-format short`
- `cargo test -p wallet-api --test mod acct_change_normal_wallet_syncs_by_token_when_symbol_mismatch -- --nocapture`
- `cargo test -p wallet-api --test mod acct_change_normal_wallet_syncs_native_by_empty_token_when_token_missing -- --nocapture`
- `cargo test -p wallet-api --test mod acct_change_syncs_sol_usdc_with_symbol_mismatch_by_token_address -- --nocapture`

## Stop Condition

- `AssetsId` 不再包含 `symbol`
- `CreateAssetsVo` / `ApiCreateAssetsVo` 承担 symbol 写入
- `wallet-api` 最小编译与关键账变回归通过

## Progress

- [x] `AssetsId` 去除 `symbol` 字段与构造参数
- [x] `CreateAssetsVo` / `ApiCreateAssetsVo` 增加 `symbol` 字段并切换 `new(...)` 签名
- [x] `wallet-database` DAO 读写 key 改为 `address + chain_code + token_address`
- [x] `wallet-api` 调用点与测试构造联动修复

## Validation Notes

- 通过:
  - `cargo check -p wallet-database --message-format short`
  - `cargo check -p wallet-api --message-format short`
  - `cargo test -p wallet-api --test mod acct_change_normal_wallet_syncs_by_token_when_symbol_mismatch -- --nocapture`
  - `cargo test -p wallet-api --test mod acct_change_normal_wallet_syncs_native_by_empty_token_when_token_missing -- --nocapture`
  - `cargo test -p wallet-api --test mod acct_change_syncs_sol_usdc_with_symbol_mismatch_by_token_address -- --nocapture`

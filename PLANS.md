# PLANS

Current task execution plan.
Refs: `docs/codex/testing.md`, `docs/codex/checklists/pr-definition-of-done.md`.

## Task

- Name: asset-token-key wallet-database batch1
- Goal:
  - 在 `wallet-database` 完成 `AssetTokenKey` 严格收敛第一批
  - 覆盖币实体主路径：`CoinData/CoinEntity`、`ApiCoinData/ApiCoinEntity`
  - 移除 `AssetTokenKey` 的 Option 风格方法并替换数据库层调用点

## Batch Scope

### In

- `wallet-database/src/entities/coin.rs`
- `wallet-database/src/entities/api_coin.rs`
- `wallet-database/src/dao/coin.rs`
- `wallet-database/src/dao/api_coin.rs`
- `wallet-database/src/repositories/coin.rs`
- `wallet-database/src/repositories/api_wallet/coin.rs`
- `wallet-database/src/entities/assets.rs`
- `wallet-database/src/entities/api_assets.rs`
- `wallet-database/src/dao/assets.rs`
- `wallet-database/src/dao/api_assets.rs`
- `wallet-database/src/repositories/assets.rs`
- 受影响的 `wallet-database` 单测
- `PLANS.md`

### Out

- `wallet-api` 业务层与事件流改造（已在上一批完成主路径）
- `api_collect/api_withdraw/multisig_queue` 等非资产主链路实体 token 字段改造
- 数据库 schema 变更

## Constraints

- 本轮只改一个 crate：`wallet-database`
- 不改数据库 schema（`token_address` 仍为 `TEXT`，主币存 `""`）
- 允许保留边界构造函数 `new(..., Option<String>)`，但实体字段统一为 `AssetTokenKey`

## Plan

1. 将 `CoinData/CoinEntity/ApiCoinData/ApiCoinEntity` 的 token 字段提升为 `AssetTokenKey`
2. DAO 与 Repo 的 coin/api_coin 主路径统一绑定 `AssetTokenKey`
3. 删除 `AssetTokenKey` 的 Option 风格方法并替换 `dao/assets`、`dao/api_assets` 等内部调用
4. 跑最小数据库回归，确认主币与合约币路径不回归

## Validation Commands

- `cargo test -p wallet-database coin -- --nocapture`
- `cargo test -p wallet-database api_wallet::coin -- --nocapture`
- `cargo test -p wallet-database assets -- --nocapture`

## Stop Condition

- `wallet-database` 的 coin/api_coin 主实体不再用 `Option<String>` 表达 token 身份
- `AssetTokenKey` 不再提供 `as_deref/is_some/is_none/as_ref/unwrap/unwrap_or_default`
- `wallet-database` 目标测试全部通过，且 DB 读写保持 `Native <-> ""` 兼容

## Progress Checklist

- [x] Update plan for this batch
- [x] Switch `Coin` / `ApiCoin` entities to `AssetTokenKey`
- [x] Update dao/repo signatures with compatibility bridge
- [x] Add/adjust focused wallet-database tests
- [x] Run focused validation

## Validation Notes

- `cargo test -p wallet-database coin -- --nocapture` ✅
- `cargo test -p wallet-database api_wallet::coin -- --nocapture` ✅
- `cargo test -p wallet-database assets -- --nocapture` ✅

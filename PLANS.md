# PLANS

Current task execution plan.
Refs: `docs/codex/testing.md`, `docs/codex/checklists/pr-definition-of-done.md`.

## Task

- Name: asset-token-key wallet-database batch1
- Goal:
  - 在 `wallet-database` 完成 `AssetTokenKey` 的实体层落地收口
  - 优先覆盖资产/币核心实体：`CoinEntity/CoinId`、`ApiCoinEntity`
  - 保持对 `wallet-api` 的现有调用兼容（过渡期保留 Option 入参 helper）

## Batch Scope

### In

- `wallet-database/src/entities/coin.rs`
- `wallet-database/src/entities/api_coin.rs`
- `wallet-database/src/dao/coin.rs`
- `wallet-database/src/dao/api_coin.rs`
- `wallet-database/src/repositories/coin.rs`
- `wallet-database/src/repositories/api_wallet/coin.rs`
- 受影响的 `wallet-database` 单测
- `PLANS.md`

### Out

- `wallet-api` 业务层与事件流改造（已在上一批完成主路径）
- `api_collect/api_withdraw/multisig_queue` 等非资产主链路实体 token 字段改造
- 数据库 schema 变更

## Constraints

- 本轮只改一个 crate：`wallet-database`
- 不做破坏性外部接口调整：允许保留 `Option<String>` 过渡构造函数
- 不触发跨 crate 的大规模调用点联动

## Plan

1. 将 `CoinId/CoinEntity` 与 `ApiCoinEntity` 的 token 字段提升为 `AssetTokenKey`
2. DAO 与 Repo 对应查询/绑定路径支持 `AssetTokenKey` 入参
3. 保留过渡 helper（`new(... Option<String>)` / `token_address()`）以兼容上层
4. 补最小数据库层单测验证 `Native <-> ""` 与 `Contract(addr)` 的读写命中

## Validation Commands

- `cargo test -p wallet-database coin -- --nocapture`
- `cargo test -p wallet-database api_wallet::coin -- --nocapture`
- `cargo test -p wallet-database assets -- --nocapture`

## Stop Condition

- `wallet-database` 的 `Coin` / `ApiCoin` 主实体不再用 `Option<String>` 表达 token 身份
- DAO/Repo 主路径可直接接收 `AssetTokenKey`
- 上层 crate 在不修改调用点情况下仍可编译通过

## Progress Checklist

- [x] Update plan for this batch
- [ ] Switch `Coin` / `ApiCoin` entities to `AssetTokenKey`
- [ ] Update dao/repo signatures with compatibility bridge
- [ ] Add/adjust focused wallet-database tests
- [ ] Run focused validation

## Validation Notes

- Pending for this batch

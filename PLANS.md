# PLANS

Current task execution plan.  
Refs: `docs/codex/testing.md`, `docs/codex/workflows.md`.

## Task

- Name: thin ResourcesRepo via RepoCtx (prep for deletion)
- Goal:
  - 抽出 `RepoCtx` 承载 `pool + transaction`，让 `ResourcesRepo` 变薄适配层
  - 不改业务 DAO 语义，不改上层调用签名
  - 为后续删除 `ResourcesRepo` 做低风险前置拆分

## Scope

### In

- `wallet-database/src/repositories/mod.rs`
- 受影响最小测试/编译验证
- `PLANS.md`

### Out

- SQLite 连接池策略和并发锁治理实现
- `sql_utils` 二次重构
- 其他 repository/DAO 的顺手改造
- `ResourcesRepo` 删除（下一批）

## Constraints

- Keep business semantics unchanged
- Test-first for touched flow boundaries
- Offline validation only
- Small, reversible patch set

## Plan

1. Introduce `RepoCtx` in `repositories/mod.rs` with current `db_pool + transaction`
2. Refactor `ResourcesRepo` to hold `ctx: RepoCtx` and delegate transaction/pool access
3. Keep `TransactionTrait` behavior identical while switching internals to `RepoCtx`
4. Run offline checks for `wallet-database` and `wallet-api`

## Validation Commands

- `cargo check -p wallet-database --offline`
- `cargo check -p wallet-api --offline`

## Expected Results

- `ResourcesRepo` 不再直接持有 transaction 状态
- `RepoCtx` 成为后续仓储拆分的统一状态载体
- `wallet-database` 与 `wallet-api` 离线编译通过

## Progress Checklist

- [x] Introduce `RepoCtx`
- [x] Move `ResourcesRepo` internals to `RepoCtx`
- [x] Migrate `bill/multisig*/address_book/stake` repo holders to `RepoCtx`
- [x] Run focused offline validation

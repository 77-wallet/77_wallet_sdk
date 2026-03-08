# PLANS

Current task execution plan.
Refs: `docs/codex/testing.md`, `docs/codex/workflows.md`.

## Task

- Name: repoctx decoupling (batch 16: CoinService stateless method extraction)
- Goal:
  - 将 `CoinService` 中不依赖 `RepoCtx` 的方法改为静态方法
  - 替换对应调用点，减少 `resource_repo()` 使用
  - 不改依赖 `RepoCtx` 的热路径（`get_hot_coin_list/pull_hot_coins/query_token_info/customize_coin`）

## Scope

### In

- `wallet-api/src/service/coin.rs`
- `wallet-api/src/api/coin.rs`
- `wallet-api/src/infrastructure/task_queue/common.rs`
- `wallet-api/src/infrastructure/task_queue/initialization.rs`
- `wallet-api/src/infrastructure/task_queue/task_handle/backend_handle.rs`
- `PLANS.md`

### Out

- `CoinService` 结构体字段移除
- `RepoCtx` 事务路径重构
- `wallet-database` 仓库大改

## Constraints

- Keep behavior unchanged
- Batch-by-batch compile validation
- Offline validation only

## Plan

1. Convert stateless coin methods to associated fns (`fn foo(...)`) without `self`
2. Update coin API and task queue call sites to static invocations
3. Run offline checks for `wallet-database` and `wallet-api`

## Validation Commands

- `cargo check -p wallet-database --offline`
- `cargo check -p wallet-api --offline`

## Progress Checklist

- [x] Convert CoinService stateless methods
- [x] Adapt API/task queue call sites
- [x] Run focused offline validation

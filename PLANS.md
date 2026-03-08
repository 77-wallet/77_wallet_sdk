# PLANS

Current task execution plan.
Refs: `docs/codex/testing.md`, `docs/codex/workflows.md`.

## Task

- Name: repoctx decoupling (batch 19: CoinService remaining api path cleanup)
- Goal:
  - 清理 `api/coin` 中剩余对 `CoinService::new(resource_repo())` 的依赖
  - 将 `get_hot_coin_list` / `customize_coin` 迁移为静态入口（内部最小 RepoCtx）
  - 保持业务语义不变

## Scope

### In

- `wallet-api/src/service/coin.rs`
- `wallet-api/src/api/coin.rs`
- `PLANS.md`

### Out

- `CoinService` 完整结构删除
- `RepoCtx` 在 `customize_coin/get_hot_coin_list` 内部移除
- 事务语义和 DAO 层改造

## Constraints

- Keep behavior unchanged
- Batch-by-batch compile validation
- Offline validation only

## Plan

1. Convert `get_hot_coin_list` and `customize_coin` to static methods
2. Replace remaining `api/coin` call sites that used `CoinService::new(resource_repo())`
3. Keep internal logic unchanged by creating minimal `RepoCtx` inside methods
4. Run offline checks for `wallet-database` and `wallet-api`

## Validation Commands

- `cargo check -p wallet-database --offline`
- `cargo check -p wallet-api --offline`

## Progress Checklist

- [x] Convert get_hot_coin_list/customize_coin to static methods
- [x] Replace remaining api/coin resource_repo call sites
- [x] Keep behavior unchanged with internal RepoCtx
- [x] Run focused offline validation

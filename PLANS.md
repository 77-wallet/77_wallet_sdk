# PLANS

Current task execution plan.
Refs: `docs/codex/testing.md`, `docs/codex/workflows.md`.

## Task

- Name: repoctx decoupling (batch 18: pull_hot_coins static migration)
- Goal:
  - 将 `CoinService::pull_hot_coins` 从 `RepoCtx` 路径迁移为 `CoreDbPool` 静态路径
  - 在 `wallet-database` / `CoinDomain` 增加最小静态 helper 支撑迁移
  - 保持行为不变

## Scope

### In

- `wallet-database/src/repositories/coin.rs`
- `wallet-api/src/domain/coin/mod.rs`
- `wallet-api/src/service/coin.rs`
- `wallet-api/src/api/coin.rs`
- `wallet-api/src/infrastructure/task_queue/initialization.rs`
- `PLANS.md`

### Out

- `customize_coin/get_hot_coin_list` 去 `RepoCtx`
- `CoinService` 结构体移除 `repo`
- 事务模型重构

## Constraints

- Keep behavior unchanged
- Batch-by-batch compile validation
- Offline validation only

## Plan

1. Add static `CoinRepo` helpers for `drop_null_token` and `upsert_multi_coin`
2. Add `CoinDomain::upsert_hot_coin_list_with_pool` while keeping old RepoCtx variant
3. Migrate `CoinService::pull_hot_coins` to static path and update call sites
4. Run offline checks for `wallet-database` and `wallet-api`

## Validation Commands

- `cargo check -p wallet-database --offline`
- `cargo check -p wallet-api --offline`

## Progress Checklist

- [x] Add static CoinRepo helpers
- [x] Add CoinDomain pool-based upsert helper
- [x] Migrate pull_hot_coins and call sites
- [x] Run focused offline validation

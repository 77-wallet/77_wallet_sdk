# PLANS

Current task execution plan.
Refs: `docs/codex/testing.md`, `docs/codex/workflows.md`.

## Task

- Name: repoctx decoupling (batch 17: CoinService query/token path cleanup)
- Goal:
  - 将 `query_token_info` 从 `RepoCtx` 读取路径迁移为 `CoreDbPool + CoinRepo` 静态路径
  - 将 `delete_wsol_error` 调整为静态 helper，作为 `pull_hot_coins` 后续解耦铺垫
  - 保持行为不变并确保编译闭环

## Scope

### In

- `wallet-database/src/repositories/coin.rs`
- `wallet-api/src/service/coin.rs`
- `wallet-api/src/api/coin.rs`
- `PLANS.md`

### Out

- `pull_hot_coins` 事务主链重构
- `customize_coin` / `get_hot_coin_list` 去 `RepoCtx`
- `CoinService` 结构体字段移除

## Constraints

- Keep behavior unchanged
- Batch-by-batch compile validation
- Offline validation only

## Plan

1. Add optional coin lookup helper by chain+token in `CoinRepo`
2. Migrate `CoinService::query_token_info` to static pool-based read path
3. Make `delete_wsol_error` static and keep `pull_hot_coins` behavior unchanged
4. Run offline checks for `wallet-database` and `wallet-api`

## Validation Commands

- `cargo check -p wallet-database --offline`
- `cargo check -p wallet-api --offline`

## Progress Checklist

- [x] Add optional coin lookup helper
- [x] Migrate query_token_info static path
- [x] Make delete_wsol_error static helper
- [x] Run focused offline validation

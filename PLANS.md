# PLANS

Current task execution plan.
Refs: `docs/codex/testing.md`, `docs/codex/workflows.md`.

## Task

- Name: repoctx decoupling (batch 30: remove local RepoCtx in coin service)
- Goal:
  - 移除 `wallet-api/src/service/coin.rs` 中局部临时 `RepoCtx` 用法
  - 统一改为 `CoreDbPool + CoinRepo/AssetsEntity` 直接调用
  - 保持行为不变，不触碰事务语义

## Scope

### In

- `wallet-api/src/service/coin.rs`
- `PLANS.md`

### Out

- `RepoCtx` 结构变更
- DAO/SQL/事务变更
- service 结构体签名变更
- coin 业务逻辑变更

## Constraints

- Keep behavior unchanged
- Batch-by-batch compile validation
- Offline validation only

## Plan

1. Replace local `RepoCtx` use in `CoinService::get_hot_coin_list`
2. Replace local `RepoCtx` use in `CoinService::delete_wsol_error` and `CoinService::customize_coin`
3. Keep logic unchanged and run offline checks for `wallet-database` and `wallet-api`

## Validation Commands

- `cargo check -p wallet-database --offline`
- `cargo check -p wallet-api --offline`

## Progress Checklist

- [x] Remove local RepoCtx usage in coin service
- [x] Keep behavior unchanged
- [x] Run focused offline validation

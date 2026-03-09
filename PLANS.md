# PLANS

Current task execution plan.
Refs: `docs/codex/testing.md`, `docs/codex/workflows.md`.

## Task

- Name: repoctx decoupling (batch 35: remove RepoCtx write usage in add_coin_v2)
- Goal:
  - 将 `AssetsService::add_coin_v2` 中 `tx.upsert_assets` 替换为 `AssetsEntity::upsert_assets`
  - 不改业务流程，不引入事务语义变化
  - 继续减少 `AssetsService` 对 `RepoCtx` 的依赖面

## Scope

### In

- `wallet-api/src/service/asset.rs`
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

1. Replace `add_coin_v2` `tx.upsert_assets` with `AssetsEntity::upsert_assets`
2. Keep other methods unchanged
3. Run offline checks for `wallet-database` and `wallet-api`

## Validation Commands

- `cargo check -p wallet-database --offline`
- `cargo check -p wallet-api --offline`

## Progress Checklist

- [x] Remove RepoCtx write usage in add_coin_v2
- [x] Keep behavior unchanged
- [x] Run focused offline validation

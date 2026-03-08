# PLANS

Current task execution plan.
Refs: `docs/codex/testing.md`, `docs/codex/workflows.md`.

## Task

- Name: repoctx decoupling (batch 25: remove dead UnitOfWork)
- Goal:
  - 删除 `wallet-database::repositories::UnitOfWork` 的死代码
  - 保留 `RepoCtx` 与 `with_tx`，不改业务语义
  - 仅做结构收敛，不扩散到 service/domain 逻辑

## Scope

### In

- `wallet-database/src/repositories/mod.rs`
- `PLANS.md`

### Out

- `RepoCtx` 重构
- 其他业务流（coin/account/assets 等）
- DAO/SQL/事务变更

## Constraints

- Keep behavior unchanged
- Batch-by-batch compile validation
- Offline validation only

## Plan

1. Remove `UnitOfWork` struct and impl block from `repositories/mod.rs`
2. Keep `RepoCtx` and `with_tx` unchanged
3. Run offline checks for `wallet-database` and `wallet-api`

## Validation Commands

- `cargo check -p wallet-database --offline`
- `cargo check -p wallet-api --offline`

## Progress Checklist

- [x] Remove dead UnitOfWork
- [x] Keep behavior unchanged
- [x] Run focused offline validation

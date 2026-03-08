# PLANS

Current task execution plan.
Refs: `docs/codex/testing.md`, `docs/codex/workflows.md`.

## Task

- Name: repoctx decoupling (batch 23: trim dead RepositoryFactory APIs)
- Goal:
  - 清理 `wallet-database::factory::RepositoryFactory` 中无调用的实例化 API
  - 移除 `new/resource_repo/multisig_account_repo`
  - 保留静态入口 `RepositoryFactory::repo(...)`
  - 不修改 repository/dao/service 行为

## Scope

### In

- `wallet-database/src/factory.rs`
- `PLANS.md`

### Out

- `RepoCtx` 主体结构重构
- `wallet-api` 业务逻辑变更
- DAO/SQL/事务变更

## Constraints

- Keep behavior unchanged
- Batch-by-batch compile validation
- Offline validation only

## Plan

1. Remove unused instance fields/methods in `RepositoryFactory`
2. Keep `RepositoryFactory::repo` static constructor unchanged
3. Run offline checks for `wallet-database` and `wallet-api`

## Validation Commands

- `cargo check -p wallet-database --offline`
- `cargo check -p wallet-api --offline`

## Progress Checklist

- [x] Remove dead RepositoryFactory APIs
- [x] Keep behavior unchanged
- [x] Run focused offline validation

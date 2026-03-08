# PLANS

Current task execution plan.
Refs: `docs/codex/testing.md`, `docs/codex/workflows.md`.

## Task

- Name: repoctx decoupling (batch 20: api/asset resource_repo removal)
- Goal:
  - 仅在 `wallet-api/src/api/asset.rs` 去掉 `self.repo_factory.resource_repo()` 调用
  - 保持 `AssetsService` 与业务逻辑不变
  - 通过本地 helper 使用 `core_pool -> RepoCtx` 构造服务

## Scope

### In

- `wallet-api/src/api/asset.rs`
- `PLANS.md`

### Out

- `AssetsService` 内部重构
- `RepoCtx` 全面移除
- DAO/SQL/事务变更

## Constraints

- Keep behavior unchanged
- Batch-by-batch compile validation
- Offline validation only

## Plan

1. Add local helper in `api/asset.rs` to build `AssetsService` from `core_pool`
2. Replace all `AssetsService::new(self.repo_factory.resource_repo())` call sites in this file
3. Run offline checks for `wallet-database` and `wallet-api`

## Validation Commands

- `cargo check -p wallet-database --offline`
- `cargo check -p wallet-api --offline`

## Progress Checklist

- [x] Add helper and replace asset API call sites
- [x] Keep behavior unchanged
- [x] Run focused offline validation

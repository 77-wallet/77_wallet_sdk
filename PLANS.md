# PLANS

Current task execution plan.
Refs: `docs/codex/testing.md`, `docs/codex/workflows.md`.

## Task

- Name: repoctx decoupling (batch 24: remove UnitOfWork from announcement flow)
- Goal:
  - 在 `announcement` 单流里移除 `UnitOfWork` 依赖
  - `AnnouncementRepo` 改为直接持有 `CoreDbPool`
  - 同步 `wallet-api` 的 `announcement service/domain` 调用
  - 不扩散到其他 repository 模块

## Scope

### In

- `wallet-database/src/repositories/announcement.rs`
- `wallet-api/src/domain/announcement.rs`
- `wallet-api/src/service/announcement.rs`
- `PLANS.md`

### Out

- `RepoCtx/UnitOfWork` 全量删除
- 其他业务流（coin/account/assets 等）
- DAO/SQL/事务变更

## Constraints

- Keep behavior unchanged
- Batch-by-batch compile validation
- Offline validation only

## Plan

1. Refactor `AnnouncementRepo` to store `CoreDbPool` and call entities directly
2. Update `AnnouncementDomain` signature and call sites
3. Update `AnnouncementService` to instantiate repo from `core_pool`
4. Run offline checks for `wallet-database` and `wallet-api`

## Validation Commands

- `cargo check -p wallet-database --offline`
- `cargo check -p wallet-api --offline`

## Progress Checklist

- [x] Refactor announcement repo/service/domain wiring
- [x] Keep behavior unchanged
- [x] Run focused offline validation

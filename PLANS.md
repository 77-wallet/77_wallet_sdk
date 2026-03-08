# PLANS

Current task execution plan.
Refs: `docs/codex/testing.md`, `docs/codex/workflows.md`.

## Task

- Name: repoctx decoupling (batch 3: announcement path)
- Goal:
  - 将 `AnnouncementService` 从显式 `RepoCtx` 依赖中解耦
  - 保持业务行为不变（仅类型与调用路径收敛）
  - 继续复用“service 无状态 + typed pool”模式

## Scope

### In

- `wallet-api/src/service/announcement.rs`（移除 `RepoCtx` 字段/构造参数）
- `wallet-api/src/api/announcement.rs`（适配 `AnnouncementService::new()`）
- `wallet-api/src/messaging/mqtt/topics/bulletin_info.rs`（适配构造调用）
- `wallet-api/src/infrastructure/task_queue/initialization.rs`（适配构造调用）
- `PLANS.md`

### Out

- 其他 service/domain 的 `RepoCtx` 解耦
- DAO/SQL 语义与事务边界重构
- 锁治理/连接池策略调整

## Constraints

- Keep behavior unchanged
- Batch-by-batch compile validation
- Offline validation only

## Plan

1. Refactor `AnnouncementService` to zero-field service (`new()`), internally构造 `UnitOfWork`
2. Adapt announcement-related API and runtime call sites to new constructor
3. Keep transaction behavior unchanged (`add/read/delete` still explicit begin/commit)
4. Run offline checks for `wallet-api` and `wallet-database`

## Validation Commands

- `cargo check -p wallet-database --offline`
- `cargo check -p wallet-api --offline`
- `cargo test -p wallet-database system_notification_repo_list_and_detail_work_without_explicit_transaction --offline -- --nocapture`

## Progress Checklist

- [x] Decouple AnnouncementService from RepoCtx
- [x] Adapt announcement call sites
- [x] Keep existing transaction behavior
- [x] Run focused offline validation

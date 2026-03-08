# PLANS

Current task execution plan.
Refs: `docs/codex/testing.md`, `docs/codex/workflows.md`.

## Task

- Name: repoctx decoupling (batch 2: system notification path)
- Goal:
  - 继续以小批次方式将 `SystemNotificationService` 从显式 `RepoCtx` 依赖中解耦
  - 保持业务行为不变（仅类型与调用路径收敛）
  - 复用 batch 1 的“service 无状态 + typed pool + repository 静态方法”模式

## Scope

### In

- `wallet-database/src/repositories/system_notification.rs`（补静态接口）
- `wallet-api/src/service/system_notification.rs`（移除 `RepoCtx` 字段/构造参数）
- `wallet-api/src/api/system_notification.rs`（适配 `SystemNotificationService::new()`）
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

1. Add static `SystemNotificationRepo` methods for upsert/list/update/delete operations
2. Refactor `SystemNotificationService` to zero-field service (`new()`), only use typed pool
3. Adapt wallet-api API entry call sites to new constructor
4. Run offline checks for `wallet-database` and `wallet-api`

## Validation Commands

- `cargo check -p wallet-database --offline`
- `cargo check -p wallet-api --offline`
- `cargo test -p wallet-database system_notification_repo_list_and_detail_work_without_explicit_transaction --offline -- --nocapture`

## Progress Checklist

- [x] Add static system-notification repository APIs
- [x] Decouple SystemNotificationService from RepoCtx
- [x] Adapt API call sites
- [x] Run focused offline validation

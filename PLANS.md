# PLANS

Current task execution plan.
Refs: `docs/codex/testing.md`, `docs/codex/workflows.md`.

## Task

- Name: repoctx decoupling (batch 4: app service path)
- Goal:
  - 将 `AppService` 从显式 `RepoCtx` 依赖中解耦
  - 保持业务行为不变（仅类型与调用路径收敛）
  - 延续“service 无状态 + typed pool + repo 静态读接口”模式

## Scope

### In

- `wallet-api/src/service/app.rs`（移除 `RepoCtx` 字段/构造参数）
- `wallet-api/src/api/app.rs`（适配 `AppService::new()`）
- `wallet-database/src/repositories/announcement.rs`（补静态 unread 读接口）
- `wallet-database/src/repositories/system_notification.rs`（补静态 unread 读接口）
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

1. Add static unread-count APIs for announcement/system-notification repositories
2. Refactor `AppService` to zero-field service (`new()`), use typed pools directly
3. Adapt app API entry call sites to new constructor
4. Run offline checks for `wallet-api` and `wallet-database`

## Validation Commands

- `cargo check -p wallet-database --offline`
- `cargo check -p wallet-api --offline`
- `cargo test -p wallet-database system_notification_repo_list_and_detail_work_without_explicit_transaction --offline -- --nocapture`

## Progress Checklist

- [x] Add static unread-count repository APIs
- [x] Decouple AppService from RepoCtx
- [x] Adapt app call sites
- [x] Run focused offline validation

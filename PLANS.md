# PLANS

Current task execution plan.
Refs: `docs/codex/testing.md`, `docs/codex/workflows.md`.

## Task

- Name: repoctx decoupling (batch 1: task queue path)
- Goal:
  - 在不扩大改动面的前提下，先将 `TaskQueueService` 从显式 `RepoCtx` 依赖中解耦
  - 保持业务行为不变（仅类型与调用路径收敛）
  - 为后续服务层逐步移除 `RepoCtx` 建立可复用模式

## Scope

### In

- `wallet-database/src/repositories/bill.rs`（补静态读接口）
- `wallet-api/src/service/task_queue.rs`（移除 `RepoCtx` 字段/构造参数）
- `wallet-api/src/manager.rs`（适配 `TaskQueueService::new()`）
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

1. Add static `BillRepo::bill_count(&CoreDbPool)` to avoid requiring `RepoCtx` in task queue flow
2. Refactor `TaskQueueService` to zero-field service (`new()`), read all data from typed pools
3. Adapt `WalletManager::get_task_queue_status` call site
4. Run offline checks and a focused task-queue entry compile path validation

## Validation Commands

- `cargo check -p wallet-database --offline`
- `cargo check -p wallet-api --offline`
- `cargo test -p wallet-database system_notification_repo_list_and_detail_work_without_explicit_transaction --offline -- --nocapture`

## Progress Checklist

- [x] Add static bill-count repository API
- [x] Decouple TaskQueueService from RepoCtx
- [x] Adapt manager call site
- [x] Run focused offline validation

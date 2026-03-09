# PLANS

Current task execution plan.
Refs: `docs/codex/testing.md`, `docs/codex/workflows.md`.

## Task

- Name: layering cleanup (batch 57: remove task_queue dao alias usage in wallet-api)
- Goal:
  - 在 `wallet-api` 的 task queue flow 移除 `CreateTaskQueueDao::*` 直接调用
  - 统一通过 `TaskQueueRepo` 暴露构造入口生成 `CreateTaskQueueEntity`
  - 保持行为不变，仅收敛分层依赖

## Scope

### In

- `wallet-database/src/repositories/task_queue.rs`
- `wallet-api/src/infrastructure/task_queue/task/mod.rs`
- `PLANS.md`

### Out

- 其他 domain/service/messaging 模块
- repository/dao 结构性重构
- 事务模型变更

## Constraints

- Keep behavior unchanged
- Batch-by-batch compile validation
- Offline validation only

## Plan

1. 在 `TaskQueueRepo` 增加两个构造 helper：`build_backend_task` / `build_mqtt_task`
2. 将 `task/mod.rs` 中 `CreateTaskQueueDao::*` 替换为 `TaskQueueRepo` helper
3. 为新增 helper 补最小单元测试（成功路径 + 错误路径）
4. 运行最小离线验证并停止本轮

## Validation Commands

- `cargo test -p wallet-database task_queue_repo --offline -- --nocapture`
- `cargo check -p wallet-database --offline`
- `cargo check -p wallet-api --offline`

## Progress Checklist

- [x] Add `TaskQueueRepo` constructor helpers
- [x] Replace `CreateTaskQueueDao::*` usage in task flow
- [x] Add minimal tests for constructor helpers
- [x] Run focused offline validation

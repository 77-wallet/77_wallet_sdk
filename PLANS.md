# PLANS

Current task execution plan.
Refs: `docs/codex/testing.md`, `docs/codex/workflows.md`.

## Task

- Name: repositories convergence (batch 103: announcement repo static API)
- Goal:
  - 去除 `AnnouncementRepo::new + self.pool` 形态
  - 统一为静态方法 + `&CoreDbPool` 参数
  - 保持公告业务语义不变

## Scope

### In

- `wallet-database/src/repositories/announcement.rs`
- `wallet-api/src/domain/announcement.rs`
- `wallet-api/src/service/announcement.rs`
- `wallet-api/src/infrastructure/task_queue/task_handle/backend_handle.rs`
- `PLANS.md`

### Out

- `&CoreDbPool` 参数统一
- `*_with_executor` 命名继续扩展
- 任何 wallet-api 接口调整

## Constraints

- Keep behavior unchanged
- Batch-by-batch compile validation
- Offline validation only

## Plan

1. 将 `AnnouncementRepo` 改为静态方法并移除 `new`
2. 同步 domain/service/调用点参数传递
3. 运行最小离线验证并停止本轮

## Validation Commands

- `cargo check -p wallet-database --offline`
- `cargo check -p wallet-api --offline`

## Progress Checklist

- [x] Convert AnnouncementRepo to static pool API
- [x] Run focused offline validation

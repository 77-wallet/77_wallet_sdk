# PLANS

Current task execution plan.
Refs: `docs/codex/testing.md`, `docs/codex/workflows.md`.

## Task

- Name: SQLite read/write split (Batch 3A: task_queue routing)
- Goal:
  - 在 `task_queue` 仓库中完成读写显式路由：读走 `read_ref()`，写走 `write_ref()`
  - 事务入口统一走 writer
  - 保持接口、业务语义、schema 不变

## Scope

### In

- `wallet-database/src/repositories/task_queue.rs`
- `PLANS.md`

### Out

- 其他 core 仓库（`account/assets/wallet/...`）
- `api_wallet` 其他残留项
- `sql_utils` 结构重构
- `wallet-api` 对外接口签名改造

## Constraints

- 单批只改 1 个仓库 + 计划文件
- `as_ref()` 仅保留兼容，不在本批新增
- 不改 DAO SQL 与业务状态机

## Plan

1. 将 `task_queue` 查询类方法改为 `read_ref()`
2. 将 `task_queue` 写入/更新/删除类方法改为 `write_ref()`，事务测试改为 writer begin
3. 运行最小离线验证与定向测试

## Validation Commands

- `cargo check -p wallet-database --offline`
- `cargo test -p wallet-database task_queue_repo_ --offline -- --nocapture`

## Progress Checklist

- [x] `task_queue` 读写路由显式化完成
- [x] 回滚测试事务入口改为 writer
- [x] Focused offline checks/tests pass

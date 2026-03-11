# PLANS

Current task execution plan.
Refs: `docs/codex/testing.md`, `docs/codex/workflows.md`.

## Task

- Name: remove unused db acquire module (Batch 4D)
- Goal:
  - 移除未被业务路径使用的 `db/acquire.rs`
  - 移除 `acquire_conn` 对外导出，避免误用
  - 保持最小改动，不触及锁治理主线

## Scope

### In

- `wallet-database/src/lib.rs`
- `wallet-database/src/db/mod.rs`
- `wallet-database/src/db/acquire.rs` (delete)
- `PLANS.md`

### Out

- `wallet-api` 接口签名
- 其它 repository 的事务抽象重构
- `sql_utils` 结构改造

## Constraints

- 单批单 crate（`wallet-database`），文件数 <= 4
- 仅做删除与引用清理，不改业务逻辑
- 只运行最小离线编译验证

## Plan

1. 移除 `lib.rs` 的 `acquire_conn` re-export
2. 移除 `db/mod.rs` 的 `acquire` 模块声明
3. 删除 `db/acquire.rs`
4. 运行最小离线验证

## Validation Commands

- `cargo check -p wallet-database --offline`

## Progress Checklist

- [x] `acquire` 模块与导出已移除
- [x] Focused offline check passes

# PLANS

Current task execution plan.
Refs: `docs/codex/testing.md`, `docs/codex/workflows.md`.

## Task

- Name: SQLite read/write split (Batch 2C: api_wallet expand/query-state)
- Goal:
  - 在 `api_wallet` 子模块完成 `expand_* / *_query_state` 的读写显式路由
  - 继续保持上层接口不变、业务语义不变、schema 不变

## Scope

### In

- `wallet-database/src/repositories/api_wallet/expand_batch.rs`
- `wallet-database/src/repositories/api_wallet/expand_batch_item.rs`
- `wallet-database/src/repositories/api_wallet/expand_notify_state.rs`
- `wallet-database/src/repositories/api_wallet/address_query_state.rs`
- `wallet-database/src/repositories/api_wallet/asset_query_state.rs`
- `PLANS.md`

### Out

- `api_wallet` 其余仓库（Batch 2B/2C）
- `core/task` 与遗留写路径清理（Batch 3）
- `sql_utils` 结构重构（后置小批）
- `wallet-api` 对外接口签名改造

## Constraints

- 单批仅 `wallet-database` + `api_funds` 热点 flow
- 默认 `as_ref()` 映射 reader；写路径显式 `write_ref()`
- 事务统一从 writer 侧开始（`write_ref().begin()` 或 writer pool 等价入口）
- 先保证最小可验证闭环，避免跨模块扩散

## Plan

1. 在 `expand/query-state` 仓库中将写操作和事务入口统一切到 writer
2. 保持读查询走 reader，避免写语义误路由
3. 跑最小离线验证并记录结果，失败仅做本批内修复

## Validation Commands

- `cargo check -p wallet-database --offline`
- `cargo test -p wallet-database expand_batch_repo_ --offline -- --nocapture`
- `cargo test -p wallet-database expand_batch_item_repo_ --offline -- --nocapture`
- `cargo test -p wallet-database expand_notify_state_repo_ --offline -- --nocapture`
- `cargo test -p wallet-database address_query_state_repo_ --offline -- --nocapture`
- `cargo test -p wallet-database asset_query_state_repo_ --offline -- --nocapture`

## Progress Checklist

- [x] expand/query-state write paths route to writer
- [x] expand/query-state tests align with writer transaction entry
- [x] Focused offline checks/tests pass

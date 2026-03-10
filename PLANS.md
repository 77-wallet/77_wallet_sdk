# PLANS

Current task execution plan.
Refs: `docs/codex/testing.md`, `docs/codex/workflows.md`.

## Task

- Name: SQLite read/write split (Batch 1: api_funds hotspot sample)
- Goal:
  - 先在 `wallet-database` 内完成 `api_funds` 样板读写分离，优先治理 `database is locked`
  - 上层接口名保持不变，不改业务语义，不改 schema

## Scope

### In

- `wallet-database/src/db_pool.rs`
- `wallet-database/src/init.rs`
- `wallet-database/src/lib.rs`
- `wallet-database/src/db/acquire.rs` (仅对齐 writer 语义，若受影响)
- `wallet-database/src/repositories/api_wallet/collect.rs`
- `wallet-database/src/repositories/api_wallet/fee.rs`
- `wallet-database/src/repositories/api_wallet/withdraw.rs`
- `wallet-database/src/repositories/api_wallet/nonce.rs`
- `PLANS.md`

### Out

- `api_wallet` 全量推广（Batch 2）
- `core/task` 与遗留写路径全量清理（Batch 3）
- `sql_utils` 结构重构（后置小批）
- `wallet-api` 对外接口签名改造

## Constraints

- 单批仅 `wallet-database` + `api_funds` 热点 flow
- 默认 `as_ref()` 映射 reader；写路径显式 `write_ref()`
- 事务统一从 writer 侧开始（`write_ref().begin()` 或 writer pool 等价入口）
- 先保证最小可验证闭环，避免跨模块扩散

## Plan

1. 实现双池抽象与上下文接入（reader/writer），保持现有类型名不变
2. 在 `collect/fee/withdraw/nonce` 中把写与事务路径切到 writer，读路径保持 reader
3. 运行最小离线验证并记录结果；若失败只做本批内修复

## Validation Commands

- `cargo check -p wallet-database --offline`
- `cargo test -p wallet-database concurrent_balance_upserts --offline -- --nocapture`
- `cargo test -p wallet-database concurrent_nonce_updates --offline -- --nocapture`
- `cargo test -p wallet-database read_queries_are_not_blocked_by_long_writer_transaction --offline -- --nocapture`

## Progress Checklist

- [x] Dual-pool abstractions are in place with backward compatibility
- [x] api_funds hotspot repos route write/tx paths to writer
- [x] Focused offline checks/tests pass

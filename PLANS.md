# PLANS

Current task execution plan.
Refs: `docs/codex/testing.md`, `docs/codex/workflows.md`.

## Task

- Name: SQLite lock hardening (Batch 3H: db/acquire guard tests)
- Goal:
  - 固化 `db/acquire.rs` 的 writer 获取语义
  - 为连接获取补“成功/超时”两条回归测试
  - 复用既有 `api_wallet/nonce` 并发读写回归作为锁问题验收

## Scope

### In

- `wallet-database/src/db/acquire.rs`
- `PLANS.md`

### Out

- 仓储层读写路由改造（已完成）
- `api_wallet` 其他残留项
- `sql_utils` 结构重构
- `wallet-api` 对外接口签名改造

## Constraints

- 单批仅 `wallet-database`，3 文件内完成
- 不改 DAO SQL 与业务语义
- 不改连接池参数默认值

## Plan

1. 在 `db/acquire.rs` 增加 `acquire_conn` 成功路径测试
2. 增加 writer 被长事务占用时的连接获取超时测试（断言 timeout 错误）
3. 跑最小离线验证 + nonce 并发锁回归 + reader-not-blocked 回归

## Validation Commands

- `cargo check -p wallet-database --offline`
- `cargo test -p wallet-database db::acquire --offline -- --nocapture`
- `cargo test -p wallet-database concurrent_nonce_updates --offline -- --nocapture`
- `cargo test -p wallet-database read_queries_are_not_blocked_by_long_writer_transaction --offline -- --nocapture`

## Progress Checklist

- [x] `db/acquire` 成功/超时回归测试完成
- [x] nonce 并发与 reader-not-blocked 回归通过
- [x] Focused offline checks/tests pass

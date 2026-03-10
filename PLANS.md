# PLANS

Current task execution plan.
Refs: `docs/codex/testing.md`, `docs/codex/workflows.md`.

## Task

- Name: SQLite read/write split (Batch 3B: address_book + system_notification)
- Goal:
  - 在 `address_book` 与 `system_notification` 仓库中完成读写显式路由
  - 读走 `read_ref()/read_pool()`，写走 `write_ref()`
  - 事务入口统一走 writer

## Scope

### In

- `wallet-database/src/repositories/address_book.rs`
- `wallet-database/src/repositories/system_notification.rs`
- `PLANS.md`

### Out

- 其他 core 仓库（`account/assets/wallet/...`）
- `api_wallet` 其他残留项
- `sql_utils` 结构重构
- `wallet-api` 对外接口签名改造

## Constraints

- 单批只改 2 个仓库 + 计划文件
- 不改 DAO SQL 与业务语义
- `as_ref()` 不在本批新增

## Plan

1. `address_book`：写方法改 `write_ref()`，查方法改 `read_ref()/read_pool()`
2. `system_notification`：写方法改 `write_ref()`，查方法改 `read_ref()`，回滚测试改 writer 事务
3. 跑最小离线验证与定向测试

## Validation Commands

- `cargo check -p wallet-database --offline`
- `cargo test -p wallet-database address_book_ --offline -- --nocapture`
- `cargo test -p wallet-database system_notification_repo_ --offline -- --nocapture`

## Progress Checklist

- [x] address_book/system_notification 读写路由显式化完成
- [x] 两处回滚测试事务入口改为 writer
- [x] Focused offline checks/tests pass

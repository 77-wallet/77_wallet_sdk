# PLANS

Current task execution plan.
Refs: `docs/codex/testing.md`, `docs/codex/workflows.md`.

## Task

- Name: SQLite read/write split (Batch 3G: core multisig_account+permission)
- Goal:
  - 在 `core` 的 `multisig_account/permission` 两个仓库完成读写显式路由
  - 读走 `read_ref()`，写走 `write_ref()`
  - 回滚测试事务入口统一走 writer

## Scope

### In

- `wallet-database/src/repositories/multisig_account.rs`
- `wallet-database/src/repositories/permission.rs`
- `PLANS.md`

### Out

- 其他 core 仓库（已完成）
- `api_wallet` 其他残留项
- `sql_utils` 结构重构
- `wallet-api` 对外接口签名改造

## Constraints

- 单批仅 `wallet-database`，3 文件内完成
- 不改 DAO SQL 与业务语义
- `as_ref()` 不在本批新增

## Plan

1. 将两仓库查询路径改为 `read_ref()`
2. 将两仓库写入/更新/删除路径改为 `write_ref()`
3. 将回滚测试的事务入口改为 `write_ref().begin()`
4. 跑最小离线验证与两组定向测试

## Validation Commands

- `cargo check -p wallet-database --offline`
- `cargo test -p wallet-database multisig_account_repo_ --offline -- --nocapture`
- `cargo test -p wallet-database permission_repo_ --offline -- --nocapture`

## Progress Checklist

- [x] 两仓库读写路由显式化完成
- [x] 两仓库回滚测试事务入口改为 writer
- [x] Focused offline checks/tests pass

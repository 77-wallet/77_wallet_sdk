# PLANS

Current task execution plan.
Refs: `docs/codex/testing.md`, `docs/codex/workflows.md`.

## Task

- Name: SQLite read/write split (Batch 3C: core account + wallet)
- Goal:
  - 在 `core` 的 `account`、`wallet` 仓库中完成读写显式路由
  - 读走 `read_ref()`，写走 `write_ref()`
  - 事务入口统一走 writer

## Scope

### In

- `wallet-database/src/repositories/account.rs`
- `wallet-database/src/repositories/wallet.rs`
- `PLANS.md`

### Out

- 其他 core 仓库（`assets/coin/node/bill/...`）
- `api_wallet` 其他残留项
- `sql_utils` 结构重构
- `wallet-api` 对外接口签名改造

## Constraints

- 单批只改 2 个仓库 + 计划文件
- 不改 DAO SQL 与业务语义
- `as_ref()` 不在本批新增

## Plan

1. `account`：查询改 `read_ref()`，写方法改 `write_ref()`
2. `wallet`：查询改 `read_ref()`，写方法改 `write_ref()`，回滚测试用 writer begin
3. 跑最小离线验证与定向测试

## Validation Commands

- `cargo check -p wallet-database --offline`
- `cargo test -p wallet-database account_ --offline -- --nocapture`
- `cargo test -p wallet-database wallet_ --offline -- --nocapture`

## Progress Checklist

- [x] account/wallet 读写路由显式化完成
- [x] 两处回滚测试事务入口改为 writer
- [x] Focused offline checks/tests pass

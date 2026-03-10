# PLANS

Current task execution plan.
Refs: `docs/codex/testing.md`, `docs/codex/workflows.md`.

## Task

- Name: SQLite read/write split (Batch 3D: core assets+coin+node+bill)
- Goal:
  - 在 `core` 的 `assets/coin/node/bill` 四个仓库完成读写显式路由
  - 读走 `read_ref()`，写走 `write_ref()`
  - 回滚测试事务入口统一走 writer

## Scope

### In

- `wallet-database/src/repositories/assets.rs`
- `wallet-database/src/repositories/coin.rs`
- `wallet-database/src/repositories/node.rs`
- `wallet-database/src/repositories/bill.rs`
- `PLANS.md`

### Out

- 其他 core 仓库（`announcement/exchange_rate/multisig_*`）
- `api_wallet` 其他残留项
- `sql_utils` 结构重构
- `wallet-api` 对外接口签名改造

## Constraints

- 单批仅 `wallet-database`，5 文件内完成
- 不改 DAO SQL 与业务语义
- `as_ref()` 不在本批新增

## Plan

1. 将四仓库查询路径改为 `read_ref()`
2. 将四仓库写入/更新/删除路径改为 `write_ref()`；`into_inner()` 保持 writer 语义
3. 将回滚测试的事务入口改为 `write_ref().begin()`
4. 跑最小离线验证与四组定向测试

## Validation Commands

- `cargo check -p wallet-database --offline`
- `cargo test -p wallet-database assets_ --offline -- --nocapture`
- `cargo test -p wallet-database coin_repo_ --offline -- --nocapture`
- `cargo test -p wallet-database node_repo_ --offline -- --nocapture`
- `cargo test -p wallet-database bill_repo_ --offline -- --nocapture`

## Progress Checklist

- [x] 四仓库读写路由显式化完成
- [x] 四仓库回滚测试事务入口改为 writer
- [x] Focused offline checks/tests pass

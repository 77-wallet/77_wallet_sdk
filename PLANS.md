# PLANS

Current task execution plan.
Refs: `docs/codex/testing.md`, `docs/codex/workflows.md`.

## Task

- Name: SQLite read/write split (Batch 2A: api_wallet assets/account/wallet)
- Goal:
  - 在 `api_wallet` 子模块先落地 3 个核心仓库的读写路由
  - 继续保持上层接口不变、业务语义不变、schema 不变

## Scope

### In

- `wallet-database/src/repositories/api_wallet/assets.rs`
- `wallet-database/src/repositories/api_wallet/account.rs`
- `wallet-database/src/repositories/api_wallet/wallet.rs`
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

1. 在 `assets/account/wallet` 中将写操作和事务入口统一切到 writer
2. 保持读查询走 reader，避免写语义误路由
3. 跑最小离线验证并记录结果，失败仅做本批内修复

## Validation Commands

- `cargo check -p wallet-database --offline`
- `cargo test -p wallet-database assets_repo_ --offline -- --nocapture`
- `cargo test -p wallet-database account_repo_ --offline -- --nocapture`
- `cargo test -p wallet-database api_wallet_repo_ --offline -- --nocapture`

## Progress Checklist

- [x] assets/account/wallet write paths route to writer
- [x] assets/account/wallet tests align with writer transaction entry
- [x] Focused offline checks/tests pass

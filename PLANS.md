# PLANS

Current task execution plan.
Refs: `docs/codex/testing.md`, `docs/codex/workflows.md`.

## Task

- Name: api_wallet assets lock regression (Batch 3J)
- Goal:
  - 在真实 `api_assets` 写路径补并发锁回归（多 writer 复现 / 默认配置成功）
  - 补“长写事务期间读可返回”回归，验证 reader 不被 writer 全阻塞
  - 不改生产行为，仅新增测试与最小测试 helper

## Scope

### In

- `wallet-database/src/repositories/api_wallet/assets.rs`
- `wallet-database/src/repositories/test_helper.rs`
- `PLANS.md`

### Out

- 仓储层读写路由改造（已完成）
- `api_wallet` 其他残留项
- 跨 DAO 大规模重构
- `wallet-api` 对外接口签名改造

## Constraints

- 单批仅 `wallet-database`，3 文件内完成
- 不改 DAO SQL 与业务语义
- 仅新增稳定离线测试，不引入 flaky 压测

## Plan

1. 增加 `setup_api_wallet_pool_with_config` helper，支持多 writer 复现池
2. 在 `api_wallet/assets` 增加并发锁回归与 reader-not-blocked 回归
3. 跑最小离线验证与 assets 定向测试

## Validation Commands

- `cargo check -p wallet-database --offline`
- `cargo test -p wallet-database assets_ --offline -- --nocapture`
- `cargo test -p wallet-database concurrent_balance_upserts_assets --offline -- --nocapture`
- `cargo test -p wallet-database read_queries_are_not_blocked_by_long_writer_transaction_assets --offline -- --nocapture`

## Progress Checklist

- [x] `api_assets` 锁复现与默认回归测试完成
- [x] `api_assets` reader-not-blocked 回归完成
- [x] Focused offline checks/tests pass

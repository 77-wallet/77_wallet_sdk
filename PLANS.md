# PLANS

Current task execution plan.
Refs: `docs/codex/testing.md`, `docs/codex/workflows.md`.

## Task

- Name: api_wallet fee lock regression (Batch 3K)
- Goal:
  - 在真实 `api_fee + api_nonce` 热点写路径补并发锁回归（多 writer 复现 / 默认配置成功）
  - 补“长写事务期间读可返回”回归，验证 reader 不被 writer 全阻塞
  - 不改生产行为，仅新增测试

## Scope

### In

- `wallet-database/src/repositories/api_wallet/fee.rs`
- `PLANS.md`

### Out

- 仓储层读写路由改造（已完成）
- `api_wallet` 其他残留项
- 跨 DAO 大规模重构
- `wallet-api` 对外接口签名改造

## Constraints

- 单批仅 `wallet-database`，2 文件内完成
- 不改 DAO SQL 与业务语义
- 仅新增稳定离线测试，不引入 flaky 压测

## Plan

1. 在 `api_wallet/fee` 增加并发锁回归（`concurrent_fee_nonce_updates`）
2. 在 `api_wallet/fee` 增加 reader-not-blocked 回归
3. 跑最小离线验证与 fee 定向测试

## Validation Commands

- `cargo check -p wallet-database --offline`
- `cargo test -p wallet-database fee_ --offline -- --nocapture`
- `cargo test -p wallet-database concurrent_fee_nonce_updates --offline -- --nocapture`
- `cargo test -p wallet-database read_queries_are_not_blocked_by_long_writer_transaction_fee --offline -- --nocapture`

## Progress Checklist

- [x] `api_fee` 锁复现与默认回归测试完成
- [x] `api_fee` reader-not-blocked 回归完成
- [x] Focused offline checks/tests pass

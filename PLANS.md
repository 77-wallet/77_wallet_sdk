# PLANS

Current task execution plan.
Refs: `docs/codex/testing.md`, `docs/codex/workflows.md`.

## Task

- Name: lock regression suite consolidation (Batch 3L)
- Goal:
  - 将锁回归收敛为最小代表集：`api_assets` 写冲突 + `api_fee/api_nonce` 事务冲突 + 单一 reader-not-blocked
  - 移除重复的跨 repo reader-not-blocked 测试，避免测试膨胀
  - 不改生产行为，仅整理测试集

## Scope

### In

- `wallet-database/src/repositories/api_wallet/assets.rs`
- `wallet-database/src/repositories/api_wallet/fee.rs`
- `wallet-database/src/repositories/api_wallet/nonce.rs`
- `PLANS.md`

### Out

- 仓储层读写路由改造（已完成）
- `api_wallet` 其他残留项
- 跨 DAO 大规模重构
- `wallet-api` 对外接口签名改造

## Constraints

- 单批仅 `wallet-database`，4 文件内完成
- 不改 DAO SQL 与业务语义
- 仅测试集收敛，不新增压力场景

## Plan

1. 保留 `api_assets` 并发写锁回归
2. 保留 `api_fee/api_nonce` 并发事务锁回归
3. 仅保留 `nonce` 的 reader-not-blocked 通用回归，移除 assets/fee 同类重复用例
4. 跑最小离线验证与目标测试

## Validation Commands

- `cargo check -p wallet-database --offline`
- `cargo test -p wallet-database assets_ --offline -- --nocapture`
- `cargo test -p wallet-database fee_ --offline -- --nocapture`
- `cargo test -p wallet-database concurrent_nonce_updates --offline -- --nocapture`
- `cargo test -p wallet-database concurrent_fee_nonce_updates --offline -- --nocapture`
- `cargo test -p wallet-database concurrent_balance_upserts_assets --offline -- --nocapture`
- `cargo test -p wallet-database read_queries_are_not_blocked_by_long_writer_transaction --offline -- --nocapture`

## Progress Checklist

- [x] 代表性三类回归保留完成（assets/fee+nonce/reader-not-blocked）
- [x] 重复 reader-not-blocked 用例清理完成
- [x] Focused offline checks/tests pass

# PLANS

Current task execution plan.
Refs: `docs/codex/testing.md`, `docs/codex/workflows.md`.

## Task

- Name: sqlite lock hardening (Batch 4A)
- Goal:
  - 在现有读写分离基础上补齐“配套两点”：锁错误有限重试 + 缩短热点写事务持锁时间
  - 仅覆盖已验证热点：`api_assets`、`api_fee`（并同步 `api_collect`/`api_withdraw` 的 nonce 事务写路径）
  - 不改业务语义与 schema

## Scope

### In

- `wallet-database/src/db/mod.rs`
- `wallet-database/src/db/sqlite_retry.rs` (new)
- `wallet-database/src/dao/api_collect.rs`
- `wallet-database/src/dao/api_fee.rs`
- `wallet-database/src/dao/api_withdraw.rs`
- `wallet-database/src/repositories/api_wallet/assets.rs`
- `PLANS.md`

### Out

- `wallet-api` 接口签名
- 其它 repository 的事务抽象重构
- `sql_utils` 结构改造

## Constraints

- 单批单 crate（`wallet-database`），文件数 < 10
- 先复用现有锁回归测试，不扩展 flaky 压测
- 只对 sqlite lock（code 5）做有限重试，避免吞掉其它错误

## Plan

1. 新增 sqlite lock 重试 helper（指数退避，2 次重试）
2. 在 `api_fee/api_collect/api_withdraw` 的 `update_tx_status_nonce` 事务写路径接入 helper
3. 在 `api_assets` 批量 upsert 路径把单长事务改为分块短事务（按块提交）
4. 运行最小离线验证与锁回归用例

## Validation Commands

- `cargo check -p wallet-database --offline`
- `cargo test -p wallet-database concurrent_balance_upserts_assets --offline -- --nocapture`
- `cargo test -p wallet-database concurrent_fee_nonce_updates --offline -- --nocapture`
- `cargo test -p wallet-database concurrent_nonce_updates --offline -- --nocapture`

## Progress Checklist

- [x] sqlite lock 重试 helper 已落地并被热点 DAO 使用
- [x] `api_assets` 批量写已分块短事务
- [x] Focused offline checks/tests pass

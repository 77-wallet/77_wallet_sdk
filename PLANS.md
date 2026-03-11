# PLANS

Current task execution plan.
Refs: `docs/codex/testing.md`, `docs/codex/workflows.md`.

## Task

- Name: sqlite writer gate strengthen (Batch 4B)
- Goal:
  - 在现有 `writer=1 + retry + 短事务` 基础上增加显式 writer gate，进一步收敛偶发锁冲突
  - 仅覆盖热点写入口：`api_assets`、`api_fee`、`api_collect`、`api_withdraw`
  - 不改业务语义与 schema

## Scope

### In

- `wallet-database/src/db_pool.rs`
- `wallet-database/src/repositories/api_wallet/assets.rs`
- `wallet-database/src/repositories/api_wallet/fee.rs`
- `wallet-database/src/repositories/api_wallet/collect.rs`
- `wallet-database/src/repositories/api_wallet/withdraw.rs`
- `PLANS.md`

### Out

- `wallet-api` 接口签名
- 其它 repository 的事务抽象重构
- `sql_utils` 结构改造

## Constraints

- 单批单 crate（`wallet-database`），文件数 <= 6
- 先复用现有锁回归测试，不扩展 flaky 压测
- gate 仅用于热点写入口，不做全量 repository 普改

## Plan

1. 在 `db_pool` 增加显式 writer gate 接口
2. 在 `api_assets` 批量写入口接入 writer gate
3. 在 `api_fee/api_collect/api_withdraw` 的 `update_*_tx_status_nonce` 入口接入 writer gate
4. 增加 writer gate 排队延迟可观测测试（`api_fee`）
5. 运行最小离线验证与锁回归用例

## Validation Commands

- `cargo check -p wallet-database --offline`
- `cargo test -p wallet-database concurrent_balance_upserts_assets --offline -- --nocapture`
- `cargo test -p wallet-database concurrent_fee_nonce_updates --offline -- --nocapture`
- `cargo test -p wallet-database concurrent_nonce_updates --offline -- --nocapture`
- `cargo test -p wallet-database writer_gate_introduces_queueing_delay_on_hot_write --offline -- --nocapture`

## Progress Checklist

- [x] writer gate 接口已落地
- [x] 热点入口（assets/fee/collect/withdraw）已接入 writer gate
- [x] writer gate 排队延迟测试已新增并通过
- [x] Focused offline checks/tests pass

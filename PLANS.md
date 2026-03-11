# PLANS

Current task execution plan.
Refs: `docs/codex/testing.md`, `docs/codex/workflows.md`.

## Task

- Name: sqlite lock observability (Batch 4C)
- Goal:
  - 补齐线上观测三指标：`writer_gate_wait_ms`、`sqlite_locked_retry_count`、`write_tx_duration_ms`
  - 仅在现有热点写入口增加结构化日志，不改业务语义
  - 保持 `wallet-database` 单 crate 小批次

## Scope

### In

- `wallet-database/src/db_pool.rs`
- `wallet-database/src/db/sqlite_retry.rs`
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

- 单批单 crate（`wallet-database`），文件数 < 10
- 不新增第三方 metrics 依赖，先用结构化日志埋点
- 仅热点路径埋点，不扩散到所有 repository

## Plan

1. 在 `db_pool` 记录 `writer_gate_wait_ms`
2. 在 `sqlite_retry` 记录 `sqlite_locked_retry_count`
3. 在 `api_assets/fee/collect/withdraw` 热点写入口记录 `write_tx_duration_ms`
4. 运行最小离线验证与目标测试

## Validation Commands

- `cargo check -p wallet-database --offline`
- `cargo test -p wallet-database concurrent_balance_upserts_assets --offline -- --nocapture`
- `cargo test -p wallet-database concurrent_fee_nonce_updates --offline -- --nocapture`
- `cargo test -p wallet-database concurrent_nonce_updates --offline -- --nocapture`
- `cargo test -p wallet-database writer_gate_introduces_queueing_delay_on_hot_write --offline -- --nocapture`

## Progress Checklist

- [x] 三个观测指标都已落地（日志埋点）
- [x] Focused offline checks/tests pass

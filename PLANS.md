# PLANS

Current task execution plan.
Refs: `docs/codex/testing.md`, `docs/codex/workflows.md`.

## Task

- Name: wallet-database test-first convergence (batch B19-B21: device + exchange_rate + stake)
- Goal:
  - 在不改生产逻辑前提下，完成 `repositories/device`、`repositories/exchange_rate`、`repositories/stake` 三个模块测试护栏
  - 继续以确定性、离线可运行为准

## Scope

### In

- `wallet-database/src/repositories/device.rs`
- `wallet-database/src/repositories/exchange_rate.rs`
- `wallet-database/src/repositories/stake.rs`
- `PLANS.md`

### Out

- 仓储 API/事务抽象改造
- DAO SQL 语义调整
- 其它 `api_wallet/*` 仓库测试扩展

## Constraints

- Test-only changes; no production behavior changes
- Offline validation only
- Deterministic tests only (no flaky stress patterns)

## Plan

1. 为 `device/exchange_rate/stake` 三个 repo 各补成功、边界、回滚三类测试
3. 执行最小验证命令并记录结果

## Validation Commands

- `cargo check -p wallet-database --offline`
- `cargo test -p wallet-database device_repo_ --offline -- --nocapture`
- `cargo test -p wallet-database exchange_rate_repo_ --offline -- --nocapture`
- `cargo test -p wallet-database stake_repo_ --offline -- --nocapture`

## Progress Checklist

- [x] Add device success/edge/rollback tests
- [x] Add exchange_rate success/edge/rollback tests
- [x] Add stake success/edge/rollback tests
- [x] Run focused offline validation

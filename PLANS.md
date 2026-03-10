# PLANS

Current task execution plan.
Refs: `docs/codex/testing.md`, `docs/codex/workflows.md`.

## Task

- Name: wallet-database test-first convergence (batch B13-B15: strategy_chain_config + expand_notify_state)
- Goal:
  - 在不改生产逻辑前提下，连续完成 `api_wallet/collect_strategy_chain_config`、`api_wallet/withdraw_strategy_chain_config`、`api_wallet/expand_notify_state` 三个模块测试护栏
  - 继续以确定性、离线可运行为准

## Scope

### In

- `wallet-database/src/repositories/api_wallet/collect_strategy_chain_config.rs`
- `wallet-database/src/repositories/api_wallet/withdraw_strategy_chain_config.rs`
- `wallet-database/src/repositories/api_wallet/expand_notify_state.rs`
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

1. 为 `collect_strategy_chain_config/withdraw_strategy_chain_config/expand_notify_state` 三个 repo 各补成功、边界、回滚三类测试
3. 执行最小验证命令并记录结果

## Validation Commands

- `cargo check -p wallet-database --offline`
- `cargo test -p wallet-database collect_strategy_chain_config_repo_ --offline -- --nocapture`
- `cargo test -p wallet-database withdraw_strategy_chain_config_repo_ --offline -- --nocapture`
- `cargo test -p wallet-database expand_notify_state_repo_ --offline -- --nocapture`

## Progress Checklist

- [x] Add collect_strategy_chain_config success/edge/rollback tests
- [x] Add withdraw_strategy_chain_config success/edge/rollback tests
- [x] Add expand_notify_state success/edge/rollback tests
- [x] Run focused offline validation

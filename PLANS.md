# PLANS

Current task execution plan.
Refs: `docs/codex/testing.md`, `docs/codex/workflows.md`.

## Task

- Name: wallet-database test-first convergence (batch B7-B9: account/coin/chain)
- Goal:
  - 在不改生产逻辑前提下，连续完成 `api_wallet/account`、`api_wallet/coin`、`api_wallet/chain` 三个模块测试护栏
  - 继续以确定性、离线可运行为准

## Scope

### In

- `wallet-database/src/repositories/api_wallet/account.rs`
- `wallet-database/src/repositories/api_wallet/coin.rs`
- `wallet-database/src/repositories/api_wallet/chain.rs`
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

1. 为 `account/coin/chain` 三个 repo 各补成功、边界、回滚三类测试
3. 执行最小验证命令并记录结果

## Validation Commands

- `cargo check -p wallet-database --offline`
- `cargo test -p wallet-database account_repo_ --offline -- --nocapture`
- `cargo test -p wallet-database coin_repo_ --offline -- --nocapture`
- `cargo test -p wallet-database chain_repo_ --offline -- --nocapture`

## Progress Checklist

- [x] Add account success/edge/rollback tests
- [x] Add coin success/edge/rollback tests
- [x] Add chain success/edge/rollback tests
- [x] Run focused offline validation

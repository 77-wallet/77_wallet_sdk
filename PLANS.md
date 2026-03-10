# PLANS

Current task execution plan.
Refs: `docs/codex/testing.md`, `docs/codex/workflows.md`.

## Task

- Name: wallet-database test-first convergence (batch B25-B27: config + chain + node)
- Goal:
  - 在不改生产逻辑前提下，完成 `repositories/config`、`repositories/chain`、`repositories/node` 三个模块测试护栏
  - 继续以确定性、离线可运行为准

## Scope

### In

- `wallet-database/src/repositories/config.rs`
- `wallet-database/src/repositories/chain.rs`
- `wallet-database/src/repositories/node.rs`
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

1. 为 `config/chain/node` 三个 repo 各补成功、边界、回滚三类测试
3. 执行最小验证命令并记录结果

## Validation Commands

- `cargo check -p wallet-database --offline`
- `cargo test -p wallet-database config_repo_ --offline -- --nocapture`
- `cargo test -p wallet-database chain_repo_ --offline -- --nocapture`
- `cargo test -p wallet-database node_repo_ --offline -- --nocapture`

## Progress Checklist

- [x] Add config success/edge/rollback tests
- [x] Add chain success/edge/rollback tests
- [x] Add node success/edge/rollback tests
- [x] Run focused offline validation

# PLANS

Current task execution plan.
Refs: `docs/codex/testing.md`, `docs/codex/workflows.md`.

## Task

- Name: wallet-database test-first convergence (batch B34-B36: address_query_state + coin + with_tx)
- Goal:
  - 在不改生产逻辑前提下，完成 `repositories/api_wallet/address_query_state`、`repositories/coin`、`repositories/mod(with_tx)` 三个模块测试护栏
  - 继续以确定性、离线可运行为准

## Scope

### In

- `wallet-database/src/repositories/api_wallet/address_query_state.rs`
- `wallet-database/src/repositories/coin.rs`
- `wallet-database/src/repositories/mod.rs`
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

1. 为 `address_query_state/coin/with_tx` 三个模块补成功、边界、回滚（或等价错误回滚）测试
3. 执行最小验证命令并记录结果

## Validation Commands

- `cargo check -p wallet-database --offline`
- `cargo test -p wallet-database address_query_state_repo_ --offline -- --nocapture`
- `cargo test -p wallet-database coin_repo_ --offline -- --nocapture`
- `cargo test -p wallet-database with_tx_ --offline -- --nocapture`

## Progress Checklist

- [x] Add address_query_state success/edge/rollback tests
- [x] Add coin success/edge/rollback tests
- [x] Add with_tx success/edge/rollback-style tests
- [x] Run focused offline validation

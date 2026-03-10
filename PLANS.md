# PLANS

Current task execution plan.
Refs: `docs/codex/testing.md`, `docs/codex/workflows.md`.

## Task

- Name: wallet-database test-first convergence (batch B55-B57: expand_batch + expand_batch_item + address_query_state assertion hardening)
- Goal:
  - 不改生产逻辑，仅增强 `expand_batch/expand_batch_item/address_query_state` 的断言强度
  - 保持离线稳定，补充关键字段与状态一致性断言

## Scope

### In

- `wallet-database/src/repositories/api_wallet/collect_strategy_chain_config.rs`
- `wallet-database/src/repositories/api_wallet/withdraw_strategy_chain_config.rs`
- `wallet-database/src/repositories/api_wallet/expand_notify_state.rs`
- `wallet-database/src/repositories/api_wallet/expand_batch.rs`
- `wallet-database/src/repositories/api_wallet/expand_batch_item.rs`
- `wallet-database/src/repositories/api_wallet/address_query_state.rs`
- `PLANS.md`

### Out

- 事务抽象/API 形态改造
- DAO SQL 语义调整
- 其它仓库新增用例扩展

## Constraints

- Test-only changes; no production behavior changes
- Offline validation only
- Deterministic tests only (no flaky stress patterns)

## Plan

1. 为 `expand_batch` 成功/回滚用例补本地完成事实相关断言
2. 为 `expand_batch_item/address_query_state` 成功/回滚用例补关键字段与状态断言
3. 跑最小离线验证命令并记录结果

## Validation Commands

- `cargo check -p wallet-database --offline`
- `cargo test -p wallet-database expand_batch_repo_ --offline -- --nocapture`
- `cargo test -p wallet-database expand_batch_item_repo_ --offline -- --nocapture`
- `cargo test -p wallet-database address_query_state_repo_ --offline -- --nocapture`

## Progress Checklist

- [x] Harden expand_batch tests
- [x] Harden expand_batch_item tests
- [x] Harden address_query_state tests
- [x] Run focused offline validation

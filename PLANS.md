# PLANS

Current task execution plan.
Refs: `docs/codex/testing.md`, `docs/codex/workflows.md`.

## Task

- Name: wallet-database test-first convergence (batch B52-B54: collect_strategy_chain_config + withdraw_strategy_chain_config + expand_notify_state assertion hardening)
- Goal:
  - 不改生产逻辑，仅增强 `collect_strategy_chain_config/withdraw_strategy_chain_config/expand_notify_state` 的断言强度
  - 保持离线稳定，补充关键字段与回滚后一致性断言

## Scope

### In

- `wallet-database/src/repositories/api_wallet/collect_strategy_chain_config.rs`
- `wallet-database/src/repositories/api_wallet/withdraw_strategy_chain_config.rs`
- `wallet-database/src/repositories/api_wallet/expand_notify_state.rs`
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

1. 为 `collect_strategy_chain_config/withdraw_strategy_chain_config` 成功与回滚用例补字段与数量断言
2. 为 `expand_notify_state` 成功用例补 upsert 覆盖断言（重复写后值生效）
3. 跑最小离线验证命令并记录结果

## Validation Commands

- `cargo check -p wallet-database --offline`
- `cargo test -p wallet-database collect_strategy_chain_config_repo_ --offline -- --nocapture`
- `cargo test -p wallet-database withdraw_strategy_chain_config_repo_ --offline -- --nocapture`
- `cargo test -p wallet-database expand_notify_state_repo_ --offline -- --nocapture`

## Progress Checklist

- [ ] Harden collect_strategy_chain_config tests
- [ ] Harden withdraw_strategy_chain_config tests
- [ ] Harden expand_notify_state tests
- [ ] Run focused offline validation

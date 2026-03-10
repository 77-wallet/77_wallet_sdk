# PLANS

Current task execution plan.
Refs: `docs/codex/testing.md`, `docs/codex/workflows.md`.

## Task

- Name: wallet-database test-first convergence (batch B40-B42: api_wallet expand_notify_state + expand_batch + expand_batch_item assertion hardening)
- Goal:
  - 不改生产逻辑，仅增强 `expand_notify_state/expand_batch/expand_batch_item` 已有三类测试的最终状态断言强度
  - 将“只验证成功/失败”提升为“验证落库字段与数量一致性”

## Scope

### In

- `wallet-database/src/repositories/api_wallet/collect.rs`
- `wallet-database/src/repositories/api_wallet/fee.rs`
- `wallet-database/src/repositories/api_wallet/withdraw.rs`
- `wallet-database/src/repositories/api_wallet/expand_notify_state.rs`
- `wallet-database/src/repositories/api_wallet/expand_batch.rs`
- `wallet-database/src/repositories/api_wallet/expand_batch_item.rs`
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

1. 为 `expand_notify_state/expand_batch/expand_batch_item` 的成功用例补关键字段一致性断言
2. 为回滚用例补“回滚后计数/分页为空”断言，避免仅靠单条查询失败断言
3. 跑最小离线验证命令并记录结果

## Validation Commands

- `cargo check -p wallet-database --offline`
- `cargo test -p wallet-database expand_notify_state_repo_ --offline -- --nocapture`
- `cargo test -p wallet-database expand_batch_repo_ --offline -- --nocapture`
- `cargo test -p wallet-database expand_batch_item_repo_ --offline -- --nocapture`

## Progress Checklist

- [x] Harden expand_notify_state tests
- [x] Harden expand_batch tests
- [x] Harden expand_batch_item tests
- [x] Run focused offline validation

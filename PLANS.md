# PLANS

Current task execution plan.
Refs: `docs/codex/testing.md`, `docs/codex/workflows.md`.

## Task

- Name: repositories pool typing convergence (core modules)
- Goal:
  - `wallet-database/src/repositories` 中非 `api_wallet`、非 `task_queue` 的仓储接口不再使用 `crate::DbPool` 作为对外参数
  - 统一收敛到 `CoreDbPool`
  - 行为保持不变

## Scope

### In

- `wallet-database/src/repositories`（除 `api_wallet/*`、`task_queue.rs`）
- 受影响 `wallet-database/src/factory.rs`
- 受影响 `wallet-api` 调用点（仅类型适配）
- `PLANS.md`

### Out

- 业务语义/SQL 变更
- 事务模型重构
- 锁治理/连接池策略调整

## Constraints

- Keep behavior unchanged
- Batch-by-batch compile validation
- Offline validation only

## Plan

1. Refactor repository signatures from `DbPool` to `CoreDbPool` in core modules
2. Update constructors/factory methods for core repos to accept `CoreDbPool`
3. Fix wallet-api and intra-crate call sites to pass `CoreDbPool`
4. Run offline checks for `wallet-database` and `wallet-api`

## Validation Commands

- `cargo check -p wallet-database --offline`
- `cargo check -p wallet-api --offline`

## Progress Checklist

- [x] Refactor core repository signatures
- [x] Update core repo constructors/factory
- [ ] Fix affected call sites
- [ ] Run focused offline validation

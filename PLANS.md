# PLANS

Current task execution plan.  
Refs: `docs/codex/testing.md`, `docs/codex/workflows.md`.

## Task

- Name: wallet-database sql_utils first-batch refactor
- Goal:
  - 收敛 `wallet-database/src/sql_utils` 的执行语义和参数绑定模型
  - 不改 repository 事务模型，不改 `wallet-api`
  - 用最小 DAO 迁移保证现有行为保持稳定

## Scope

### In

- `wallet-database/src/sql_utils/*`
- 直接依赖 `DynamicUpdateBuilder` / `DynamicDeleteBuilder` 返回行语义的少量 DAO
- `PLANS.md`

### Out

- `repositories/mod.rs` 事务模型重构
- SQLite 连接池策略调整
- `wallet-api` 兼容层改动

## Constraints

- Keep business semantics unchanged
- Tests first for touched infra
- Offline validation only
- Limit change set to one crate and one infra module

## Plan

1. Replace runtime arg closures with explicit argument collection in `sql_utils`
2. Make `UPDATE` / `DELETE` builders opt-in for `RETURNING`
3. Migrate only affected DAO call sites to explicit `.returning("*")`
4. Add focused `sql_utils` tests for arg order, bind failure, and returning semantics
5. Validate with crate check plus minimal test filters

## Validation Commands

- `cargo check -p wallet-database --offline`
- `cargo test -p wallet-database sql_utils --offline -- --nocapture`

## Expected Results

- 参数绑定错误不再被静默忽略
- `DynamicUpdateBuilder` / `DynamicDeleteBuilder` 默认不再附加 `RETURNING *`
- 现有依赖返回行的 DAO 通过显式 returning 保持行为不变
- `wallet-database` 离线编译通过

## Progress Checklist

- [x] Rewrite `sql_utils` internals
- [x] Migrate affected DAO call sites
- [x] Add focused tests
- [x] Run validation commands

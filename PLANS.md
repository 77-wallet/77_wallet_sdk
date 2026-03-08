# PLANS

Current task execution plan.
Refs: `docs/codex/testing.md`, `docs/codex/workflows.md`.

## Task

- Name: repoctx decoupling (batch 8: bill/stake repo stateless cleanup)
- Goal:
  - 将 `BillRepo` / `StakeRepo` 从“实例持有 CoreDbPool”收敛为“静态方法 + 显式 CoreDbPool 参数”
  - 保持行为不变，继续减小隐式依赖
  - 不扩展到 service 或 transaction 语义改造

## Scope

### In

- `wallet-database/src/repositories/bill.rs`
- `wallet-database/src/repositories/stake.rs`
- `PLANS.md`

### Out

- `wallet-api` service 层改造
- `RepoCtx` 结构与事务模型改造
- DAO/SQL 语义变更

## Constraints

- Keep behavior unchanged
- Batch-by-batch compile validation
- Offline validation only

## Plan

1. Convert `BillRepo` instance field usage to static API with explicit `&CoreDbPool`
2. Convert `StakeRepo` instance field usage to static API with explicit `&CoreDbPool`
3. Keep compatibility `new(...)` constructors as thin no-op shims
4. Run offline checks for `wallet-database` and `wallet-api`

## Validation Commands

- `cargo check -p wallet-database --offline`
- `cargo check -p wallet-api --offline`

## Progress Checklist

- [x] Convert BillRepo static APIs
- [x] Convert StakeRepo static APIs
- [x] Keep compatibility constructors
- [x] Run focused offline validation

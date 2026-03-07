# PLANS

Current task execution plan.
Refs: `docs/codex/testing.md`, `docs/codex/workflows.md`.

## Task

- Name: repository trait/transaction convergence (batch 1)
- Goal:
  - 修复当前 `UnitOfWork` 借用错误，恢复编译基线
  - 移除 `repositories/mod.rs` 里显式 `impl XxxRepoTrait for RepoCtx` 的耦合写法
  - 减少 `wallet-api` 对 `TransactionTrait` 的直接依赖暴露（先从已改动链路开始）

## Scope

### In

- `wallet-database/src/repositories/mod.rs`
- `wallet-database/src/repositories/{chain,coin,bill,assets,node}.rs`
- `wallet-api` 中受影响最小调用点
- `PLANS.md`

### Out

- 大范围 repository 接口重写
- DAO/SQL 语义改动
- SQLite 锁治理策略改动
- wallet-api 全量风格迁移

## Constraints

- Keep business semantics unchanged
- Small reversible patch set
- Offline validation only

## Plan

1. Fix `UnitOfWork::executor` borrow issue and restore compile baseline
2. Replace per-type trait impls with blanket impls in repository trait modules
3. Migrate minimal wallet-api usage away from explicit `TransactionTrait` import where possible
4. Run offline checks for `wallet-database` and `wallet-api`

## Validation Commands

- `cargo check -p wallet-database --offline`
- `cargo check -p wallet-api --offline`

## Progress Checklist

- [x] Fix `UnitOfWork` borrow error
- [x] Remove explicit `impl XxxRepoTrait for RepoCtx` lines
- [x] Add blanket impls for selected repository traits
- [x] Shrink direct `TransactionTrait` usage in touched wallet-api path
- [x] Run focused offline validation

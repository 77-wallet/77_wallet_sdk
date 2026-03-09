# PLANS

Current task execution plan.
Refs: `docs/codex/testing.md`, `docs/codex/workflows.md`.

## Task

- Name: repoctx decoupling (batch 43: remove RepoCtx core and dead executor macro)
- Goal:
  - 删除 `repositories/mod.rs` 中 `RepoCtx` 与 `ExecutorWrapper`
  - 删除无调用的 `execute_with_executor!` 宏
  - 保留 `with_tx` 作为轻量事务 helper
  - 保持行为与 SQL 语义不变

## Scope

### In

- `wallet-database/src/repositories/mod.rs`
- `wallet-database/src/lib.rs`
- `PLANS.md`

### Out

- `RepoCtx` 结构删除
- 其他 repository 模块改造
- wallet-api 侧调用改动

## Constraints

- Keep behavior unchanged
- Batch-by-batch compile validation
- Offline validation only

## Plan

1. 删除 `repositories/mod.rs` 中 `RepoCtx` 与 `ExecutorWrapper` 代码
2. 删除 `lib.rs` 中无调用的 `execute_with_executor!` 宏
3. 用全局检索确认 `RepoCtx`、`ExecutorWrapper`、`execute_with_executor!` 已无残留
4. 运行离线编译校验（`wallet-database` + `wallet-api`）

## Validation Commands

- `cargo check -p wallet-database --offline`
- `cargo check -p wallet-api --offline`

## Progress Checklist

- [x] Remove `RepoCtx` core and dead executor macro
- [x] Keep behavior unchanged
- [x] Run focused offline validation

# PLANS

Current task execution plan.
Refs: `docs/codex/testing.md`, `docs/codex/workflows.md`.

## Task

- Name: layering cleanup (batch 51: chain transaction domain use repos)
- Goal:
  - `wallet-api/src/domain/chain/transaction.rs` 不再直接调用 `Entity::*`
  - 改为 `AssetsRepo/AccountRepo` 提供同语义调用
  - 保持行为不变，仅收敛调用分层

## Scope

### In

- `wallet-api/src/domain/chain/transaction.rs`
- `PLANS.md`

### Out

- 其他 service/domain 模块
- repository/dao 结构性重构
- 事务模型变更

## Constraints

- Keep behavior unchanged
- Batch-by-batch compile validation
- Offline validation only

## Plan

1. 替换 `domain/chain/transaction.rs` 中 `Entity::*` 直调为 repo 调用
3. 清理不再需要的 entity import
3. 运行离线编译校验（`wallet-database` + `wallet-api`)

## Validation Commands

- `cargo check -p wallet-database --offline`
- `cargo check -p wallet-api --offline`

## Progress Checklist

- [x] Replace direct entity calls in chain transaction domain
- [x] Remove stale imports
- [x] Run focused offline validation

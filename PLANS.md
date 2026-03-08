# PLANS

Current task execution plan.
Refs: `docs/codex/testing.md`, `docs/codex/workflows.md`.

## Task

- Name: repoctx decoupling (batch 12: multisig_account borrow tightening)
- Goal:
  - 在不改业务行为和调用流程的前提下，收紧 `MultisigAccountRepo` 方法借用签名
  - 将无需可变借用的方法从 `&mut self` 调整为 `&self`
  - 降低后续去实例化改造的阻力

## Scope

### In

- `wallet-database/src/repositories/multisig_account.rs`
- `PLANS.md`

### Out

- `wallet-api` service 调用流程改写
- `RepositoryFactory` 改动
- 事务/DAO/SQL 语义变更

## Constraints

- Keep behavior unchanged
- Batch-by-batch compile validation
- Offline validation only

## Plan

1. Tighten non-mutating `MultisigAccountRepo` method receivers from `&mut self` to `&self`
2. Keep existing method names and behavior unchanged
3. Run offline checks for `wallet-database` and `wallet-api`

## Validation Commands

- `cargo check -p wallet-database --offline`
- `cargo check -p wallet-api --offline`

## Progress Checklist

- [x] Tighten repo method borrow signatures
- [x] Keep behavior unchanged
- [x] Run focused offline validation

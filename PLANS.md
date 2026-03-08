# PLANS

Current task execution plan.
Refs: `docs/codex/testing.md`, `docs/codex/workflows.md`.

## Task

- Name: repoctx decoupling (batch 10: multisig_queue tail cleanup)
- Goal:
  - 仅清理 `MultisigQueueRepo` 中残余实例方法
  - 去掉 `wallet-api` 中对 `MultisigQueueRepo::new(...)` 的唯一依赖点
  - 保持业务行为不变

## Scope

### In

- `wallet-database/src/repositories/multisig_queue.rs`
- `wallet-api/src/service/multisig_transaction.rs`
- `PLANS.md`

### Out

- `multisig_account` 仓库改造
- 事务模型变更
- 其他 service/repo 大改

## Constraints

- Keep behavior unchanged
- Batch-by-batch compile validation
- Offline validation only

## Plan

1. Convert `MultisigQueueRepo` tail instance APIs to static APIs with `&CoreDbPool`
2. Replace the only `MultisigQueueRepo::new(...)` call site in `multisig_transaction`
3. Run offline checks for `wallet-database` and `wallet-api`

## Validation Commands

- `cargo check -p wallet-database --offline`
- `cargo check -p wallet-api --offline`

## Progress Checklist

- [x] Convert tail instance APIs in MultisigQueueRepo
- [x] Adapt multisig_transaction call site
- [x] Run focused offline validation

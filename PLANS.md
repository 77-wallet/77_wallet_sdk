# PLANS

Current task execution plan.
Refs: `docs/codex/testing.md`, `docs/codex/workflows.md`.

## Task

- Name: repoctx decoupling (batch 15: multisig_account service partial static migration)
- Goal:
  - 只迁移 `MultisigAccountService` 两个方法到 `MultisigAccountRepo` 静态 helper
  - 验证“静态 helper 可落地”而不引入大改
  - 保持行为不变

## Scope

### In

- `wallet-api/src/service/multisig_account.rs`
- `PLANS.md`

### Out

- 全量迁移 `MultisigAccountService`
- 结构体字段删除与构造签名重构
- DAO/SQL/事务语义变更

## Constraints

- Keep behavior unchanged
- Batch-by-batch compile validation
- Offline validation only

## Plan

1. Migrate `check_participant_exists` to pool + static repo helpers
2. Migrate `whether_multisig_address` to pool + static repo helper
3. Run offline checks for `wallet-database` and `wallet-api`

## Validation Commands

- `cargo check -p wallet-database --offline`
- `cargo check -p wallet-api --offline`

## Progress Checklist

- [x] Migrate check_participant_exists
- [x] Migrate whether_multisig_address
- [x] Run focused offline validation

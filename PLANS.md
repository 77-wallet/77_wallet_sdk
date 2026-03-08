# PLANS

Current task execution plan.
Refs: `docs/codex/testing.md`, `docs/codex/workflows.md`.

## Task

- Name: repoctx decoupling (batch 11: multisig_account tiny callsite cleanup)
- Goal:
  - 清理 `AccountService` 中唯一直接 `MultisigAccountRepo::new(...)` 用法
  - 保持 `MultisigAccountService` 现有结构不动，避免大改
  - 通过新增静态方法完成最小替换

## Scope

### In

- `wallet-database/src/repositories/multisig_account.rs`
- `wallet-api/src/service/account.rs`
- `PLANS.md`

### Out

- `wallet-api/src/service/multisig_account.rs` 重构
- `RepositoryFactory::multisig_account_repo` 调整
- 事务语义与 DAO 变更

## Constraints

- Keep behavior unchanged
- Batch-by-batch compile validation
- Offline validation only

## Plan

1. Add static `found_by_address_with_pool` in `MultisigAccountRepo`
2. Replace `AccountService::current_accounts` direct repo instance usage
3. Run offline checks for `wallet-database` and `wallet-api`

## Validation Commands

- `cargo check -p wallet-database --offline`
- `cargo check -p wallet-api --offline`

## Progress Checklist

- [x] Add static pool-based API in multisig_account repo
- [x] Adapt account service call site
- [x] Run focused offline validation

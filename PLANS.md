# PLANS

Current task execution plan.
Refs: `docs/codex/testing.md`, `docs/codex/workflows.md`.

## Task

- Name: repositories convergence (batch 109: remove unused account_count_with_pool)
- Goal:
  - 删除 `MultisigAccountRepo::account_count_with_pool` 无调用入口
  - 保持现有 `self.repo.account_count` 流程不变
  - 不改业务语义

## Scope

### In

- `wallet-database/src/repositories/multisig_account.rs`
- `PLANS.md`

### Out

- `&CoreDbPool` 参数统一
- `*_with_executor` 命名继续扩展
- 任何 wallet-api 接口调整

## Constraints

- Keep behavior unchanged
- Batch-by-batch compile validation
- Offline validation only

## Plan

1. 删除无调用方法 `account_count_with_pool`
2. 运行最小离线验证并停止本轮

## Validation Commands

- `cargo check -p wallet-database --offline`
- `cargo check -p wallet-api --offline`

## Progress Checklist

- [x] Remove account_count_with_pool in multisig_account repo
- [x] Run focused offline validation

# PLANS

Current task execution plan.
Refs: `docs/codex/testing.md`, `docs/codex/workflows.md`.

## Task

- Name: repositories convergence (batch 108: remove multisig *_with_pool member aliases)
- Goal:
  - 删除 `MultisigAccountRepo` 中未必要的 `_with_pool` 成员查询别名
  - 保持外部通过 Repo 访问，不直接调用 DAO
  - 不改业务语义

## Scope

### In

- `wallet-database/src/repositories/multisig_account.rs`
- `wallet-api/src/service/multisig_account.rs`
- `wallet-database/src/repositories/multisig_member.rs`
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

1. 将 `self_address_by_id_with_pool` 调用迁移到 `MultisigMemberRepo`
2. 删除仓储中的重复别名方法
3. 运行最小离线验证并停止本轮

## Validation Commands

- `cargo check -p wallet-database --offline`
- `cargo check -p wallet-api --offline`

## Progress Checklist

- [x] Replace self_address_by_id_with_pool call site via MultisigMemberRepo
- [x] Remove member/self *_with_pool aliases in multisig_account repo
- [x] Run focused offline validation

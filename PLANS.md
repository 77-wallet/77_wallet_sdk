# PLANS

Current task execution plan.
Refs: `docs/codex/testing.md`, `docs/codex/workflows.md`.

## Task

- Name: repositories convergence (batch 111: multisig_account instance alias cleanup)
- Goal:
  - 将 `self.repo.member_by_account_id / found_by_address / self_address_by_id` 调用迁移到静态入口
  - 删除 `MultisigAccountRepo` 中对应实例别名方法
  - 不改业务语义

## Scope

### In

- `wallet-database/src/repositories/multisig_account.rs`
- `wallet-api/src/service/multisig_account.rs`
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

1. 将 service 中 member/address/self-address 查询改为静态 repo 调用
2. 删除仓储中的对应实例别名方法
3. 运行最小离线验证并停止本轮

## Validation Commands

- `cargo check -p wallet-database --offline`
- `cargo check -p wallet-api --offline`

## Progress Checklist

- [x] Replace member/address/self-address instance call sites
- [x] Remove instance alias methods in repo
- [x] Run focused offline validation

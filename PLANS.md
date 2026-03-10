# PLANS

Current task execution plan.
Refs: `docs/codex/testing.md`, `docs/codex/workflows.md`.

## Task

- Name: repositories convergence (batch 106: remove multisig find_by_id_or alias)
- Goal:
  - 统一 `MultisigAccountRepo` 查询入口到 `find_by_id(pool, id)`
  - 删除重复别名 `find_by_id_or(pool, id)`
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

1. 删除无调用别名 `find_by_id_or`
2. 删除仓储中的重复别名方法
3. 运行最小离线验证并停止本轮

## Validation Commands

- `cargo check -p wallet-database --offline`
- `cargo check -p wallet-api --offline`

## Progress Checklist

- [x] Remove find_by_id_or alias in repo
- [x] Run focused offline validation

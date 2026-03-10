# PLANS

Current task execution plan.
Refs: `docs/codex/testing.md`, `docs/codex/workflows.md`.

## Task

- Name: repositories convergence (batch 107: remove multisig found_by_address_with_pool alias)
- Goal:
  - 统一地址查询入口到 `find_by_condition(pool, \"address\", address)`
  - 删除重复别名 `found_by_address_with_pool(pool, address)`
  - 不改业务语义

## Scope

### In

- `wallet-database/src/repositories/multisig_account.rs`
- `wallet-api/src/domain/multisig/account.rs`
- `wallet-api/src/service/multisig_account.rs`
- `wallet-api/src/service/account.rs`
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

1. 将 `found_by_address_with_pool` 调用点迁移到 `find_by_condition`
2. 删除仓储中的重复别名方法
3. 运行最小离线验证并停止本轮

## Validation Commands

- `cargo check -p wallet-database --offline`
- `cargo check -p wallet-api --offline`

## Progress Checklist

- [x] Replace found_by_address_with_pool call sites
- [x] Remove found_by_address_with_pool alias in repo
- [x] Run focused offline validation

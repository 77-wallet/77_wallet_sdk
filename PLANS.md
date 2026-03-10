# PLANS

Current task execution plan.
Refs: `docs/codex/testing.md`, `docs/codex/workflows.md`.

## Task

- Name: repositories convergence (batch 110: remove instance found_by_id alias)
- Goal:
  - 将 `self.repo.found_by_id` 调用统一到静态 `find_by_id(&pool, id)`
  - 删除重复实例查询入口 `found_by_id(&self, id)`
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

1. 替换 service 内 `self.repo.found_by_id` 调用
2. 删除仓储中的实例别名 `found_by_id`
3. 运行最小离线验证并停止本轮

## Validation Commands

- `cargo check -p wallet-database --offline`
- `cargo check -p wallet-api --offline`

## Progress Checklist

- [x] Replace self.repo.found_by_id call sites
- [x] Remove found_by_id instance alias in repo
- [x] Run focused offline validation

# PLANS

Current task execution plan.
Refs: `docs/codex/testing.md`, `docs/codex/workflows.md`.

## Task

- Name: repositories convergence (batch 122: bill repo test style convergence)
- Goal:
  - 将 `BillRepo` 测试中的 `NewBillEntity::new` 构造改为 `BillRepo::build_bill`
  - 保持测试语义不变

## Scope

### In

- `wallet-database/src/repositories/bill.rs`
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

1. 将 `bill_repo_create_and_get_by_hash_opt_success` 的构造改为 `BillRepo::build_bill`
2. 保持状态断言与存取逻辑不变
3. 运行最小离线验证并停止本轮

## Validation Commands

- `cargo test -p wallet-database bill_repo_create_and_get_by_hash_opt_success --offline -- --nocapture`

## Progress Checklist

- [x] Replace test constructor with BillRepo builder
- [x] Keep test behavior unchanged
- [x] Run focused offline validation

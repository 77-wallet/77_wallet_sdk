# PLANS

Current task execution plan.
Refs: `docs/codex/testing.md`, `docs/codex/workflows.md`.

## Task

- Name: repositories convergence (batch 118: replace bill from/try_from constructors)
- Goal:
  - 用 `BillRepo` 泛型 builder 替换 `NewBillEntity::from/try_from` 直接调用
  - 覆盖 `swap` 与 `domain::chain::transaction`
  - 保持行为不变

## Scope

### In

- `wallet-database/src/repositories/bill.rs`
- `wallet-api/src/service/swap.rs`
- `wallet-api/src/domain/chain/transaction.rs`
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

1. 在 `BillRepo` 增加 `build_bill_from`/`try_build_bill_from` 泛型构建入口
2. 替换 `swap/domain::chain::transaction` 的直接 `from/try_from` 调用
3. 运行最小离线验证并停止本轮

## Validation Commands

- `cargo check -p wallet-database --offline`
- `cargo check -p wallet-api --offline`

## Progress Checklist

- [x] Add BillRepo generic from/try_from builders
- [x] Replace swap/domain direct constructors
- [x] Run focused offline validation

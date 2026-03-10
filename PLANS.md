# PLANS

Current task execution plan.
Refs: `docs/codex/testing.md`, `docs/codex/workflows.md`.

## Task

- Name: repositories convergence (batch 119: remove direct NewBillEntity literal in BillDomain)
- Goal:
  - 在 `BillDomain::handle_sync_bill` 中用 `BillRepo` builder 代替 `NewBillEntity` 字面量构建
  - 保持业务语义不变

## Scope

### In

- `wallet-api/src/domain/bill.rs`
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

1. 在 `BillDomain::handle_sync_bill` 用 `BillRepo::build_bill(...)` 初始化账单
2. 保留其余字段赋值逻辑与原行为一致
3. 运行最小离线验证并停止本轮

## Validation Commands

- `cargo check -p wallet-api --offline`

## Progress Checklist

- [x] Replace direct bill literal with BillRepo builder in BillDomain
- [x] Keep handle_sync_bill behavior unchanged
- [x] Run focused offline validation

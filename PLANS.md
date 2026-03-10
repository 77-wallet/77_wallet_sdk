# PLANS

Current task execution plan.
Refs: `docs/codex/testing.md`, `docs/codex/workflows.md`.

## Task

- Name: repositories convergence (batch 121: request transaction builder convergence)
- Goal:
  - 收敛 `request/transaction` 中剩余的 `NewBillEntity` 直接构建
  - 统一通过 `BillRepo` builder 构建并保持字段语义不变

## Scope

### In

- `wallet-api/src/request/transaction/transfer.rs`
- `wallet-api/src/request/transaction/swap.rs`
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

1. `transfer.rs` 的 `TryFrom` 改为 `BillRepo::build_bill(...)` 路径
2. `swap.rs` 的 `From/TryFrom` 改为 `BillRepo::build_bill*` 路径
3. 保留原字段语义（含 `Approve` 的 `tx_type` 行为）
3. 运行最小离线验证并停止本轮

## Validation Commands

- `cargo check -p wallet-api --offline`

## Progress Checklist

- [ ] Replace direct constructors in transfer request conversions
- [ ] Replace direct constructors in swap request conversions
- [ ] Run focused offline validation

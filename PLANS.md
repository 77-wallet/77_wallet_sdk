# PLANS

Current task execution plan.
Refs: `docs/codex/testing.md`, `docs/codex/workflows.md`.

## Task

- Name: repositories convergence (batch 101: remove unused repo constructors)
- Goal:
  - 删除 `wallet-database/repositories` 中无用的 `new(_db_pool)` 构造器
  - 保持现有仓储静态 API 和业务语义不变
  - 继续按小批次收敛，避免跨 crate 扩散

## Scope

### In

- `wallet-database/src/repositories/address_book.rs`
- `wallet-database/src/repositories/bill.rs`
- `wallet-database/src/repositories/stake.rs`
- `wallet-database/src/repositories/multisig_queue.rs`
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

1. 删除无引用的 repo `new(_db_pool)` 构造器
2. 确认无调用点残留
3. 运行最小离线验证并停止本轮

## Validation Commands

- `cargo check -p wallet-database --offline`
- `cargo check -p wallet-api --offline`

## Progress Checklist

- [x] Remove unused repo constructors in wallet-database
- [x] Run focused offline validation

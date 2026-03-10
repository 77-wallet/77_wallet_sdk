# PLANS

Current task execution plan.
Refs: `docs/codex/testing.md`, `docs/codex/workflows.md`.

## Task

- Name: repositories convergence (batch 93: remove low-coupling dao type aliases)
- Goal:
  - 删除低耦合的 DAO type alias：`NewBillDao`、`CreateExpandBatchDao`、`CreateExpandBatchItemDao`、`CreateExpandNotifyStateDao`
  - 调用方直接改用对应 `*Entity` 构造
  - 不改业务语义，不扩散到仓储接口

## Scope

### In

- `wallet-database/src/dao/bill.rs`
- `wallet-database/src/repositories/bill.rs`
- `wallet-database/src/dao/expand_batch.rs`
- `wallet-database/src/repositories/api_wallet/expand_batch.rs`
- `wallet-database/src/dao/expand_batch_item.rs`
- `wallet-database/src/repositories/api_wallet/expand_batch_item.rs`
- `wallet-database/src/dao/expand_notify_state.rs`
- `wallet-database/src/repositories/api_wallet/expand_notify_state.rs`
- `PLANS.md`
- `PLANS.md`

### Out

- 命名去重/别名折叠
- `&CoreDbPool` 统一（留到后续批次）
- 跨 crate 调用点迁移

## Constraints

- Keep behavior unchanged
- Batch-by-batch compile validation
- Offline validation only

## Plan

1. 删除 4 个低耦合 DAO type alias
2. 同步调整最小调用点到 `*Entity`
3. 运行最小离线验证并停止本轮

## Validation Commands

- `cargo check -p wallet-database --offline`
- `cargo check -p wallet-api --offline`

## Progress Checklist

- [x] Remove low-coupling DAO type aliases
- [x] Run focused offline validation

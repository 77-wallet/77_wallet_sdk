# PLANS

Current task execution plan.
Refs: `docs/codex/testing.md`, `docs/codex/workflows.md`.

## Task

- Name: repositories convergence (batch 116: replace selected direct entity constructors)
- Goal:
  - 用 repo builder 替换 `NewMultisigAccountEntity::new` 调用
  - 用 repo builder 替换 `BillUpdateEntity::new` 调用
  - 保持行为不变

## Scope

### In

- `wallet-database/src/repositories/bill.rs`
- `wallet-api/src/service/multisig_account.rs`
- `wallet-api/src/service/transaction.rs`
- `wallet-api/src/service/api_wallet/transaction.rs`
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

1. 在 `BillRepo` 增加 `BillUpdateEntity` 构建入口
2. 替换 3 个 service 中的直接 `*Entity::new` 调用
3. 运行最小离线验证并停止本轮

## Validation Commands

- `cargo check -p wallet-database --offline`
- `cargo check -p wallet-api --offline`

## Progress Checklist

- [x] Add BillRepo bill-update builder
- [x] Replace selected direct entity constructors
- [x] Run focused offline validation

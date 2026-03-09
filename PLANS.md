# PLANS

Current task execution plan.
Refs: `docs/codex/testing.md`, `docs/codex/workflows.md`.

## Task

- Name: layering cleanup (batch 71: remove dao direct call in multisig created mqtt flow)
- Goal:
  - 在 `wallet-api/src/messaging/mqtt/topics/order/multisig_account/order_multisign_created.rs` 移除 `MultisigAccountDaoV1` 直接调用
  - 在 `wallet-database/src/repositories/multisig_account.rs` 增加 `update_multisig_address` repo 包装
  - 改为通过 repo 访问，避免 API 层直接依赖 dao
  - 保持行为不变，仅收敛分层依赖

## Scope

### In

- `wallet-api/src/messaging/mqtt/topics/order/multisig_account/order_multisign_created.rs`
- `wallet-database/src/repositories/multisig_account.rs`
- `PLANS.md`

### Out

- 其他 domain/service/messaging 模块
- repository/dao 结构性重构
- 事务模型变更

## Constraints

- Keep behavior unchanged
- Batch-by-batch compile validation
- Offline validation only

## Plan

1. 给 `MultisigAccountRepo` 增加 `update_multisig_address` 包装并替换 mqtt flow 调用
2. 运行最小离线验证并停止本轮

## Validation Commands

- `cargo test -p wallet-database bill_repo --offline -- --nocapture`
- `cargo test -p wallet-database multisig_account_repo --offline -- --nocapture`
- `cargo check -p wallet-database --offline`
- `cargo check -p wallet-api --offline`

## Progress Checklist

- [x] Replace direct `MultisigAccountDaoV1` usage in mqtt flow
- [x] Run focused offline validation

# PLANS

Current task execution plan.
Refs: `docs/codex/testing.md`, `docs/codex/workflows.md`.

## Task

- Name: layering cleanup (batch 82: replace dao direct calls in service/multisig_transaction.rs)
- Goal:
  - 在 `wallet-api/src/service/multisig_transaction.rs` 移除 `multisig_queue/multisig_member` DAO 直接调用
  - 在 `wallet-database` 增加最小 repo 包装（过期更新与回滚）
  - 保持行为不变，仅替换调用层级与依赖方向

## Scope

### In

- `wallet-api/src/service/multisig_transaction.rs`
- `wallet-database/src/repositories/multisig_queue.rs`
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

1. 在 `MultisigQueueRepo` 增加最小包装（`update_expired_queue`/`rollback_update_fail`）
2. 替换 `service/multisig_transaction.rs` 中 DAO 直调为 repo 调用
2. 运行最小离线验证并停止本轮

## Validation Commands

- `cargo test -p wallet-database multisig_queue_repo --offline -- --nocapture`
- `cargo check -p wallet-api --offline`

## Progress Checklist

- [x] Replace direct dao usage in multisig transaction flow
- [x] Run focused offline validation

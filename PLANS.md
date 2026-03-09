# PLANS

Current task execution plan.
Refs: `docs/codex/testing.md`, `docs/codex/workflows.md`.

## Task

- Name: layering cleanup (batch 74: replace dao direct calls in domain/multisig/queue.rs)
- Goal:
  - 在 `wallet-api/src/domain/multisig/queue.rs` 移除 `multisig_account/multisig_member/multisig_queue` DAO 直接调用
  - 在 `wallet-database` 增加最小 repo 包装（仅本文件需要的方法）
  - 保持行为不变，仅替换调用层级与依赖方向

## Scope

### In

- `wallet-api/src/domain/multisig/queue.rs`
- `wallet-database/src/repositories/multisig_account.rs`
- `wallet-database/src/repositories/multisig_queue.rs`
- `wallet-database/src/repositories/multisig_member.rs`
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

1. 补 `MultisigQueueRepo` 所需最小包装（`list_by_account_ids`）
2. 替换 `domain/multisig/queue.rs` 对应 DAO 直调为 repo 调用
3. 运行最小离线验证并停止本轮

## Validation Commands

- `cargo test -p wallet-database multisig_queue_repo --offline -- --nocapture`
- `cargo check -p wallet-database --offline`
- `cargo check -p wallet-api --offline`

## Progress Checklist

- [x] Replace direct dao usage in domain multisig queue flow
- [x] Run focused offline validation

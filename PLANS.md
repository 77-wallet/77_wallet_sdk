# PLANS

Current task execution plan.
Refs: `docs/codex/testing.md`, `docs/codex/workflows.md`.

## Task

- Name: layering cleanup (batch 60: remove multisig queue dao alias usage)
- Goal:
  - 在 `wallet-api` multisig queue flow 移除 `NewMultisigQueueDao/NewSignatureDao` 直接调用
  - 统一通过 `MultisigQueueRepo` 暴露构造 helper 生成队列和签名实体
  - 保持行为不变，仅收敛分层依赖

## Scope

### In

- `wallet-database/src/repositories/multisig_queue.rs`
- `wallet-api/src/domain/multisig/queue.rs`
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

1. 在 `MultisigQueueRepo` 增加构造 helper（队列转换 + 签名构造）
2. 将 `domain/multisig/queue.rs` 中 `NewMultisigQueueDao/NewSignatureDao` 替换为 repo helper
3. 为新增 helper 补最小单元测试（队列映射 + 签名构造路径）
4. 运行最小离线验证并停止本轮

## Validation Commands

- `cargo test -p wallet-database multisig_queue_repo --offline -- --nocapture`
- `cargo check -p wallet-database --offline`
- `cargo check -p wallet-api --offline`

## Progress Checklist

- [x] Add `MultisigQueueRepo` constructor helpers
- [x] Replace `NewMultisigQueueDao/NewSignatureDao` usage in flow
- [x] Add minimal tests for constructor helpers
- [x] Run focused offline validation

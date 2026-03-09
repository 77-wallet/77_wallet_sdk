# PLANS

Current task execution plan.
Refs: `docs/codex/testing.md`, `docs/codex/workflows.md`.

## Task

- Name: layering cleanup (batch 83: remove leftover multisig dao imports in api_wallet service)
- Goal:
  - 清理 `wallet-api/src/service/api_wallet/wallet.rs` 中残留的 `dao::multisig_*` import
  - 保持行为不变，仅做无用依赖收口
  - 保持行为不变，仅替换调用层级与依赖方向

## Scope

### In

- `wallet-api/src/service/api_wallet/wallet.rs`
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

1. 移除无用 `dao::multisig_member` import
2. 移除无用 `MultisigDomain` import
2. 运行最小离线验证并停止本轮

## Validation Commands

- `cargo test -p wallet-database multisig_queue_repo --offline -- --nocapture`
- `cargo check -p wallet-api --offline`

## Progress Checklist

- [x] Remove leftover multisig dao imports
- [x] Run focused offline validation

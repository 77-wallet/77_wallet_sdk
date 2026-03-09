# PLANS

Current task execution plan.
Refs: `docs/codex/testing.md`, `docs/codex/workflows.md`.

## Task

- Name: layering cleanup (batch 72: replace multisig member/signature dao direct calls)
- Goal:
  - 在 `wallet-api/src/domain/multisig/account.rs` 移除 `multisig_member/multisig_signatures` DAO 直接调用
  - 在 `wallet-database` 增加最小 repo 包装（member + signature）
  - 保持行为不变，仅替换调用层级
  - 保持行为不变，仅收敛分层依赖

## Scope

### In

- `wallet-api/src/domain/multisig/account.rs`
- `wallet-database/src/repositories/multisig_member.rs`
- `wallet-database/src/repositories/multisig_signature.rs`
- `wallet-database/src/repositories/mod.rs`
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

1. 补 `MultisigMemberRepo` / `MultisigSignatureRepo` 所需最小包装
2. 替换 `domain/multisig/account.rs` 对应 DAO 直调为 repo 调用
2. 运行最小离线验证并停止本轮

## Validation Commands

- `cargo test -p wallet-database multisig_member_repo --offline -- --nocapture`
- `cargo check -p wallet-database --offline`
- `cargo check -p wallet-api --offline`

## Progress Checklist

- [x] Replace direct multisig member/signature dao usage in domain multisig account flow
- [x] Run focused offline validation

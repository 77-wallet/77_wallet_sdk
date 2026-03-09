# PLANS

Current task execution plan.
Refs: `docs/codex/testing.md`, `docs/codex/workflows.md`.

## Task

- Name: layering cleanup (batch 63: remove dao alias usage in multisig domain account)
- Goal:
  - 在 `wallet-api/src/domain/multisig/account.rs` 移除 `NewMultisigAccountDao::new` 直接调用
  - 统一通过 `MultisigAccountRepo::build_new_account` 构造实体
  - 保持行为不变，仅收敛分层依赖
  - 保持行为不变，仅收敛分层依赖

## Scope

### In

- `wallet-database/src/repositories/multisig_account.rs`
- `wallet-api/src/domain/multisig/account.rs`
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

1. 在 `MultisigAccountRepo` 增加 `build_new_account` helper
2. 将 `domain/multisig/account.rs` 中 `NewMultisigAccountDao::new` 替换为 repo helper
3. 为新增 helper 补最小单测（成员映射 + self 标记）
4. 运行最小离线验证并停止本轮

## Validation Commands

- `cargo test -p wallet-database bill_repo --offline -- --nocapture`
- `cargo test -p wallet-database multisig_account_repo --offline -- --nocapture`
- `cargo check -p wallet-database --offline`
- `cargo check -p wallet-api --offline`

## Progress Checklist

- [x] Add `MultisigAccountRepo::build_new_account` helper
- [x] Replace `NewMultisigAccountDao::new` usage in domain flow
- [x] Add minimal tests for helper
- [x] Run focused offline validation

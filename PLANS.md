# PLANS

Current task execution plan.
Refs: `docs/codex/testing.md`, `docs/codex/workflows.md`.

## Task

- Name: layering cleanup (batch 77: replace dao direct call in domain/bill.rs)
- Goal:
  - 在 `wallet-api/src/domain/bill.rs` 移除 `multisig_account` DAO 直接调用
  - 在 `wallet-database` 增加最小 `find_by_conditions` repo 包装
  - 保持行为不变，仅替换调用层级与依赖方向

## Scope

### In

- `wallet-api/src/domain/bill.rs`
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

1. 在 `MultisigAccountRepo` 增加最小多条件查找包装
2. 替换 `domain/bill.rs` 中 DAO 直调为 repo 调用
2. 运行最小离线验证并停止本轮

## Validation Commands

- `cargo test -p wallet-database multisig_queue_repo --offline -- --nocapture`
- `cargo check -p wallet-api --offline`

## Progress Checklist

- [x] Replace direct dao usage in domain bill flow
- [x] Run focused offline validation

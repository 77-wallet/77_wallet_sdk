# PLANS

Current task execution plan.
Refs: `docs/codex/testing.md`, `docs/codex/workflows.md`.

## Task

- Name: layering cleanup (batch 61: shrink dao alias usage in multisig transaction service)
- Goal:
  - 在 `wallet-api/src/service/multisig_transaction.rs` 移除 `New*Dao` 直接调用
  - `NewSignatureDao`/`NewBillDao` 改走 `MultisigQueueRepo`/`BillRepo` helper
  - `NewMultisigQueueDao::from` 改为 `NewMultisigQueueEntity::from`
  - 保持行为不变，仅收敛分层依赖

## Scope

### In

- `wallet-database/src/repositories/bill.rs`
- `wallet-api/src/service/multisig_transaction.rs`
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

1. 在 `BillRepo` 增加创建账单 helper（`build_signed_bill`、`build_bill`）
2. 将 `multisig_transaction.rs` 中 `NewSignatureDao`/`NewBillDao` 替换为 repo helper
3. 将 `NewMultisigQueueDao::from` 改为 `NewMultisigQueueEntity::from`
4. 为新增 `BillRepo` helper 补最小测试并运行最小离线验证

## Validation Commands

- `cargo test -p wallet-database bill_repo --offline -- --nocapture`
- `cargo check -p wallet-database --offline`
- `cargo check -p wallet-api --offline`

## Progress Checklist

- [x] Add `BillRepo` constructor helpers
- [x] Replace `NewSignatureDao`/`NewBillDao` usages in service flow
- [x] Replace `NewMultisigQueueDao::from` with entity conversion
- [x] Run focused offline validation

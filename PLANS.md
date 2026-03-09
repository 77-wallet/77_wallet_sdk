# PLANS

Current task execution plan.
Refs: `docs/codex/testing.md`, `docs/codex/workflows.md`.

## Task

- Name: layering cleanup (batch 62: shrink dao alias usage in multisig account service)
- Goal:
  - 在 `wallet-api/src/service/multisig_account.rs` 移除 `New*Dao` 直接调用
  - `NewBillDao::new_deploy_bill` 改走 `BillRepo` helper
  - `NewMultisigAccountDao::new` 改为 `NewMultisigAccountEntity::new`
  - 保持行为不变，仅收敛分层依赖

## Scope

### In

- `wallet-database/src/repositories/bill.rs`
- `wallet-api/src/service/multisig_account.rs`
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

1. 在 `BillRepo` 增加 `build_deploy_bill` helper
2. 将 `multisig_account.rs` 中 `NewBillDao::new_deploy_bill` 替换为 repo helper
3. 将 `NewMultisigAccountDao::new` 改为 `NewMultisigAccountEntity::new`
4. 为新增 helper 补最小测试并运行最小离线验证

## Validation Commands

- `cargo test -p wallet-database bill_repo --offline -- --nocapture`
- `cargo check -p wallet-database --offline`
- `cargo check -p wallet-api --offline`

## Progress Checklist

- [x] Add `BillRepo::build_deploy_bill` helper
- [x] Replace `NewBillDao::new_deploy_bill` usage in service flow
- [x] Replace `NewMultisigAccountDao::new` with entity constructor
- [x] Run focused offline validation

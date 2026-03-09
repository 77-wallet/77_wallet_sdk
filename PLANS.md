# PLANS

Current task execution plan.
Refs: `docs/codex/testing.md`, `docs/codex/workflows.md`.

## Task

- Name: layering cleanup (batch 47: transaction service use Repo in tx path)
- Goal:
  - `wallet-api/src/service/transaction.rs` 不再直接调用 `AssetsEntity::*`
  - 为事务路径补充 `AssetsRepo` 的 `tx` 版本接口并替换调用
  - 保持行为不变，仅收敛调用分层

## Scope

### In

- `wallet-database/src/repositories/assets.rs`
- `wallet-api/src/service/transaction.rs`
- `PLANS.md`

### Out

- 其他 service/domain 模块
- repository/dao 结构变更
- 事务模型变更

## Constraints

- Keep behavior unchanged
- Batch-by-batch compile validation
- Offline validation only

## Plan

1. 在 `AssetsRepo` 增加事务内更新余额接口（`update_balance_tx`）
2. 替换 `transaction.rs` 中 `AssetsEntity::update_balance` 为 `AssetsRepo::update_balance_tx`
3. 清理不再需要的 `AssetsEntity` import
4. 运行离线编译校验（`wallet-database` + `wallet-api`)

## Validation Commands

- `cargo check -p wallet-database --offline`
- `cargo check -p wallet-api --offline`

## Progress Checklist

- [x] Add tx-path method in AssetsRepo
- [x] Replace direct AssetsEntity usage in transaction service
- [x] Run focused offline validation

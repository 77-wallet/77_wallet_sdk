# PLANS

Current task execution plan.
Refs: `docs/codex/testing.md`, `docs/codex/workflows.md`.

## Task

- Name: layering cleanup (batch 50: multisig account domain use repos)
- Goal:
  - `wallet-api/src/domain/multisig/account.rs` 不再直接调用 `Entity::*`
  - 改为 `AssetsRepo/AccountRepo/WalletRepo` 提供同语义调用
  - 保持行为不变，仅收敛调用分层

## Scope

### In

- `wallet-api/src/domain/multisig/account.rs`
- `wallet-database/src/repositories/assets.rs`
- `wallet-database/src/repositories/account.rs`
- `PLANS.md`

### Out

- 其他 service/domain 模块
- repository/dao 结构性重构
- 事务模型变更

## Constraints

- Keep behavior unchanged
- Batch-by-batch compile validation
- Offline validation only

## Plan

1. 在 `AssetsRepo` 增加当前 `multisig account domain` 缺失的薄封装方法
2. 替换 `domain/multisig/account.rs` 中 `Entity::*` 直调为 repo 调用
3. 清理不再需要的 entity import
3. 运行离线编译校验（`wallet-database` + `wallet-api`)

## Validation Commands

- `cargo check -p wallet-database --offline`
- `cargo check -p wallet-api --offline`

## Progress Checklist

- [x] Add minimal repo wrappers for multisig-account-domain call sites
- [x] Replace direct entity calls in multisig account domain
- [x] Remove stale imports
- [x] Run focused offline validation

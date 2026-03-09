# PLANS

Current task execution plan.
Refs: `docs/codex/testing.md`, `docs/codex/workflows.md`.

## Task

- Name: layering cleanup (batch 53: clear all remaining entity direct calls)
- Goal:
  - 清理 `wallet-api` 剩余所有 `Entity::*` 直接调用
  - 统一改为 `Repo` 调用，行为保持不变
  - 保持行为不变，仅收敛调用分层

## Scope

### In

- `wallet-api/src/service/multisig_account.rs`
- `wallet-api/src/domain/bill.rs`
- `wallet-api/src/domain/permission.rs`
- `wallet-api/src/domain/multisig/queue.rs`
- `wallet-api/src/messaging/mqtt/topics/order/permission.rs`
- `wallet-database/src/repositories/permission.rs`
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

1. 替换剩余 5 个业务文件中的 `Entity::*` 直调为 repo 调用
2. 在 `PermissionRepo` 增加最小 helper，避免消息层直接依赖 `PermissionEntity::get_id`
3. 清理不再需要的 entity import
3. 运行离线编译校验（`wallet-database` + `wallet-api`)

## Validation Commands

- `cargo check -p wallet-database --offline`
- `cargo check -p wallet-api --offline`

## Progress Checklist

- [x] Replace all remaining direct entity calls in wallet-api
- [x] Add minimal repo helper for permission id generation
- [x] Remove stale imports
- [x] Run focused offline validation

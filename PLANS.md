# PLANS

Current task execution plan.
Refs: `docs/codex/testing.md`, `docs/codex/workflows.md`.

## Task

- Name: layering cleanup (batch 48: swap service use AccountRepo)
- Goal:
  - `wallet-api/src/service/swap.rs` 不再直接调用 `AccountEntity::*`
  - 改为 `AccountRepo` 提供同语义查询
  - 保持行为不变，仅收敛调用分层

## Scope

### In

- `wallet-api/src/service/swap.rs`
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

1. 替换 `swap.rs` 的 `AccountEntity::lists_by_wallet_address` 为 `AccountRepo` 调用
2. 清理不再需要的 `AccountEntity` import
3. 运行离线编译校验（`wallet-database` + `wallet-api`)

## Validation Commands

- `cargo check -p wallet-database --offline`
- `cargo check -p wallet-api --offline`

## Progress Checklist

- [x] Replace direct AccountEntity usage in swap service
- [x] Remove stale imports
- [x] Run focused offline validation

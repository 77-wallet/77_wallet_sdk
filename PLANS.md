# PLANS

Current task execution plan.
Refs: `docs/codex/testing.md`, `docs/codex/workflows.md`.

## Task

- Name: repositories convergence (batch 97: impl back to dao for system_notification/chain)
- Goal:
  - 将 `system_notification` 与 `chain` 的 `impl *Entity` 收口为 `impl *Dao`
  - 保持实体作为返回值，不恢复 type alias
  - 不改业务语义

## Scope

### In

- `wallet-database/src/dao/system_notification.rs`
- `wallet-database/src/repositories/system_notification.rs`
- `wallet-database/src/dao/chain.rs`
- `wallet-database/src/repositories/chain.rs`
- `PLANS.md`
- `PLANS.md`

### Out

- 命名去重/别名折叠
- `&CoreDbPool` 统一（留到后续批次）
- 跨 crate 调用点迁移

## Constraints

- Keep behavior unchanged
- Batch-by-batch compile validation
- Offline validation only

## Plan

1. 将 `system_notification/chain` 的 `impl *Entity` 改为 `impl *Dao`
2. 同步 repo 调用到 `*Dao`
3. 运行最小离线验证并停止本轮

## Validation Commands

- `cargo check -p wallet-database --offline`
- `cargo check -p wallet-api --offline`

## Progress Checklist

- [ ] Convert system_notification/chain DAO impls from Entity to Dao
- [ ] Run focused offline validation

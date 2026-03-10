# PLANS

Current task execution plan.
Refs: `docs/codex/testing.md`, `docs/codex/workflows.md`.

## Task

- Name: repositories convergence (batch 96: impl back to dao for touched modules)
- Goal:
  - 将已触达模块的 `impl *Entity` 收口为 `impl *Dao`
  - 保持实体作为返回值，不恢复 type alias
  - 不改业务语义

## Scope

### In

- `wallet-database/src/dao/announcement.rs`
- `wallet-database/src/repositories/announcement.rs`
- `wallet-database/src/dao/node.rs`
- `wallet-database/src/repositories/node.rs`
- `wallet-database/src/dao/exchange_rate.rs`
- `wallet-database/src/repositories/exchange_rate.rs`
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

1. 将三处 `impl *Entity` 改为 `impl *Dao`
2. 同步 repo 调用到 `*Dao`
3. 运行最小离线验证并停止本轮

## Validation Commands

- `cargo check -p wallet-database --offline`
- `cargo check -p wallet-api --offline`

## Progress Checklist

- [x] Convert touched DAO impls from Entity to Dao
- [x] Run focused offline validation

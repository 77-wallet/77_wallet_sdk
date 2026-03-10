# PLANS

Current task execution plan.
Refs: `docs/codex/testing.md`, `docs/codex/workflows.md`.

## Task

- Name: repositories convergence (batch 102: stake dao struct convergence)
- Goal:
  - 将 `dao/stake.rs` 从模块函数收口为 `StakeDao` 结构
  - 保持 `StakeRepo` 对外 API 与业务语义不变
  - 为后续“DAO 统一入口”继续打底

## Scope

### In

- `wallet-database/src/dao/stake.rs`
- `wallet-database/src/repositories/stake.rs`
- `PLANS.md`

### Out

- `&CoreDbPool` 参数统一
- `*_with_executor` 命名继续扩展
- 任何 wallet-api 接口调整

## Constraints

- Keep behavior unchanged
- Batch-by-batch compile validation
- Offline validation only

## Plan

1. 将 `stake` 模块函数迁移到 `StakeDao` 关联函数
2. 同步 `StakeRepo` 使用 `StakeDao::*`
3. 运行最小离线验证并停止本轮

## Validation Commands

- `cargo check -p wallet-database --offline`
- `cargo check -p wallet-api --offline`

## Progress Checklist

- [x] Convert stake dao free functions into StakeDao
- [x] Run focused offline validation

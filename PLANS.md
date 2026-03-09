# PLANS

Current task execution plan.
Refs: `docs/codex/testing.md`, `docs/codex/workflows.md`.

## Task

- Name: layering cleanup (batch 45: coin service use Repo instead of Entity)
- Goal:
  - `wallet-api/src/service/coin.rs` 不再直接调用 `AssetsEntity::*`
  - 全部改为通过 `AssetsRepo` 调用
  - 保持行为不变，仅收敛调用分层

## Scope

### In

- `wallet-api/src/service/coin.rs`
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

1. 替换 `coin.rs` 中全部 `AssetsEntity::*` 调用为 `AssetsRepo::*`
2. 清理不再需要的 `AssetsEntity` import
3. 运行离线编译校验（`wallet-database` + `wallet-api`)

## Validation Commands

- `cargo check -p wallet-database --offline`
- `cargo check -p wallet-api --offline`

## Progress Checklist

- [x] Replace direct AssetsEntity usage in coin service
- [x] Remove stale imports
- [x] Run focused offline validation

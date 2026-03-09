# PLANS

Current task execution plan.
Refs: `docs/codex/testing.md`, `docs/codex/workflows.md`.

## Task

- Name: layering cleanup (batch 44: asset service use Repo instead of Entity)
- Goal:
  - `wallet-api` 资产服务层不再直接调用 `AssetsEntity::*`
  - 通过 `wallet-database::repositories::assets::AssetsRepo` 暴露所需接口
  - 保持行为不变，仅收敛调用分层

## Scope

### In

- `wallet-database/src/repositories/assets.rs`
- `wallet-api/src/service/asset.rs`
- `PLANS.md`

### Out

- 其他 service/domain 模块
- DAO SQL 逻辑变更
- 事务模型变更

## Constraints

- Keep behavior unchanged
- Batch-by-batch compile validation
- Offline validation only

## Plan

1. 在 `AssetsRepo` 增加 `asset service` 所需静态方法
2. 将 `wallet-api/src/service/asset.rs` 中 `AssetsEntity::*` 调用替换为 `AssetsRepo::*`
3. 保持函数签名与业务行为不变
4. 运行离线编译校验（`wallet-database` + `wallet-api`）

## Validation Commands

- `cargo check -p wallet-database --offline`
- `cargo check -p wallet-api --offline`

## Progress Checklist

- [x] Add missing AssetsRepo methods for asset service
- [x] Replace direct AssetsEntity usage in asset service
- [x] Run focused offline validation

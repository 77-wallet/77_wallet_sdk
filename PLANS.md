# PLANS

Current task execution plan.
Refs: `docs/codex/testing.md`, `docs/codex/workflows.md`.

## Task

- Name: repoctx decoupling (batch 27: assets domain read-path decoupling)
- Goal:
  - 将 `AssetsDomain` 两个读方法从 `RepoCtx` 参数改为 `CoreDbPool`
  - 仅收敛读路径，写路径与事务语义保持不变
  - 为后续 `RepoCtx` 进一步瘦身建立过渡

## Scope

### In

- `wallet-api/src/domain/assets/mod.rs`
- `wallet-api/src/service/asset.rs`
- `wallet-api/src/service/wallet.rs`
- `PLANS.md`

### Out

- `RepoCtx` 结构变更
- DAO/SQL/事务变更
- assets 写路径重构

## Constraints

- Keep behavior unchanged
- Batch-by-batch compile validation
- Offline validation only

## Plan

1. Change `AssetsDomain::get_account_assets_entity/get_local_coin_list` to use `CoreDbPool`
2. Update `service/asset.rs` and `service/wallet.rs` call sites
3. Keep logic unchanged and run offline checks for `wallet-database` and `wallet-api`

## Validation Commands

- `cargo check -p wallet-database --offline`
- `cargo check -p wallet-api --offline`

## Progress Checklist

- [x] Decouple assets domain read methods from RepoCtx
- [x] Keep behavior unchanged
- [x] Run focused offline validation

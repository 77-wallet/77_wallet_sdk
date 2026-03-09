# PLANS

Current task execution plan.
Refs: `docs/codex/testing.md`, `docs/codex/workflows.md`.

## Task

- Name: repoctx decoupling (batch 33: shrink RepoCtx usage in assets service read paths)
- Goal:
  - 在 `AssetsService` 中将纯查询路径改为 `AssetsEntity` 直连池调用
  - 不改写路径、不改事务边界，只收敛读路径的 `RepoCtx` 依赖
  - 保持行为不变，不触碰事务语义

## Scope

### In

- `wallet-api/src/service/account.rs`
- `wallet-api/src/service/wallet.rs`
- `wallet-api/src/service/asset.rs`
- `PLANS.md`

### Out

- `RepoCtx` 结构变更
- DAO/SQL/事务变更
- service 结构体签名变更
- coin 业务逻辑变更

## Constraints

- Keep behavior unchanged
- Batch-by-batch compile validation
- Offline validation only

## Plan

1. Replace `RepoCtx` reads in `get_multisig_account_assets` / `detail` / `get_all_account_assets`
2. Keep write/remove logic untouched for this batch
3. Keep logic unchanged and run offline checks for `wallet-database` and `wallet-api`

## Validation Commands

- `cargo check -p wallet-database --offline`
- `cargo check -p wallet-api --offline`

## Progress Checklist

- [x] Shrink RepoCtx usage in assets read paths
- [x] Keep behavior unchanged
- [x] Run focused offline validation

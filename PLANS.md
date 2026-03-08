# PLANS

Current task execution plan.
Refs: `docs/codex/testing.md`, `docs/codex/workflows.md`.

## Task

- Name: repoctx decoupling (batch 26: remove RepositoryFactory::repo callsites)
- Goal:
  - 将 `wallet-api` 中所有 `RepositoryFactory::repo(...)` 替换为 `RepoCtx::new(...)`
  - 移除对 `wallet_database::factory` 的真实运行时依赖
  - 不改业务逻辑与事务语义

## Scope

### In

- `wallet-api/src/api/asset.rs`
- `wallet-api/src/api/account.rs`
- `wallet-api/src/api/wallet.rs`
- `wallet-api/src/api/phrase.rs`
- `wallet-api/src/data.rs`
- `wallet-api/src/domain/coin/mod.rs`
- `wallet-api/src/service/coin.rs`
- `PLANS.md`

### Out

- `RepoCtx` 行为变更
- DAO/SQL/事务变更
- `wallet-database` repository 结构变更

## Constraints

- Keep behavior unchanged
- Batch-by-batch compile validation
- Offline validation only

## Plan

1. Replace `RepositoryFactory::repo(...)` with `RepoCtx::new(...)` in target files
2. Adjust imports to use `wallet_database::repositories::RepoCtx`
3. Run offline checks for `wallet-database` and `wallet-api`

## Validation Commands

- `cargo check -p wallet-database --offline`
- `cargo check -p wallet-api --offline`

## Progress Checklist

- [x] Replace RepositoryFactory::repo callsites
- [x] Keep behavior unchanged
- [x] Run focused offline validation

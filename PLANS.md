# PLANS

Current task execution plan.
Refs: `docs/codex/testing.md`, `docs/codex/workflows.md`.

## Task

- Name: remove repo traits from chain service (batch 2)
- Goal:
  - 清理 `wallet-api/src/service/chain.rs` 中剩余 `CoinRepoTrait/AssetsRepoTrait` 调用
  - 保持静态 repo 调用模式，减少对 `RepoCtx + trait` 的显式依赖
  - 保持业务语义不变

## Scope

### In

- `wallet-database/src/repositories/{coin,assets}.rs`
- `wallet-api/src/service/chain.rs`
- `PLANS.md`

### Out

- `wallet-api` 其他 service/domain 的 trait 全量迁移
- DAO/SQL 语义改动
- 连接池/锁治理策略改动

## Constraints

- Keep behavior unchanged
- Small reversible patch set
- Offline validation only

## Plan

1. Add static APIs in `CoinRepo`/`AssetsRepo` for chain service use-cases
2. Refactor `chain` service to call static APIs directly
3. Remove unused trait imports from chain service
4. Run focused offline validation

## Validation Commands

- `cargo check -p wallet-database --offline`
- `cargo check -p wallet-api --offline`

## Progress Checklist

- [x] Add static APIs in coin/assets repos
- [x] Refactor chain service call sites
- [x] Remove trait imports in chain service
- [x] Run focused offline validation

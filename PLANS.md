# PLANS

Current task execution plan.
Refs: `docs/codex/testing.md`, `docs/codex/workflows.md`.

## Task

- Name: repoctx decoupling (batch 6: chain service path)
- Goal:
  - 将 `ChainService` 从显式 `RepoCtx` 依赖中解耦
  - 保持业务行为不变（仅类型与调用路径收敛）
  - 继续以最小调用点替换推进

## Scope

### In

- `wallet-api/src/service/chain.rs`
- `wallet-api/src/api/chain.rs`
- `PLANS.md`

### Out

- `CoinService/AssetsService/AccountService/WalletService` 的大改
- DAO/SQL 与事务语义变更
- 锁治理/连接池策略调整

## Constraints

- Keep behavior unchanged
- Batch-by-batch compile validation
- Offline validation only

## Plan

1. Refactor `ChainService` to zero-field service (`new()`)
2. Adapt chain API call sites to new constructor
3. Keep method behavior unchanged
4. Run offline checks for `wallet-api` and `wallet-database`

## Validation Commands

- `cargo check -p wallet-database --offline`
- `cargo check -p wallet-api --offline`
- `cargo test -p wallet-database system_notification_repo_list_and_detail_work_without_explicit_transaction --offline -- --nocapture`

## Progress Checklist

- [x] Decouple ChainService
- [x] Adapt chain call sites
- [x] Keep method behavior unchanged
- [x] Run focused offline validation

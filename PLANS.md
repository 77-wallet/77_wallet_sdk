# PLANS

Current task execution plan.
Refs: `docs/codex/testing.md`, `docs/codex/workflows.md`.

## Task

- Name: repoctx decoupling (batch 5: node + exchange rate)
- Goal:
  - 将 `NodeService`、`ExchangeRateService` 从显式 `RepoCtx` 依赖中解耦
  - 保持业务行为不变（仅类型与调用路径收敛）
  - 继续以“最小调用点替换”方式推进

## Scope

### In

- `wallet-api/src/service/node.rs`
- `wallet-api/src/service/exchange_rate.rs`
- `wallet-api/src/api/node.rs`
- `wallet-api/src/messaging/mqtt/topics/token_price.rs`
- `wallet-api/src/infrastructure/task_queue/task_handle/backend_handle.rs`（仅 exchange rate 调用点）
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

1. Refactor `NodeService` and `ExchangeRateService` to zero-field services (`new()`)
2. Adapt direct call sites to new constructors
3. Keep method semantics unchanged
4. Run offline checks for `wallet-api` and `wallet-database`

## Validation Commands

- `cargo check -p wallet-database --offline`
- `cargo check -p wallet-api --offline`
- `cargo test -p wallet-database system_notification_repo_list_and_detail_work_without_explicit_transaction --offline -- --nocapture`

## Progress Checklist

- [x] Decouple NodeService and ExchangeRateService
- [x] Adapt node/exchange-rate call sites
- [x] Keep method behavior unchanged
- [x] Run focused offline validation

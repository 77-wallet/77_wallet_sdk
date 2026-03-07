# PLANS

Current task execution plan.  
Refs: `docs/codex/testing.md`, `docs/codex/workflows.md`.

## Task

- Name: wallet-database repositories transaction-surface tighten (batch 2)
- Goal:
  - 收敛 `wallet-database/src/repositories/mod.rs` 的事务接口暴露面
  - 只做最小调用点迁移，不改 DAO 语义，不扩散到 `wallet-api`
  - 维持离线可编译并补最小回归护栏

## Scope

### In

- `wallet-database/src/repositories/mod.rs`
- `wallet-database/src/repositories/announcement.rs`
- `wallet-database/src/repositories/system_notification.rs`
- 受影响最小测试
- `PLANS.md`

### Out

- SQLite 连接池策略和并发锁治理实现
- `sql_utils` 二次重构
- 其他 repository/DAO 的顺手改造
- `wallet-api` 兼容层改动

## Constraints

- Keep business semantics unchanged
- Test-first for touched flow boundaries
- Offline validation only
- Small, reversible patch set

## Plan

1. Add minimal tests around repo trait transaction usage for touched paths
2. Remove internal transaction mutability accessors from `TransactionTrait`
3. Keep trait contract minimal around transaction lifecycle + executor access
4. Defer `get_db_pool()` migration for pagination-style DAO calls to a dedicated small batch
5. Run focused check/test commands and stop if compile surface grows unexpectedly

## Validation Commands

- `cargo check -p wallet-database --offline`
- `cargo test -p wallet-database announcement --offline -- --nocapture`
- `cargo test -p wallet-database system_notification --offline -- --nocapture`

## Expected Results

- `TransactionTrait` 不再泄漏 `db_pool` 与 transaction 内部可变访问接口
- 受影响仓储查询/写入路径继续工作，语义不变
- `wallet-database` 离线编译通过

## Progress Checklist

- [x] Add minimal tests for touched repository flows
- [x] Tighten `TransactionTrait` surface
- [x] Run focused offline validation
- [ ] Migrate direct `get_db_pool()` call sites (deferred: DAO pagination signatures currently pool-bound)

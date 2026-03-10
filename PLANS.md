# PLANS

Current task execution plan.
Refs: `docs/codex/testing.md`, `docs/codex/workflows.md`.

## Task

- Name: SQLite read/write split (Batch 2B: api_wallet chain/coin/strategy)
- Goal:
  - 在 `api_wallet` 子模块继续推进 6 个仓库的读写路由
  - 继续保持上层接口不变、业务语义不变、schema 不变

## Scope

### In

- `wallet-database/src/repositories/api_wallet/chain.rs`
- `wallet-database/src/repositories/api_wallet/coin.rs`
- `wallet-database/src/repositories/api_wallet/collect_strategy.rs`
- `wallet-database/src/repositories/api_wallet/withdraw_strategy.rs`
- `wallet-database/src/repositories/api_wallet/collect_strategy_chain_config.rs`
- `wallet-database/src/repositories/api_wallet/withdraw_strategy_chain_config.rs`
- `PLANS.md`

### Out

- `api_wallet` 其余仓库（Batch 2B/2C）
- `core/task` 与遗留写路径清理（Batch 3）
- `sql_utils` 结构重构（后置小批）
- `wallet-api` 对外接口签名改造

## Constraints

- 单批仅 `wallet-database` + `api_funds` 热点 flow
- 默认 `as_ref()` 映射 reader；写路径显式 `write_ref()`
- 事务统一从 writer 侧开始（`write_ref().begin()` 或 writer pool 等价入口）
- 先保证最小可验证闭环，避免跨模块扩散

## Plan

1. 在 `chain/coin/strategy/strategy_chain_config` 中将写操作和事务入口统一切到 writer
2. 保持读查询走 reader，避免写语义误路由
3. 跑最小离线验证并记录结果，失败仅做本批内修复

## Validation Commands

- `cargo check -p wallet-database --offline`
- `cargo test -p wallet-database chain_repo_ --offline -- --nocapture`
- `cargo test -p wallet-database coin_repo_ --offline -- --nocapture`
- `cargo test -p wallet-database collect_strategy_repo_ --offline -- --nocapture`
- `cargo test -p wallet-database withdraw_strategy_repo_ --offline -- --nocapture`
- `cargo test -p wallet-database strategy_chain_config_repo_ --offline -- --nocapture`

## Progress Checklist

- [x] chain/coin/strategy write paths route to writer
- [x] chain/coin/strategy tests align with writer transaction entry
- [x] Focused offline checks/tests pass

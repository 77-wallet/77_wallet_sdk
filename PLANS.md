# PLANS

Current task execution plan.
Refs: `docs/codex/testing.md`, `docs/codex/workflows.md`.

## Task

- Name: wallet-database cleanup (batch B49-B51: remove api_window table + collect/withdraw strategy test hardening)
- Goal:
  - 删除未使用的 `api_window` 表及对应 DAO/Repo 导出与测试入口
  - 完成 `collect_strategy/withdraw_strategy` 测试断言增强并回归验证

## Scope

### In

- `wallet-database/src/repositories/api_wallet/collect_strategy.rs`
- `wallet-database/src/repositories/api_wallet/withdraw_strategy.rs`
- `wallet-database/src/dao/mod.rs`
- `wallet-database/src/repositories/api_wallet/mod.rs`
- `wallet-database/src/dao/api_window.rs` (remove)
- `wallet-database/src/repositories/api_wallet/window.rs` (remove)
- `wallet-database/schema/api_wallet/migrations/*drop_api_window*.sql` (add)
- `PLANS.md`

### Out

- 事务抽象/API 形态改造
- DAO SQL 语义调整
- 其它仓库新增用例扩展

## Constraints

- Keep behavior-compatible cleanup; no unrelated architecture refactor
- Offline validation only
- Deterministic tests only (no flaky stress patterns)

## Plan

1. 删除 `api_window` 代码路径并新增 drop migration，确保 schema 收敛
2. 完成 `collect_strategy/withdraw_strategy` 断言增强
3. 跑最小离线验证命令并记录结果

## Validation Commands

- `cargo check -p wallet-database --offline`
- `cargo test -p wallet-database collect_strategy_repo_ --offline -- --nocapture`
- `cargo test -p wallet-database withdraw_strategy_repo_ --offline -- --nocapture`

## Progress Checklist

- [x] Harden collect_strategy tests
- [x] Harden withdraw_strategy tests
- [x] Remove api_window table/repo/dao path
- [x] Run focused offline validation

# PLANS

Current task execution plan.
Refs: `docs/codex/testing.md`, `docs/codex/workflows.md`.

## Task

- Name: wallet-database test-first convergence (batch B1: api_wallet nonce)
- Goal:
  - 在不改生产逻辑前提下，为 `api_wallet/nonce` 补齐成功、边界、回滚三类仓库测试
  - 继续以确定性、离线可运行为准

## Scope

### In

- `wallet-database/src/repositories/api_wallet/nonce.rs`
- `wallet-database/src/repositories/test_helper.rs`
- `PLANS.md`

### Out

- 仓储 API/事务抽象改造
- DAO SQL 语义调整
- 其它 `api_wallet/*` 仓库测试扩展

## Constraints

- Test-only changes; no production behavior changes
- Offline validation only
- Deterministic tests only (no flaky stress patterns)

## Plan

1. 扩展测试 helper：支持 `api_funds.db` pool 创建
2. 为 `api_wallet/nonce` 补 3 条测试（成功、边界、回滚）
3. 执行最小验证命令并记录结果

## Validation Commands

- `cargo check -p wallet-database --offline`
- `cargo test -p wallet-database nonce_ --offline -- --nocapture`

## Progress Checklist

- [x] Add api_funds pool test helper
- [x] Add nonce success/edge/rollback tests
- [x] Run focused offline validation

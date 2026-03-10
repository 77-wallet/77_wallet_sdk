# PLANS

Current task execution plan.
Refs: `docs/codex/testing.md`, `docs/codex/workflows.md`.

## Task

- Name: wallet-database test-first convergence (batch A: core repositories)
- Goal:
  - 在不改生产逻辑前提下，补齐 Core 仓库 `account/assets/wallet/address_book` 的基础回归测试护栏
  - 每个仓库覆盖成功、边界/失败、事务回滚不落库三类场景

## Scope

### In

- `wallet-database/src/repositories/account.rs`
- `wallet-database/src/repositories/assets.rs`
- `wallet-database/src/repositories/wallet.rs`
- `wallet-database/src/repositories/address_book.rs`
- `wallet-database/src/repositories/mod.rs`
- `wallet-database/src/repositories/test_helper.rs` (new)
- `PLANS.md`

### Out

- 仓储 API/事务抽象改造
- DAO SQL 语义调整
- `api_wallet/*` 仓库测试扩展

## Constraints

- Test-only changes; no production behavior changes
- Offline validation only
- Deterministic tests only (no flaky stress patterns)

## Plan

1. 提取最小测试 helper（临时 sqlite、基础建数函数）供四个仓库复用
2. 为 `account/assets/wallet/address_book` 各补 3 条测试（成功、边界、回滚）
3. 执行最小验证命令并记录结果

## Validation Commands

- `cargo check -p wallet-database --offline`
- `cargo test -p wallet-database account_ --offline -- --nocapture`
- `cargo test -p wallet-database assets_ --offline -- --nocapture`
- `cargo test -p wallet-database wallet_ --offline -- --nocapture`
- `cargo test -p wallet-database address_book_ --offline -- --nocapture`

## Progress Checklist

- [x] Add shared test helper for core repositories
- [x] Add account success/edge/rollback tests
- [x] Add assets success/edge/rollback tests
- [x] Add wallet success/edge/rollback tests
- [x] Add address_book success/edge/rollback tests
- [x] Run focused offline validation

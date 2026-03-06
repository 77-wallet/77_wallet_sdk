# PLANS

Current task execution plan.  
Refs: `docs/codex/testing.md`, `docs/codex/workflows.md`.

## Task

- Name: wallet-transport-backend runtime guard + unified send entry
- Goal:
  - 修复 `BackendApi::new()` 在无 Tokio runtime 场景的隐式 panic 风险
  - 将 `app + chain` 模块收敛到统一发送入口（限流/重试/解密路径一致）

## Scope

### In

- `wallet-transport-backend/src/api/mod.rs`
- `wallet-transport-backend/src/api/wallet/app.rs`
- `wallet-transport-backend/src/api/wallet/chain.rs`
- `wallet-transport-backend/tests/offline_smoke.rs`
- `PLANS.md`

### Out

- `wallet-transport-backend` 其他业务模块（stake/coin/device 等）
- 对外 API 签名与协议
- 跨 crate 重构

## Constraints

- No new business semantics
- No protocol/interface change
- Offline-test requirement
- No real network dependency for default test path

## Plan

1. Add runtime guard in `initialize_cleanup_task` via `Handle::try_current`
2. Add internal helper send methods on `BackendApi` for app/chain usage
3. Migrate app + chain calls to helper methods
4. Add/adjust offline tests and run affected validation commands

## Validation Commands

- `cargo fmt --all`
- `cargo test -p wallet-transport-backend --lib`
- `cargo test -p wallet-transport-backend --test offline_smoke`
- `cargo test -p wallet-transport-backend --no-run --features online-tests`

## Expected Results

- `BackendApi::new()` in non-Tokio context no longer panics
- `app + chain` modules stop using direct naked `self.client.post/get(...).send...`
- Validation commands pass without adding network dependency

## Progress Checklist

- [x] Implement runtime guard
- [x] Implement internal helper send methods
- [x] Migrate app + chain calls
- [x] Run validation commands
- [x] Delivery notes

## Delivery Notes

- Changed files:
  - `wallet-transport-backend/src/api/mod.rs`
  - `wallet-transport-backend/src/api/wallet/app.rs`
  - `wallet-transport-backend/src/api/wallet/chain.rs`
  - `PLANS.md`
- Validation:
  - `cargo fmt --all` (passed)
  - `cargo test -p wallet-transport-backend --lib` (passed: 7/7)
  - `cargo test -p wallet-transport-backend --test offline_smoke` (passed: 3/3)
  - `cargo test -p wallet-transport-backend --no-run --features online-tests` (passed)
- Key decisions:
  - cleanup task init now runtime-aware and skip-safe in non-Tokio context
  - app/chain network calls unified via internal helper methods over `send_with_limit`
  - external API signatures and response semantics kept unchanged

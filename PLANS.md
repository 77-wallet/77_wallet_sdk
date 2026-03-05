# PLANS

Current task execution plan.  
Refs: `docs/codex/testing.md`, `docs/codex/workflows.md`.

## Task

- Name: chain_balance SOL coin-not-found stabilization
- Goal: 修复 `getChainBalance` 在手续费查询场景偶发 `coin not found: chain_code: sol, symbol: SOL`
- Deliverables:
  - `coin_by_symbol_chain` 统一规范化 `token_address`（trim 后判空）
  - `coin_by_symbol_chain` 增加主币兜底查询（仅主币符号生效）
  - 补充可观测日志与回归测试（成功 + 失败）

## Scope

### In

- `wallet-api/src/api/transaction.rs`
- `wallet-api/src/service/transaction.rs`
- `wallet-database/src/repositories/coin.rs`
- `PLANS.md`

### Out

- 前端代码改动
- API 签名/协议变更
- 非本缺陷相关的大规模重构

## Constraints

- No new business semantics
- Offline-test requirement
- Fallback 仅允许主币符号命中，避免误判 token

## Plan

1. Implement token_address normalization at coin repository boundary
2. Implement main-coin fallback in coin repository query
3. Add tests for fallback success/failure and input normalization
4. Run affected test targets and summarize results

## Validation Commands

- `cargo test -p wallet-database coin_by_symbol_chain_`
- `cargo test -p wallet-api --lib --no-run`

## Expected Results

- 主币 `SOL` 在误带 token_address 时不再触发 630
- 非主币误参仍按 NotFound 返回
- 对外统一将空白 token_address 视为 `None`，仅在 DAO 查询时映射到空字符串

## Progress Checklist

- [x] Implement code changes
- [x] Add/adjust tests
- [x] Run validation commands
- [x] Delivery notes

## Delivery Notes

- Changed files:
  - `wallet-api/src/api/transaction.rs`
  - `wallet-api/src/service/transaction.rs`
  - `wallet-database/src/repositories/coin.rs`
  - `PLANS.md`
- Validation:
  - `cargo test -p wallet-database coin_by_symbol_chain_ -- --nocapture` (passed: 3/3)
  - `cargo test -p wallet-api --lib --no-run` (passed)
- Key decisions:
  - `token_address` 规范化收敛到 `CoinRepo`，避免 API/Repo 双重规范化
  - `coin_by_symbol_chain` 仅在“symbol 与主币符号一致”时启用主币兜底
  - `chain_balance` 链上余额查询改为使用 `coin.token_address()`，避免 fallback 后继续携带污染入参
  - 在 `chain_balance` 与兜底分支补充可观测日志

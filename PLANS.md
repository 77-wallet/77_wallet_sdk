# PLANS

Current task execution plan.
Refs: `docs/codex/testing.md`, `docs/codex/checklists/pr-definition-of-done.md`.

## Task

- Name: wallet-api normal-wallet acct_change token-key sync follow-up
- Goal:
  - 普通钱包 `ACCT_CHANGE -> InnerEvent -> AssetsDomain` 固定按 `chain_code + normalized_token_address` 命中资产
  - 主币事实值统一归一为 `""`
  - 保留手动 `sync_assets_by_wallet(wallet_address, account_id, symbol)` 的 symbol 兼容语义

## Batch Scope

### In

- `wallet-api/src/infrastructure/inner_event.rs`
- `wallet-api/src/domain/assets/mod.rs`
- `wallet-api/tests/mqtt/mod.rs`
- `PLANS.md`

### Out

- 新增前端手动 token-key 同步接口
- 普通钱包公开 API 签名变更
- 其他 crate / schema / migration 调整

## Constraints

- Keep this round within one crate (`wallet-api`) and one flow (`order::AcctChange -> InnerEvent::SyncAssets -> AssetsDomain`)
- 事件流里不允许回退到 symbol 兜底
- 手动接口仍保留 `symbol=[]` 全量、`symbol=[..]` 按 symbol 过滤

## Plan

1. 在 `InnerEvent` 构造阶段归一化 `token_address`，让 `None` / `Some(\"\")` / 空白串在事件流里都视为主币 `\"\"`
2. 在 `AssetsDomain` 明确区分两种模式：事件流走 token-key，手动接口走 symbol/全量
3. 补普通钱包主币回归和手动兼容语义回归，保持 API wallet 既有回归不受影响

## Validation Commands

- `cargo test -p wallet-api --lib normal_assets_sync_filter_matches_by_token_when_symbol_differs -- --nocapture`
- `cargo test -p wallet-api --lib normal_assets_manual_sync_keeps_symbol_filter_when_token_missing -- --nocapture`
- `cargo test -p wallet-api --lib normal_assets_manual_sync_keeps_full_sync_when_symbol_empty -- --nocapture`
- `cargo test -p wallet-api --test mod acct_change_normal_wallet_syncs_by_token_when_symbol_mismatch -- --nocapture`
- `cargo test -p wallet-api --test mod acct_change_normal_wallet_syncs_native_by_empty_token_when_token_missing -- --nocapture`
- `cargo test -p wallet-api --test mod acct_change_syncs_sol_usdc_with_symbol_mismatch_by_token_address -- --nocapture`

## Stop Condition

- 普通钱包事件流主币和代币都按 token-key 命中
- 手动 `sync_assets_by_wallet` 兼容语义未被破坏
- 不扩展到新增公开接口

## Progress Checklist

- [x] Update plan for this batch
- [x] Normalize event-flow token semantics
- [x] Preserve manual symbol compatibility path
- [x] Add focused regression coverage
- [x] Run focused validation

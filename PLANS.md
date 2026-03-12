# PLANS

Current task execution plan.
Refs: `docs/codex/testing.md`, `docs/codex/checklists/pr-definition-of-done.md`.

## Task

- Name: wallet-api acct-change asset sync token-address matching fix
- Goal:
  - 让 `ApiWalletAcctChange -> InnerEvent -> ApiAssetsDomain` 这条资产同步链路统一按 `chain_code + token_address` 匹配资产
  - 让空 `token` 主币统一视为 `""`，不再依赖 MQTT `symbol`
  - 用离线回归测试锁定 “token symbol 与本地 symbol 不一致导致漏同步” 和 “主币空 token 误走 symbol 匹配” 两个缺陷

## Batch Scope

### In

- `wallet-api/src/infrastructure/inner_event.rs`
- `wallet-api/src/domain/api_wallet/assets.rs`
- `wallet-api/src/messaging/mqtt/topics/api_wallet/acct_change.rs`（仅在必要时保持参数透传一致）
- `PLANS.md`

### Out

- `wallet-database` schema / migration / entity 字段新增
- 普通钱包 `AssetsDomain` 行为调整
- coin 自动创建 / acct_change repair / 前端通知语义变更

## Constraints

- Keep this round within one crate and one flow
- Normalize `None` / empty / whitespace token to the native-asset key `""`
- Do not rely on MQTT `symbol` for API wallet asset identity
- Prefer offline-stable tests without real chain / backend calls

## Plan

1. Thread normalized `token_address` through `InnerEvent` dispatch into `ApiAssetsDomain`
2. Filter API assets only by `chain_code + normalized_token_address`, including native assets
3. Add focused regression unit tests for token mismatch, native token empty-key matching, and non-target token exclusion

## Validation Commands

- `cargo test -p wallet-api api_wallet_acct_change_syncs_sol_usdc_by_token_address_when_symbol_differs -- --nocapture`
- `cargo test -p wallet-api api_wallet_acct_change_syncs_native_asset_by_empty_token_without_symbol_matching -- --nocapture`
- `cargo test -p wallet-api api_wallet_acct_change_does_not_sync_other_assets_with_different_token_address -- --nocapture`

## Stop Condition

- Stop after API wallet acct-change sync no longer depends on `symbol` and the three regression tests pass
- Do not expand into normal wallet asset sync or database/schema changes in this round

## Assertion Matrix

| Flow | 输入组合（关键参数） | 预期行为 | 失败不变性 |
|---|---|---|---|
| 代币账变同步 | `chain_code=sol`，MQTT `token=EPj...`，MQTT `symbol=usd coin`，本地资产 `symbol=USDC` | 按 `chain_code + token_address` 命中并进入余额同步 | 不允许因 symbol 不一致被过滤 |
| 主币账变同步 | MQTT `token=""` 或空白 | 归一化为主币空 token 并命中本地主币资产 | 不允许回退到 symbol 匹配 |
| 非目标资产隔离 | 同地址同链下存在不同 `token_address` 资产 | 仅同步目标 token 对应资产 | 其他 token 资产不得被误同步 |

## Progress Checklist

- [x] Update plan for this batch
- [x] Implement token-address-based API asset matching
- [x] Add focused regression tests
- [x] Run focused validation

# PLANS

Current task execution plan.
Refs: `docs/codex/testing.md`, `docs/codex/workflows.md`.

## Task

- Name: repositories convergence (batch 115: remove domain _with_pool naming)
- Goal:
  - 清理 `wallet-api/domain` 中 `_with_pool` 命名残留
  - 事务/池显式方法统一为 `*_in_pool`
  - 删除重复接口并保持行为不变

## Scope

### In

- `wallet-api/src/domain/coin/mod.rs`
- `wallet-api/src/service/coin.rs`
- `wallet-api/src/domain/api_wallet/trans/collect.rs`
- `wallet-api/src/domain/api_wallet/trans/fee.rs`
- `wallet-api/src/domain/api_wallet/trans/withdraw.rs`
- `wallet-api/src/domain/api_wallet/trans/confirm_tx_tests.rs`
- `PLANS.md`

### Out

- `&CoreDbPool` 参数统一
- `*_with_executor` 命名继续扩展
- 任何 wallet-api 接口调整

## Constraints

- Keep behavior unchanged
- Batch-by-batch compile validation
- Offline validation only

## Plan

1. 删除 coin 领域重复的 `_with_pool` 接口并替换调用
2. 将 collect/fee/withdraw 的 `confirm_tx_with_pool` 重命名为 `confirm_tx_in_pool`
3. 同步测试调用点
3. 运行最小离线验证并停止本轮

## Validation Commands

- `cargo check -p wallet-database --offline`
- `cargo check -p wallet-api --offline`

## Progress Checklist

- [x] Remove duplicate coin _with_pool entry
- [x] Rename confirm_tx_with_pool to confirm_tx_in_pool
- [x] Update tests/callers
- [x] Run focused offline validation

# PLANS

Current task execution plan.
Refs: `docs/codex/testing.md`, `docs/codex/workflows.md`.

## Task

- Name: repositories convergence (batch 88: exchange_rate tx naming direct-close)
- Goal:
  - 在 `ExchangeRateRepo` 先做“事务命名直收口”
  - 去掉 `*_tx` 命名，直接改为 `*_with_executor`
  - 不引入兼容别名，不改业务语义

## Scope

### In

- `wallet-database/src/repositories/exchange_rate.rs`
- `PLANS.md`

### Out

- 命名去重/别名折叠
- `&CoreDbPool` 统一（留到后续批次）
- 跨 crate 调用点迁移

## Constraints

- Keep behavior unchanged
- Batch-by-batch compile validation
- Offline validation only

## Plan

1. 将 `ExchangeRateRepo` 里的事务方法从 `*_tx` 改为 `*_with_executor`
2. 同文件内调用同步更新
3. 运行最小离线验证并停止本轮

## Validation Commands

- `cargo check -p wallet-database --offline`

## Progress Checklist

- [x] Rename tx methods to `*_with_executor` in `ExchangeRateRepo`
- [x] Run focused offline validation

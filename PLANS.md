# PLANS

Current task execution plan.
Refs: `docs/codex/testing.md`, `docs/codex/workflows.md`.

## Task

- Name: repositories convergence (batch 94: remove two dao type aliases)
- Goal:
  - 删除 `CreateAddressQueryStateDao` 与 `ExchangeRateDao` 两个 alias
  - 调用方直接改用对应 `*Entity`
  - 不改业务语义，不扩散到仓储接口

## Scope

### In

- `wallet-database/src/dao/address_query_state.rs`
- `wallet-database/src/dao/exchange_rate.rs`
- `wallet-database/src/repositories/exchange_rate.rs`
- `PLANS.md`
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

1. 删除 `CreateAddressQueryStateDao` 与 `ExchangeRateDao` alias
2. 同步调整最小调用点到 `*Entity`
3. 运行最小离线验证并停止本轮

## Validation Commands

- `cargo check -p wallet-database --offline`
- `cargo check -p wallet-api --offline`

## Progress Checklist

- [x] Remove two DAO type aliases
- [x] Run focused offline validation

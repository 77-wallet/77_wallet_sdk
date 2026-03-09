# PLANS

Current task execution plan.
Refs: `docs/codex/testing.md`, `docs/codex/workflows.md`.

## Task

- Name: repositories convergence (batch 86: exchange_rate API shape pilot)
- Goal:
  - 先在 `ExchangeRateRepo` 落地统一 API 形态样板
  - 普通路径标准化为 `*_with_pool`，事务路径标准化为 `*_with_executor`
  - 旧方法保留兼容，不改业务语义

## Scope

### In

- `wallet-database/src/repositories/exchange_rate.rs`
- `PLANS.md`

### Out

- 命名去重/别名折叠
- 其他 repository 签名迁移
- `wallet-api` 侧批量调用点重写

## Constraints

- Keep behavior unchanged
- Batch-by-batch compile validation
- Offline validation only

## Plan

1. 在 `ExchangeRateRepo` 新增标准方法：`*_with_pool` 与 `*_with_executor`
2. 旧方法保留为薄兼容封装，内部委托到标准方法
3. 运行最小离线验证并停止本轮

## Validation Commands

- `cargo check -p wallet-database --offline`
- `cargo check -p wallet-api --offline`

## Progress Checklist

- [x] Add standardized `*_with_pool` / `*_with_executor` methods in `ExchangeRateRepo`
- [x] Keep old methods as compatibility wrappers
- [x] Run focused offline validation

# PLANS

Current task execution plan.
Refs: `docs/codex/testing.md`, `docs/codex/workflows.md`.

## Task

- Name: layering cleanup (batch 84: add wallet-api layering guard test)
- Goal:
  - 在 `wallet-api` 增加防回归测试，阻止再次出现 `DaoV1::` 直调
  - 只新增最小测试，不改业务语义
  - 保持行为不变，仅替换调用层级与依赖方向

## Scope

### In

- `wallet-api/tests/layering_guard.rs`
- `wallet-api/tests/mod.rs`
- `PLANS.md`

### Out

- 其他 domain/service/messaging 模块
- repository/dao 结构性重构
- 事务模型变更

## Constraints

- Keep behavior unchanged
- Batch-by-batch compile validation
- Offline validation only

## Plan

1. 新增 `layering_guard` 测试，扫描 `wallet-api/src` 中的 `DaoV1::` 直调
2. 在 `tests/mod.rs` 注册新测试模块
2. 运行最小离线验证并停止本轮

## Validation Commands

- `cargo test -p wallet-api --offline layering_guard -- --nocapture`
- `cargo check -p wallet-api --offline`

## Progress Checklist

- [x] Add layering guard test for wallet-api
- [x] Run focused offline validation

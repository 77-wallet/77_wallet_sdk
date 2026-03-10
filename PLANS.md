# PLANS

Current task execution plan.
Refs: `docs/codex/testing.md`, `docs/codex/workflows.md`.

## Task

- Name: repositories convergence (batch 114: narrow with_tx scope)
- Goal:
  - `repositories/mod.rs::with_tx` 仅用于当前测试，收敛为 `#[cfg(test)]`
  - 避免在生产 API 暴露无调用的事务辅助入口
  - 保持现有测试行为不变

## Scope

### In

- `wallet-database/src/repositories/mod.rs`
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

1. 将 `with_tx` 标记为 `#[cfg(test)]`
2. 保留并运行同文件事务测试
3. 运行最小离线验证并停止本轮

## Validation Commands

- `cargo check -p wallet-database --offline`
- `cargo check -p wallet-api --offline`

## Progress Checklist

- [x] Restrict with_tx to test-only scope
- [x] Preserve with_tx tests
- [x] Run focused offline validation

# PLANS

Current task execution plan.
Refs: `docs/codex/testing.md`, `docs/codex/workflows.md`.

## Task

- Name: address_book pool type convergence to CoreDbPool
- Goal:
  - `wallet-database/src/repositories/address_book.rs` 不再暴露 `crate::DbPool` 参数
  - 对齐到具体库类型：Core 路径统一使用 `CoreDbPool`
  - 保持业务语义不变

## Scope

### In

- `wallet-database/src/repositories/address_book.rs`
- `wallet-database/src/factory.rs`
- 受影响 `wallet-api` 调用点（仅编译修复）
- `PLANS.md`

### Out

- 其他 repositories 的类型重构
- DAO/SQL 语义调整
- 事务模型调整

## Constraints

- Keep behavior unchanged
- Small reversible patch set
- Offline validation only

## Plan

1. Replace `DbPool` arguments in address_book repo public APIs with `CoreDbPool`
2. Update repository factory construction path to pass `CoreDbPool`
3. Fix affected wallet-api call sites using address_book static methods
4. Run offline checks for wallet-database and wallet-api

## Validation Commands

- `cargo check -p wallet-database --offline`
- `cargo check -p wallet-api --offline`

## Progress Checklist

- [x] Refactor address_book repo signatures to CoreDbPool
- [x] Update factory and call sites
- [x] Run focused offline validation

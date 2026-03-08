# PLANS

Current task execution plan.
Refs: `docs/codex/testing.md`, `docs/codex/workflows.md`.

## Task

- Name: repoctx decoupling (batch 14: multisig_account static query helpers)
- Goal:
  - 在 `MultisigAccountRepo` 中补齐静态查询 helper（显式 `&CoreDbPool`）
  - 保留现有实例方法，避免当批次扩散到 service 大改
  - 为后续去实例化迁移提供平滑路径

## Scope

### In

- `wallet-database/src/repositories/multisig_account.rs`
- `PLANS.md`

### Out

- `wallet-api` 业务调用改写
- 删除旧实例接口
- 事务/DAO/SQL 语义变更

## Constraints

- Keep behavior unchanged
- Batch-by-batch compile validation
- Offline validation only

## Plan

1. Add static query helpers with explicit `&CoreDbPool`
2. Keep existing instance methods intact
3. Run offline checks for `wallet-database` and `wallet-api`

## Validation Commands

- `cargo check -p wallet-database --offline`
- `cargo check -p wallet-api --offline`

## Progress Checklist

- [x] Add static query helpers
- [x] Keep existing instance methods intact
- [x] Run focused offline validation

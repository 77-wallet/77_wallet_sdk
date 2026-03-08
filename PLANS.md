# PLANS

Current task execution plan.
Refs: `docs/codex/testing.md`, `docs/codex/workflows.md`.

## Task

- Name: repoctx decoupling (batch 13: multisig_account service mut cleanup)
- Goal:
  - 只做 `wallet-api/src/service/multisig_account.rs` 的借用签名收敛
  - 将不需要可变借用的 `mut self` / `&mut self` 调整为 `self` / `&self`
  - 保持业务逻辑与调用路径不变

## Scope

### In

- `wallet-api/src/service/multisig_account.rs`
- `PLANS.md`

### Out

- 业务流程与事务语义改造
- `wallet-database` repository 结构重写
- 其它 service/repo 大规模迁移

## Constraints

- Keep behavior unchanged
- Batch-by-batch compile validation
- Offline validation only

## Plan

1. Tighten service method receivers in `multisig_account.rs`
2. Keep method bodies and control flow unchanged
3. Run offline checks for `wallet-database` and `wallet-api`

## Validation Commands

- `cargo check -p wallet-database --offline`
- `cargo check -p wallet-api --offline`

## Progress Checklist

- [x] Tighten service receiver signatures
- [x] Keep behavior unchanged
- [x] Run focused offline validation

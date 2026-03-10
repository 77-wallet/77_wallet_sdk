# PLANS

Current task execution plan.
Refs: `docs/codex/testing.md`, `docs/codex/workflows.md`.

## Task

- Name: wallet-database test-first convergence (batch B28-B30: permission + multisig_member + multisig_signature)
- Goal:
  - 在不改生产逻辑前提下，完成 `repositories/permission`、`repositories/multisig_member`、`repositories/multisig_signature` 三个模块测试护栏
  - 继续以确定性、离线可运行为准

## Scope

### In

- `wallet-database/src/repositories/permission.rs`
- `wallet-database/src/repositories/multisig_member.rs`
- `wallet-database/src/repositories/multisig_signature.rs`
- `PLANS.md`

### Out

- 仓储 API/事务抽象改造
- DAO SQL 语义调整
- 其它 `api_wallet/*` 仓库测试扩展

## Constraints

- Test-only changes; no production behavior changes
- Offline validation only
- Deterministic tests only (no flaky stress patterns)

## Plan

1. 为 `permission/multisig_member/multisig_signature` 三个 repo 各补成功、边界、回滚三类测试
3. 执行最小验证命令并记录结果

## Validation Commands

- `cargo check -p wallet-database --offline`
- `cargo test -p wallet-database permission_repo_ --offline -- --nocapture`
- `cargo test -p wallet-database multisig_member_repo_ --offline -- --nocapture`
- `cargo test -p wallet-database multisig_signature_repo_ --offline -- --nocapture`

## Progress Checklist

- [x] Add permission success/edge/rollback tests
- [x] Add multisig_member success/edge/rollback tests
- [x] Add multisig_signature success/edge/rollback tests
- [x] Run focused offline validation

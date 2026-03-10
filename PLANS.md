# PLANS

Current task execution plan.
Refs: `docs/codex/testing.md`, `docs/codex/workflows.md`.

## Task

- Name: repositories convergence (batch 112: multisig_account static API migration)
- Goal:
  - 将 `MultisigAccountRepo` 剩余核心实例方法改为静态 `&CoreDbPool` 风格
  - `wallet-api` 的 `MultisigAccountService` 去掉 `repo` 成员，统一调用静态 repo
  - 保持业务语义不变

## Scope

### In

- `wallet-database/src/repositories/multisig_account.rs`
- `wallet-api/src/service/multisig_account.rs`
- `wallet-api/src/api/multisig_account.rs`
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

1. 将 `MultisigAccountRepo` 实例方法改为静态 `pool` 参数形式
2. 替换 `MultisigAccountService` 内全部 `self.repo.*` 调用
3. 去掉 service 的 `repo` 字段与 API 构造入参
3. 运行最小离线验证并停止本轮

## Validation Commands

- `cargo check -p wallet-database --offline`
- `cargo check -p wallet-api --offline`

## Progress Checklist

- [x] Convert remaining multisig account repo instance methods to static
- [x] Replace all service self.repo call sites
- [x] Remove service repo field and API constructor injection
- [x] Run focused offline validation

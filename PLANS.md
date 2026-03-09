# PLANS

Current task execution plan.
Refs: `docs/codex/testing.md`, `docs/codex/workflows.md`.

## Task

- Name: layering cleanup (batch 54: clear remaining entity calls to dao names)
- Goal:
  - 清理 `wallet-api` 与 `wallet-database` 中剩余 `Entity::*` 调用点
  - 外部调用统一使用 `Dao::*` 命名（含测试构造路径）
  - 行为保持不变，仅做分层命名收敛

## Scope

### In

- `wallet-database/src/dao/address_query_state.rs`
- `wallet-database/src/dao/permission.rs`
- `wallet-api/src/domain/assets/mod.rs`（注释内旧调用同步）
- `wallet-api/src/service/node.rs`（注释内旧调用同步）
- `PLANS.md`

### Out

- 其他 service/domain 模块
- repository/dao 结构性重构
- 事务模型变更

## Constraints

- Keep behavior unchanged
- Batch-by-batch compile validation
- Offline validation only

## Plan

1. 将当前扫描出的 `Entity::*` 剩余点替换为 `Dao::*`
2. 对 dao 内部 helper 做最小修正，避免 `Dao` 再回调 `Entity` 名称
3. 运行离线编译校验（`wallet-database` + `wallet-api`)

## Validation Commands

- `cargo check -p wallet-database --offline`
- `cargo check -p wallet-api --offline`

## Progress Checklist

- [x] Replace all remaining `Entity::*` call sites from latest scan
- [x] Keep behavior unchanged and imports clean
- [x] Run focused offline validation

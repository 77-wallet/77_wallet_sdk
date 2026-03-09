# PLANS

Current task execution plan.
Refs: `docs/codex/testing.md`, `docs/codex/workflows.md`.

## Task

- Name: layering cleanup (batch 55: replace ConfigDao direct usage with ConfigRepo)
- Goal:
  - 在 `wallet-api` 移除对 `ConfigDao::*` 的直接调用，统一走 `ConfigRepo`
  - 在 `wallet-database` 增加 `ConfigRepo`（最小封装）并补最小回归测试
  - 保持行为不变，仅收敛调用分层

## Scope

### In

- `wallet-database/src/repositories/config.rs`（新增）
- `wallet-database/src/repositories/mod.rs`
- `wallet-api/src/domain/app/config.rs`
- `wallet-api/src/domain/task_queue.rs`
- `wallet-api/src/service/app.rs`
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

1. 新增 `ConfigRepo`（`find_by_key/list_v2/upsert`）并在 `repositories/mod.rs` 导出
2. 将 `wallet-api` 内的 `ConfigDao::*` 调用替换为 `ConfigRepo::*`
3. 为 `ConfigRepo` 补最小测试：成功路径（upsert+find）+ 失败/不变性路径（缺失 key 返回 None）
4. 运行离线校验（受影响测试 + 两个 crate check）

## Validation Commands

- `cargo test -p wallet-database config_repo --offline -- --nocapture`
- `cargo check -p wallet-database --offline`
- `cargo check -p wallet-api --offline`

## Progress Checklist

- [x] Add `ConfigRepo` and export module
- [x] Replace direct `ConfigDao` calls in wallet-api
- [x] Add minimal `ConfigRepo` tests (happy + none/miss path)
- [x] Run focused offline validation

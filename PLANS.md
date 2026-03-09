# PLANS

Current task execution plan.
Refs: `docs/codex/testing.md`, `docs/codex/workflows.md`.

## Task

- Name: layering cleanup (batch 59: remove system_notification dao alias usage)
- Goal:
  - 在 `wallet-api` system notification flow 移除 `CreateSystemNotificationDao::new` 直接调用
  - 统一通过 `SystemNotificationRepo` 暴露构造入口生成 `CreateSystemNotificationEntity`
  - 保持行为不变，仅收敛分层依赖

## Scope

### In

- `wallet-database/src/repositories/system_notification.rs`
- `wallet-api/src/messaging/system_notification/mod.rs`
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

1. 在 `SystemNotificationRepo` 增加构造 helper：`build_create`
2. 将 `mod.rs` 中 `CreateSystemNotificationDao::new` 替换为 repo helper
3. 为新增 helper 补最小单元测试（字段映射 + 空 key/value 路径）
4. 运行最小离线验证并停止本轮

## Validation Commands

- `cargo test -p wallet-database system_notification_repo --offline -- --nocapture`
- `cargo check -p wallet-database --offline`
- `cargo check -p wallet-api --offline`

## Progress Checklist

- [x] Add `SystemNotificationRepo` constructor helper
- [x] Replace `CreateSystemNotificationDao::new` usage in flow
- [x] Add minimal tests for constructor helper
- [x] Run focused offline validation

# PLANS

Current task execution plan.
Refs: `docs/codex/testing.md`, `docs/codex/workflows.md`.

## Task

- Name: SQLite read/write split (Batch 2D: api_funds read-path explicit)
- Goal:
  - 在 `api_wallet` 的 `collect/fee/withdraw/nonce` 四个热点仓库中，将读路径显式统一为 `read_ref()`
  - 保持写路径继续走 `write_ref()`，事务边界不变
  - 保持上层接口、业务语义、schema 不变

## Scope

### In

- `wallet-database/src/repositories/api_wallet/collect.rs`
- `wallet-database/src/repositories/api_wallet/fee.rs`
- `wallet-database/src/repositories/api_wallet/withdraw.rs`
- `wallet-database/src/repositories/api_wallet/nonce.rs`
- `PLANS.md`

### Out

- `api_wallet` 其他仓库（后续批次）
- `core/task` 与遗留写路径清理（Batch 3）
- `sql_utils` 结构重构（后置小批）
- `wallet-api` 对外接口签名改造

## Constraints

- 单批仅 `wallet-database`，且只处理一个 flow：api_funds 仓库读路径显式化
- `as_ref()` 仅保留兼容，不在本批新增调用
- 写路径与事务入口不得改成 reader
- 先保证最小可验证闭环，避免跨模块扩散

## Plan

1. 将四个仓库所有读查询/扫描的 `pool.as_ref()` 替换为 `pool.read_ref()`
2. 复查写路径，确保 `INSERT/UPDATE/DELETE/UPSERT` 与事务入口仍走 `write_ref()`
3. 运行最小离线检查与定向测试，失败仅做本批内修复

## Validation Commands

- `cargo check -p wallet-database --offline`
- `cargo test -p wallet-database collect_repo_ --offline -- --nocapture`
- `cargo test -p wallet-database fee_repo_ --offline -- --nocapture`
- `cargo test -p wallet-database withdraw_repo_ --offline -- --nocapture`
- `cargo test -p wallet-database nonce_ --offline -- --nocapture`

## Progress Checklist

- [x] api_funds 四仓库读路径改为 `read_ref()`
- [x] 写路径保持 `write_ref()`，事务入口保持 writer
- [x] Focused offline checks/tests pass

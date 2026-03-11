# PLANS

Current task execution plan.
Refs: `docs/codex/testing.md`, `docs/codex/workflows.md`.

## Task

- Name: harden api wallet import write path (Batch 5A)
- Goal:
  - 收敛 `api.importApiWallet` 首次导入出款钱包的 SQLite 锁竞争
  - 仅加固 `api_wallet` 导入链路的写热点：`ApiWalletRepo` 与 `ApiAccountRepo`
  - 增加一条 `wallet-api` 业务回归，覆盖“导入出款钱包 + 并发资产查询”场景

## Scope

### In

- `wallet-database/src/repositories/api_wallet/account.rs`
- `wallet-database/src/repositories/api_wallet/wallet.rs`
- `wallet-api/tests/api_wallet_smoke.rs`
- `PLANS.md`

### Out

- 其它 repo 的事务抽象重构
- `sql_utils` 结构改造
- 非导入链路的额外 lock 治理

## Constraints

- 分批执行；本轮仅覆盖一个 flow：`importApiWallet(Withdrawal)`
- 写路径改动必须保留现有业务语义，仅增加 gate / retry / metric
- 按模块最小验证：先 `wallet-database`，再 `wallet-api` 目标回归

## Plan

1. 给 `ApiAccountRepo::upsert_account_multi` 增加 writer gate、锁重试与耗时日志
2. 给 `ApiWalletRepo` 导入链路写方法增加 writer gate、锁重试与耗时日志
3. 在 `wallet-api` 增加“导入出款钱包并发资产查询”回归测试
4. 运行最小离线编译与目标测试验证

## Validation Commands

- `cargo check -p wallet-database --offline`
- `cargo test -p wallet-database api_wallet_repo_ --offline -- --nocapture`
- `cargo check -p wallet-api --offline`
- `cargo test -p wallet-api import_withdrawal_wallet --features integration-tests --offline -- --nocapture`

## Progress Checklist

- [x] `ApiAccountRepo` 导入热点已加固
- [x] `ApiWalletRepo` 导入热点已加固
- [x] `wallet-api` 导入并发回归已补齐
- [x] Focused checks/tests pass

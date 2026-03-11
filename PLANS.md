# PLANS

Current task execution plan.
<<<<<<< HEAD
Refs: `docs/codex/testing.md`, `docs/codex/workflows.md`.

## Task

- Name: harden api wallet import write path (Batch 5A)
- Goal:
  - 收敛 `api.importApiWallet` 首次导入出款钱包的 SQLite 锁竞争
  - 仅加固 `api_wallet` 导入链路的写热点：`ApiWalletRepo` 与 `ApiAccountRepo`
  - 增加一条 `wallet-api` 业务回归，覆盖“导入出款钱包 + 并发资产查询”场景
=======
Refs: `docs/codex/testing.md`, `docs/codex/checklists/pr-definition-of-done.md`.

## Task

- Name: wallet-api tron multisig create to-address validation fix
- Goal:
  - 修复 TRON 多签转账创建时，对 `to` 地址误报 `Account(NotFound)` 的失败路径
  - 明确该 flow 不依赖“目标地址已存在于本地账户表”
  - 用最小回归测试锁定该缺陷
>>>>>>> a1ee4d9b15c30f145f9f0377f851c945a8d8fd38

## Scope

### In

<<<<<<< HEAD
- `wallet-database/src/repositories/api_wallet/account.rs`
- `wallet-database/src/repositories/api_wallet/wallet.rs`
- `wallet-api/tests/api_wallet_smoke.rs`
=======
- `wallet-api/src/service/multisig_transaction.rs`
- `wallet-api` 中与该 service 同模块的最小测试补充
>>>>>>> a1ee4d9b15c30f145f9f0377f851c945a8d8fd38
- `PLANS.md`

### Out

<<<<<<< HEAD
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
=======
- `wallet-database` repository/DAO 改动
- 其他链种的多签/普通转账行为调整
- 真实网络集成测试扩展

## Constraints

- Keep change set in one crate and one flow
- Prefer offline-stable tests
- Add regression coverage for the reported failure
- Do not broaden business semantics beyond removing the erroneous local-account dependency

## Plan

1. Add regression coverage for TRON multisig create error remapping around `to` address
2. Adjust create flow error handling so `to` address local absence does not surface as `Account(NotFound)`
3. Validate with the smallest `wallet-api` test target that covers the change

## Validation Commands

- `cargo test -p wallet-api remap_tron_to_not_found_error -- --nocapture`
- `cargo test -p wallet-api multisig_tx::multisig_tron -- --nocapture`

## Stop Condition

- Stop after the TRON multisig create flow no longer returns local-account-not-found for `req.to`
- Do not expand into non-TRON chains or broader transaction adapter refactors in this round

## Assertion Matrix

| Flow | 输入组合（关键参数） | 预期 backend 调用（接口/次数/字段） | 预期 DB 变化（表/字段） | 失败不变性（必须保持不变字段） |
|---|---|---|---|---|
| TRON 多签创建（错误映射） | `chain_code=tron` 且错误为 `Business(Account(NotFound(req.to)))` | 无新增 backend 调用（仅错误映射单测） | 无 DB 写入 | 非 `req.to` 的 `Account(NotFound)` 不应被重映射 |
| TRON 多签创建（错误映射） | `chain_code=tron` 且错误为 `ChainInteract(RpcError("Account not found:{req.to}"))` | 无新增 backend 调用（仅错误映射单测） | 无 DB 写入 | 仅匹配 `to` 的 not found 触发映射，其他 RPC 错误保持原样 |

## Progress Checklist

- [x] Update plan for this batch
- [x] Add regression test
- [x] Implement minimal fix
- [x] Run focused validation
>>>>>>> a1ee4d9b15c30f145f9f0377f851c945a8d8fd38

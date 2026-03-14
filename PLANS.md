# PLANS

Current task execution plan.
Refs: `docs/codex/testing.md`, `docs/codex/checklists/pr-definition-of-done.md`.

## Task

- Name: asset-token-key strict convergence (entity accessor cleanup batch)
- Goal:
  - 移除 `wallet-database` 资产实体 `token_address() -> Option<String>` 兼容访问器
  - `wallet-api` 侧显式使用 `AssetTokenKey`（`token_key()` / `as_db_str()`）做边界转换
  - 保持 `sync_assets_by_wallet(wallet_address, account_id, symbol)` 兼容语义不变（不改接口签名）

## Batch Scope

### In

- `wallet-database/src/entities/assets.rs`
- `wallet-database/src/entities/api_assets.rs`
- `wallet-api` 中直接调用上述实体 `token_address()` 的最小联动修复
- `PLANS.md`

### Out

- 普通钱包/Api 钱包 ACCT_CHANGE 语义变更（本轮不改行为）
- `wallet-api` 大规模重构（仅修必要编译断点）
- 数据库 schema 迁移

## Constraints

- 本轮主改一个模块：`wallet-database` 资产实体访问器收敛
- 遵守最小联动原则；`wallet-api` 仅做边界适配，不引入新接口
- 保持现有手动接口 `sync_assets_by_wallet` 签名与行为不变

## Plan

1. 移除 `AssetsEntity/ApiAssetsEntity` 及 `WithAddressType` 的 `token_address()` 兼容方法
2. 修复 `wallet-api` 直接调用点，按场景改为：
   - 需要 `String`：`token_key().as_db_str().to_string()`
   - 需要 `Option<String>`（协议边界）：`token_key().to_option_string_for_api()`
3. 运行 `wallet-api` 最小验证，确认编译和关键回归无回退

## Validation Commands

- `cargo check -p wallet-database --message-format short`
- `cargo check -p wallet-api --message-format short`

## Validation Notes

- 通过:
  - `cargo check -p wallet-database --message-format short`
  - `cargo test -p wallet-database repositories::coin::tests -- --nocapture`
  - `cargo check -p wallet-api --message-format short`

## Validation Notes

- 通过:
  - `cargo check -p wallet-database --message-format short`
  - `cargo check -p wallet-api --message-format short`

## Validation Notes

- 通过:
  - `cargo check -p wallet-database --message-format short`
  - `cargo check -p wallet-api --message-format short`

---

## Task

- Name: api coin domain service-layer symbol parameter removal
- Goal:
  - `ApiCoinDomain::get_coin_by_token_key` 去掉 `symbol` 参数
  - 先迁移 `service/api_wallet/*` 调用点，降低对 symbol 的内部依赖
  - `api_trans/*` 调用点下一批再迁（避免单轮改动过大）

## Batch Scope

### In

- `wallet-api/src/domain/api_wallet/coin.rs`
- `wallet-api/src/service/api_wallet/asset.rs`
- `wallet-api/src/service/api_wallet/transaction.rs`

### Out

- `wallet-api/src/infrastructure/api_trans/*` 的调用点
- 对外 request/response 协议字段调整

## Validation Commands

- `cargo check -p wallet-api --message-format short`

## Validation Notes

- 通过:
  - `cargo check -p wallet-api --message-format short`

---

## Task

- Name: normal wallet coin repo method convergence (safe rename rollout)
- Goal:
  - 在 `CoinRepo` 增加标准入口 `coin_by_chain_token_key(chain_code, token_key, pool)`
  - 普通钱包 `CoinDomain` 与 `TokenCurrencyGetter` 主调用切换到新入口
  - 旧 `coin_by_symbol_chain` 暂保留兼容，避免一次性行为回退

## Batch Scope

### In

- `wallet-database/src/repositories/coin.rs`
- `wallet-api/src/domain/coin/mod.rs`
- `wallet-api/src/domain/coin/token_price.rs`

### Out

- 删除 `coin_by_symbol_chain`（下一批）
- 普通钱包全部业务调用点去除 `symbol` 参数（下一批）

## Validation Commands

- `cargo check -p wallet-database --message-format short`
- `cargo check -p wallet-api --message-format short`

## Validation Notes

- 通过:
  - `cargo check -p wallet-api --message-format short`

---

## Task

- Name: api coin domain api_trans migration to exact token-key method
- Goal:
  - `infrastructure/api_trans/*` 调用点切换到 `ApiCoinDomain::get_coin_by_token_key_exact`
  - 为下一步移除兼容 wrapper (`get_coin_by_token_key` 带 symbol) 做准备

## Batch Scope

### In

- `wallet-api/src/infrastructure/api_trans/collect/process_collect_tx_send.rs`
- `wallet-api/src/infrastructure/api_trans/collect/shadow/worker/collect_worker.rs`
- `wallet-api/src/infrastructure/api_trans/collect_fee/process_fee_tx_send.rs`
- `wallet-api/src/infrastructure/api_trans/collect_fee/shadow/worker/shadow_fee_worker.rs`
- `wallet-api/src/infrastructure/api_trans/withdraw/process_withdraw_tx_send.rs`
- `wallet-api/src/infrastructure/api_trans/withdraw/shadow/worker/shadow_withdraw_worker.rs`

### Out

- 删除 `ApiCoinDomain::get_coin_by_token_key` 兼容方法（下一批）

## Validation Commands

- `cargo check -p wallet-api --message-format short`
- `cargo test -p wallet-api --test mod acct_change_normal_wallet_syncs_by_token_when_symbol_mismatch -- --nocapture`
- `cargo test -p wallet-api --test mod acct_change_normal_wallet_syncs_native_by_empty_token_when_token_missing -- --nocapture`
- `cargo test -p wallet-api --test mod acct_change_syncs_sol_usdc_with_symbol_mismatch_by_token_address -- --nocapture`

## Stop Condition

- `wallet-database` 资产实体不再暴露 `token_address() -> Option<String>` 兼容访问器
- `wallet-api` 在此变更下可通过最小编译
- 普通钱包与 API 钱包已存在的 symbol mismatch 回归继续通过

## Progress Checklist

- [x] Update plan for this batch
- [x] Remove entity Option-style token accessors
- [x] Apply minimal wallet-api compatibility fixes
- [x] Run focused validation commands

## Additional Convergence (This Round)

- 将 `CoinData::new` / `ApiCoinData::new` / `CoinId::new` 的 `token_address` 参数从 `Option<String>` 收敛为 `AssetTokenKey`
- 调用点改为显式 `AssetTokenKey` 构造（`.into()` 或 `AssetTokenKey::from_raw(...)`）
- 修复 `wallet-api/src/response_vo/standard_wallet/coin.rs` 中因移除实体访问器导致的 `token_address()` 递归实现
- 合并 `AssetsId` / `AssetsIdVo`：删除 `AssetsIdVo`，统一 `ApiAssetsDao::assets_by_id` / `ApiAssetsRepo::find_by_id` 使用 `AssetsId`
- `AssetsId::new` 入参改为 `AssetTokenKey` 后，修复普通钱包与 API 钱包资产路径及回归测试调用点

## Validation Notes

- 已通过（上一批）:
  - `cargo check -p wallet-api --message-format short`
  - `cargo test -p wallet-api --lib api_wallet_acct_change_syncs_sol_usdc_by_token_address_when_symbol_differs -- --nocapture`
  - `cargo test -p wallet-api --lib api_wallet_acct_change_syncs_native_asset_by_empty_token_without_symbol_matching -- --nocapture`
  - `cargo test -p wallet-api --lib api_wallet_acct_change_does_not_sync_other_assets_with_different_token_address -- --nocapture`
  - `cargo test -p wallet-api --test mod acct_change_normal_wallet_syncs_by_token_when_symbol_mismatch -- --nocapture`
  - `cargo test -p wallet-api --test mod acct_change_normal_wallet_syncs_native_by_empty_token_when_token_missing -- --nocapture`
  - `cargo test -p wallet-api --test mod acct_change_syncs_sol_usdc_with_symbol_mismatch_by_token_address -- --nocapture`
- 已通过（本轮新增）:
  - `cargo check -p wallet-api --message-format short`
  - `cargo test -p wallet-api --test mod acct_change_normal_wallet_syncs_by_token_when_symbol_mismatch -- --nocapture`
  - `cargo test -p wallet-api --test mod acct_change_normal_wallet_syncs_native_by_empty_token_when_token_missing -- --nocapture`
  - `cargo test -p wallet-api --test mod acct_change_syncs_sol_usdc_with_symbol_mismatch_by_token_address -- --nocapture`

---

## Task

- Name: remove symbol from `AssetsId` and keep symbol only in create payloads
- Goal:
  - `AssetsId` 只表达资产定位键（`address + chain_code + token_address`）
  - `CreateAssetsVo` / `ApiCreateAssetsVo` 显式携带 `symbol` 用于 insert/upsert
  - 不改变 `sync_assets_by_wallet(wallet_address, account_id, symbol)` 对外兼容语义

## Batch Scope

### In

- `wallet-database/src/entities/assets.rs`
- `wallet-database/src/entities/api_assets.rs`
- `wallet-database/src/dao/assets.rs`
- `wallet-database/src/dao/api_assets.rs`
- `wallet-api` 中 `AssetsId::new` / 结构体字面量及 `CreateAssetsVo::new` / `ApiCreateAssetsVo::new` 最小联动修复

### Out

- schema migration（本轮不改表结构/索引）
- 普通钱包手动同步接口语义改造
- 非资产主路径的 symbol 业务语义调整

## Plan

1. `AssetsId` 移除 `symbol` 字段与构造参数，统一 key 仅保留 `address/chain_code/token_address`
2. `CreateAssetsVo` / `ApiCreateAssetsVo` 新增 `symbol` 字段并改 `new(...)` 入参
3. DAO 查询与更新条件改为 key-only；insert/upsert 从 create vo 读 symbol
4. 修复 `wallet-api` 调用点与测试构造，保持行为不变

## Validation Commands

- `cargo check -p wallet-database --message-format short`
- `cargo check -p wallet-api --message-format short`
- `cargo test -p wallet-api --test mod acct_change_normal_wallet_syncs_by_token_when_symbol_mismatch -- --nocapture`
- `cargo test -p wallet-api --test mod acct_change_normal_wallet_syncs_native_by_empty_token_when_token_missing -- --nocapture`
- `cargo test -p wallet-api --test mod acct_change_syncs_sol_usdc_with_symbol_mismatch_by_token_address -- --nocapture`

## Stop Condition

- `AssetsId` 不再包含 `symbol`
- `CreateAssetsVo` / `ApiCreateAssetsVo` 承担 symbol 写入
- `wallet-api` 最小编译与关键账变回归通过

## Progress

- [x] `AssetsId` 去除 `symbol` 字段与构造参数
- [x] `CreateAssetsVo` / `ApiCreateAssetsVo` 增加 `symbol` 字段并切换 `new(...)` 签名
- [x] `wallet-database` DAO 读写 key 改为 `address + chain_code + token_address`
- [x] `wallet-api` 调用点与测试构造联动修复

## Validation Notes

- 通过:
  - `cargo check -p wallet-database --message-format short`
  - `cargo check -p wallet-api --message-format short`

---

## Task

- Name: api coin repo token-key query method merge
- Goal:
  - `ApiCoinDao` 去掉 `get_coin_by_token_key`，统一到 `get_coin_by_chain_code_token_address`
  - `ApiCoinRepo::coin_by_symbol_chain` 精确查询不再依赖 `symbol`
  - API 钱包 coin 命中规则与普通钱包保持一致：`chain_code + token_key`

## Batch Scope

### In

- `wallet-database/src/dao/api_coin.rs`
- `wallet-database/src/repositories/api_wallet/coin.rs`

### Out

- `wallet-api` service/domain 接口签名改造
- MQTT/HTTP 协议字段变更

## Validation Commands

- `cargo check -p wallet-database --message-format short`
- `cargo check -p wallet-api --message-format short`
  - `cargo test -p wallet-api --test mod acct_change_normal_wallet_syncs_by_token_when_symbol_mismatch -- --nocapture`
  - `cargo test -p wallet-api --test mod acct_change_normal_wallet_syncs_native_by_empty_token_when_token_missing -- --nocapture`
  - `cargo test -p wallet-api --test mod acct_change_syncs_sol_usdc_with_symbol_mismatch_by_token_address -- --nocapture`

---

## Task

- Name: multisig queue token accessor convergence
- Goal:
  - 去掉 `MultisigQueueEntity::token_address() -> Option<String>` 的 Option 语义
  - 改为 `token_key() -> AssetTokenKey`，在边界调用点再显式转换

## Batch Scope

### In

- `wallet-database/src/entities/multisig_queue.rs`
- `wallet-api/src/service/multisig_transaction.rs`（最小调用点联动）

### Out

- 交易/请求/响应协议层 `Option<String>` 的全面替换
- 普通/Api 钱包资产同步主链路（本批不改行为）

## Plan

1. `MultisigQueueEntity` 增加 `token_key()` 并移除 `token_address()` Option 接口
2. `multisig_transaction` 调用点改为 `queue.token_key().to_option_string_for_api()`
3. 跑最小编译和目标路径测试

## Validation Commands

- `cargo check -p wallet-database --message-format short`
- `cargo check -p wallet-api --message-format short`
- `cargo test -p wallet-api --test mod api_wallet::acct_change_syncs_sol_usdc_with_symbol_mismatch_by_token_address -- --nocapture`

## Validation Notes

- 通过:
  - `cargo check -p wallet-api --message-format short`
  - `cargo test -p wallet-database repositories::assets::tests::assets_upsert_update_and_query_consistent -- --nocapture`
  - `cargo test -p wallet-api --test mod api_wallet::acct_change_syncs_sol_usdc_with_symbol_mismatch_by_token_address -- --nocapture`

---

## Task

- Name: chain transaction token-key signature convergence
- Goal:
  - `ChainTransDomain::assets` / `update_balance` 从 `Option<String>` 切到 `AssetTokenKey`
  - 删掉 `assets(...)` 未使用的 `symbol` 参数
  - 调用点显式传递 `AssetTokenKey`，仅边界保留 `Option<String>`

## Batch Scope

### In

- `wallet-api/src/domain/chain/transaction.rs`
- `wallet-api/src/service/multisig_transaction.rs`
- `wallet-api/src/service/transaction.rs`

### Out

- `sync_assets_by_wallet` 对外接口语义改造
- 其他 domain/service 的 token 参数全量切换

## Plan

1. 修改 `ChainTransDomain` 两个方法签名为 `AssetTokenKey`
2. 修复多签与交易服务调用点
3. 跑最小编译与关键回归

## Validation Commands

- `cargo check -p wallet-api --message-format short`
- `cargo test -p wallet-database repositories::assets::tests::assets_upsert_update_and_query_consistent -- --nocapture`
- `cargo test -p wallet-api --test mod api_wallet::acct_change_syncs_sol_usdc_with_symbol_mismatch_by_token_address -- --nocapture`

## Stop Condition

- `ChainTransDomain` 主路径不再接收 `Option<String>` token 参数
- 相关调用点编译通过且目标回归通过

---

## Task

- Name: transaction adapter balance token-key convergence
- Goal:
  - 普通钱包链适配器 `TransactionAdapter::balance` 改为接收 `AssetTokenKey`
  - 仅在适配器边界内部转换为链 SDK 仍需的 `Option<String>`
  - `TransactionService` 调用点去掉 token Option 语义

## Batch Scope

### In

- `wallet-api/src/domain/chain/adapter/transaction_adapter.rs`
- `wallet-api/src/service/transaction.rs`

### Out

- API wallet adapter 的 balance 签名改造
- `CoinDomain::get_coin` 签名调整

## Plan

1. 调整 `TransactionAdapter::balance` 入参为 `AssetTokenKey`
2. 在方法内部做边界转换：`token_key.to_option_string_for_api()`
3. 修复 `TransactionService` 调用点并做最小验证

## Validation Commands

- `cargo check -p wallet-api --message-format short`
- `cargo test -p wallet-api --test mod acct_change_normal_wallet_syncs_by_token_when_symbol_mismatch -- --nocapture`
- `cargo test -p wallet-api --test mod acct_change_normal_wallet_syncs_native_by_empty_token_when_token_missing -- --nocapture`

## Stop Condition

- 普通钱包 `TransactionAdapter::balance` 调用链不再传递 `Option<String>`
- 编译通过且普通钱包账变关键回归通过

## Validation Notes

- 通过:
  - `cargo check -p wallet-api --message-format short`
  - `cargo test -p wallet-api --test mod acct_change_normal_wallet_syncs_by_token_when_symbol_mismatch -- --nocapture`
  - `cargo test -p wallet-api --test mod acct_change_normal_wallet_syncs_native_by_empty_token_when_token_missing -- --nocapture`
  - `cargo test -p wallet-api --test mod acct_change_syncs_sol_usdc_with_symbol_mismatch_by_token_address -- --nocapture`

---

## Task

- Name: api coin domain token-key bridge (service layer migration batch)
- Goal:
  - 在 `ApiCoinDomain` 增加 `AssetTokenKey` 入参方法，作为后续全面替换的桥接入口
  - 优先迁移 `service/api_wallet` 调用点，减少 API wallet service 层 `Option<String>` token 语义扩散
  - 先不改 `infrastructure/api_trans/*`，将变更规模控制在单模块小批次

## Batch Scope

### In

- `wallet-api/src/domain/api_wallet/coin.rs`
- `wallet-api/src/service/api_wallet/asset.rs`
- `wallet-api/src/service/api_wallet/transaction.rs`

### Out

- `wallet-api/src/infrastructure/api_trans/*` 中 `ApiCoinDomain::get_coin` 调用点
- 对外 request/response 协议字段签名改造

## Plan

1. 在 `ApiCoinDomain` 新增 `get_coin_by_token_key(chain_code, symbol, token_key: AssetTokenKey)`
2. 旧 `get_coin(..., token_address: Option<String>)` 变为桥接包装，内部调用新方法
3. `service/api_wallet` 调用点显式传递 `AssetTokenKey`

## Validation Commands

- `cargo check -p wallet-api --message-format short`
- `cargo test -p wallet-api --test mod acct_change_syncs_sol_usdc_with_symbol_mismatch_by_token_address -- --nocapture`

## Stop Condition

- `ApiCoinDomain` 提供 token-key 原生入口
- `service/api_wallet` 不再直接传 `Option<String>` 给 coin 查询
- 编译通过且 API wallet 账变关键回归通过

## Validation Notes

- 通过:
  - `cargo check -p wallet-api --message-format short`
  - `cargo test -p wallet-api --test mod acct_change_syncs_sol_usdc_with_symbol_mismatch_by_token_address -- --nocapture`

---

## Task

- Name: coin repo token-key query method merge
- Goal:
  - `CoinDao::get_coin_by_token_key` 不再接收/依赖 `symbol`
  - 与 `get_coin_by_chain_code_token_address` 融合，保留单一查询实现
  - `CoinRepo` 查询路径统一按 `chain_code + token_key` 精确命中，避免同 token 不同 symbol 混乱

## Batch Scope

### In

- `wallet-database/src/dao/coin.rs`
- `wallet-database/src/repositories/coin.rs`

### Out

- `ApiCoinDao` / `ApiCoinRepo`（本批不改）
- 上层 service/domain 接口签名改造

## Plan

1. 删除 `get_coin_by_token_key`，把 token-key 查询统一收敛到 `get_coin_by_chain_code_token_address`
2. `CoinRepo::coin_by_symbol_chain` 精确查询改为 `chain_code + token_key`，不再把 symbol 作为 SQL 条件
3. 运行 `wallet-database` 与 `wallet-api` 最小编译验证

## Validation Commands

- `cargo check -p wallet-database --message-format short`
- `cargo check -p wallet-api --message-format short`

## Stop Condition

- `CoinDao` 仅保留一条 token-key 精确查询实现
- `CoinRepo` 中不存在 `CoinDao::get_coin_by_token_key` 调用

## Validation Notes

- 通过:
  - `cargo check -p wallet-database --message-format short`
  - `cargo check -p wallet-api --message-format short`

---

## Task

- Name: api_trans coin lookup token-key convergence
- Goal:
  - 迁移 `infrastructure/api_trans/*` 中 `ApiCoinDomain::get_coin` 旧调用
  - API 资金主流程（collect / collect_fee / withdraw）统一使用 `get_coin_by_token_key`
  - 将 `Option<String>` token 仅保留在边界数据结构字段，不作为 domain 查询参数在流程内传播

## Batch Scope

### In

- `wallet-api/src/infrastructure/api_trans/collect/process_collect_tx_send.rs`
- `wallet-api/src/infrastructure/api_trans/collect/shadow/worker/collect_worker.rs`
- `wallet-api/src/infrastructure/api_trans/collect_fee/process_fee_tx_send.rs`
- `wallet-api/src/infrastructure/api_trans/collect_fee/shadow/worker/shadow_fee_worker.rs`
- `wallet-api/src/infrastructure/api_trans/withdraw/process_withdraw_tx_send.rs`
- `wallet-api/src/infrastructure/api_trans/withdraw/shadow/worker/shadow_withdraw_worker.rs`

### Out

- `req.token_addr` 字段类型改造（仍为 `Option<String>`）
- 非 API wallet 的普通钱包交易链路

## Plan

1. 将上述文件中 `ApiCoinDomain::get_coin(..., req.token_addr.clone())` 改为 `get_coin_by_token_key(..., req.token_addr.clone().into())`
2. 保持其余业务逻辑不变，仅收敛查询参数语义
3. 跑最小编译与 API wallet 账变回归

## Validation Commands

- `cargo check -p wallet-api --message-format short`
- `cargo test -p wallet-api --test mod acct_change_syncs_sol_usdc_with_symbol_mismatch_by_token_address -- --nocapture`

## Stop Condition

- `infrastructure/api_trans/*` 中不再有旧式 `ApiCoinDomain::get_coin(..., Option<String>)` 调用
- 编译通过且 API wallet 账变关键回归通过

## Validation Notes

- 通过:
  - `cargo check -p wallet-api --message-format short`
  - `cargo test -p wallet-api --test mod acct_change_syncs_sol_usdc_with_symbol_mismatch_by_token_address -- --nocapture`

---

## Task

- Name: transaction service token-key boundary convergence
- Goal:
  - `TransactionService::chain_balance` 去掉对 `CoinRepo::coin_by_symbol_chain(..., Option<String>)` 的直接依赖
  - 改为 `AssetTokenKey -> CoinDomain::get_coin` 统一路径
  - `transaction_fee` 内部 token 查询改用 `AssetTokenKey::as_db_str()`，避免 `unwrap_or_default` 字符串语义

## Batch Scope

### In

- `wallet-api/src/service/transaction.rs`

### Out

- 对外 `chain_balance(..., token_address: Option<String>)` 接口签名变更
- API wallet 的 transaction 语义调整

## Plan

1. `chain_balance` 入口立即把 `Option<String>` 转 `AssetTokenKey`
2. 用 `CoinDomain::get_coin` 查询币元信息
3. `transaction_fee` 以 `AssetTokenKey` 驱动 `coin_by_chain_address` 查询

## Validation Commands

- `cargo check -p wallet-api --message-format short`
- `cargo test -p wallet-api --test mod acct_change_normal_wallet_syncs_by_token_when_symbol_mismatch -- --nocapture`
- `cargo test -p wallet-api --test mod acct_change_normal_wallet_syncs_native_by_empty_token_when_token_missing -- --nocapture`

## Stop Condition

- `TransactionService` 内部不再直接以 `Option<String>` 参与 coin 元数据匹配
- 编译通过且普通钱包账变回归通过

---

## Task

- Name: coin domain token-key signature convergence
- Goal:
  - `CoinDomain::get_coin` 改为接收 `AssetTokenKey`
  - 普通钱包调用点不再传递 `Option<String>` token 语义
  - 仅在仓储边界做 `AssetTokenKey -> Option<String>` 转换

## Batch Scope

### In

- `wallet-api/src/domain/coin/mod.rs`
- 普通钱包调用 `CoinDomain::get_coin` 的最小联动文件

### Out

- `ApiCoinDomain::get_coin` 签名调整
- 对外 request/response 的 token 字段语义改造

## Plan

1. 修改 `CoinDomain::get_coin(chain_code, symbol, token_key: AssetTokenKey)`
2. 在内部调用 `CoinRepo::coin_by_symbol_chain` 前显式转换为 `token_key.to_option_string_for_api()`
3. 修复普通钱包调用点并回归验证

## Validation Commands

- `cargo check -p wallet-api --message-format short`
- `cargo test -p wallet-api --test mod acct_change_normal_wallet_syncs_by_token_when_symbol_mismatch -- --nocapture`
- `cargo test -p wallet-api --test mod acct_change_normal_wallet_syncs_native_by_empty_token_when_token_missing -- --nocapture`

## Stop Condition

- 普通钱包 `CoinDomain::get_coin` 调用链不再传递 `Option<String>`
- 编译通过且普通钱包账变回归通过

## Validation Notes

- 通过:
  - `cargo check -p wallet-api --message-format short`
  - `cargo test -p wallet-api --test mod acct_change_normal_wallet_syncs_by_token_when_symbol_mismatch -- --nocapture`
  - `cargo test -p wallet-api --test mod acct_change_normal_wallet_syncs_native_by_empty_token_when_token_missing -- --nocapture`

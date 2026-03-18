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

---

## Task

- Name: api-funds migration folding (dev-stage schema cleanup)
- Goal:
  - 将 `api_funds` 中后续 `add_*` migration 的字段直接折叠回初始建表 SQL
  - 避免 fresh 数据库初始化时重复 `ADD COLUMN` 导致冲突
  - 保持字段语义不变，仅调整 migration 组织方式

## Batch Scope

### In

- `wallet-database/schema/api_funds/migrations/20250815110217_api_withdraw.sql`
- `wallet-database/schema/api_funds/migrations/20250828094953_api_collect.sql`
- `wallet-database/schema/api_funds/migrations/20250901071722_api_fee.sql`
- `wallet-database/schema/api_funds/migrations/20260213170000_add_tx_res_received_at.sql`（删除）
- `wallet-database/schema/api_funds/migrations/20260224120000_add_collect_broadcast_uncertain_tracking.sql`（删除）
- `wallet-database/schema/api_funds/migrations/20260225120000_add_api_fee_broadcast_uncertain_tracking.sql`（删除）
- `wallet-database/schema/api_funds/migrations/20260225130000_add_api_withdraw_broadcast_uncertain_tracking.sql`（删除）

### Out

- 业务代码/实体/DAO 行为变更
- 新增 migration 文件

## Plan

1. 将 `tx_res_received_at` 字段合并进三张初始表定义
2. 将 `broadcast_uncertain_*` 字段合并进 withdraw/collect/fee 三张初始表定义
3. 删除对应 `add_*` migration 文件

## Validation Commands

- `cargo test -p wallet-database`

## Stop Condition

- `api_funds` 不再包含上述 `add_*` 列迁移文件
- 初始建表 SQL 已完整包含这些字段

---

## Task

- Name: wallet schema nullability/type convergence (dev-stage direct edit)
- Goal:
  - 继续在开发阶段直接收敛初始建表 schema，不新增 migration
  - 修复“schema 可空但实体按必填字符串使用”的高风险字段
  - 统一 `api_funds` 主表 `uid` 类型与可空策略

## Batch Scope

### In

- `wallet-database/schema/api_funds/migrations/20250815110217_api_withdraw.sql`
- `wallet-database/schema/api_funds/migrations/20250828094953_api_collect.sql`
- `wallet-database/schema/api_funds/migrations/20250901071722_api_fee.sql`
- `wallet-database/schema/api_wallet/migrations/20250722111447_api_wallet.sql`
- `wallet-database/schema/api_wallet/migrations/20250815073748_api_assets.sql`
- `wallet-database/schema/api_wallet/migrations/20250919065056_create_api_coin.sql`

### Out

- 新增 migration 文件
- DAO/实体/业务逻辑代码改动

## Plan

1. `api_funds` 三张主表 `uid` 改为 `TEXT NOT NULL DEFAULT ''`
2. 将 `api_collect/api_fee` 中实体必填字符串字段改为 `NOT NULL DEFAULT ''`（`block_height/notes/err_msg`）
3. `api_wallet` 侧收敛可空冲突字段：
   - `api_wallet.merchant_id -> TEXT NOT NULL DEFAULT ''`
   - `api_assets.balance -> TEXT NOT NULL DEFAULT '0'`
   - `api_coin.token_address/name/price -> TEXT NOT NULL DEFAULT ''`

## Validation Commands

- `cargo test -p wallet-database`

## Stop Condition

- 上述 6 个建表文件完成字段收敛
- `wallet-database` 测试可运行（允许报告既有失败）

---

## Task

- Name: remove attempted_at fields (batch 1: collect flow in wallet-database)
- Goal:
  - 删除 collect 流中的 `*_attempted_at` 字段及其读写
  - 去掉 `result_ack_attempted_at` 的发送 gate
  - 保持 Scanner/事实推进行为不依赖 attempted 字段

## Batch Scope

### In

- `wallet-database/schema/api_funds/migrations/20250828094953_api_collect.sql`
- `wallet-database/src/entities/api_collect.rs`
- `wallet-database/src/dao/api_collect.rs`
- `wallet-database/src/repositories/api_wallet/collect.rs`（注释同步）

### Out

- `api_fee` / `api_withdraw` attempted_at 字段删除（下一批）
- `wallet-api` 联动编译修复（下一批）

## Validation Commands

- `cargo test -p wallet-database dao::api_collect::tests -- --nocapture`
- `cargo test -p wallet-database repositories::api_wallet::collect::tests -- --nocapture`

## Stop Condition

- collect 相关 attempted_at 字段已从 schema/entity/dao 中移除
- collect DAO & repository 测试通过

---

## Task

- Name: semantic nullability alignment (batch 2: wallet-database entities)
- Goal:
  - 将 `merchant_id`、`api_fee/api_collect` 的可空语义对齐到实体类型
  - 避免继续出现“schema 可空但实体强制 String”错配

## Batch Scope

### In

- `wallet-database/src/entities/api_wallet.rs`
- `wallet-database/src/entities/api_fee.rs`
- `wallet-database/src/entities/api_collect.rs`
- `wallet-database/src/dao/api_fee.rs`（测试断言联动）

### Out

- `wallet-api` 联动改造（下一批）
- 额外 schema 结构重构

## Validation Commands

- `cargo test -p wallet-database dao::api_collect::tests -- --nocapture`
- `cargo test -p wallet-database dao::api_fee::tests -- --nocapture`
- `cargo test -p wallet-database repositories::api_wallet::wallet::tests -- --nocapture`

## Stop Condition

- 三个实体类型与当前 schema 可空语义一致
- 上述定向测试通过

---

## Task

- Name: wallet-oss p0 stability and testability hardening
- Goal:

---

## Task

- Name: core schema strategy cleanup (dev-stage migration removal)
- Goal:
  - 删除主库 `schema/migrations` 中误放的 `api_wallet` 策略表 migration
  - 将 `core task_queue` 旧表兼容逻辑收敛到代码中，再删除开发阶段追加的补丁 migration
  - 不改业务代码，只收敛 migration 归属

## Batch Scope

### In

- `wallet-database/schema/migrations/20250912015125_api_collect_strategy.sql`（删除）
- `wallet-database/schema/migrations/20251224090306_api_collect_strategy_chain_config.sql`（删除）
- `wallet-database/schema/migrations/20250912015114_api_withdraw_strategy.sql`（删除）
- `wallet-database/schema/migrations/20251224090344_api_withdraw_strategy_chain_config.sql`（删除）
- `wallet-database/schema/migrations/20251023071251_add_column_err_msg_to_task_queue.sql`（删除）
- `wallet-database/schema/migrations/20251112080513_add_column_remark_to_task_queue.sql.sql`（删除）
- `wallet-database/schema/migrations/20251218000000_add_task_queue_indexes.sql`（删除）
- `wallet-database/src/dao/task_queue.rs`
- `wallet-database/src/repositories/task_queue.rs`
- `PLANS.md`

### Out

- `task_queue` 相关 core migration 删除
- `api_wallet` / `task` schema 内容改动
- DAO / repository / domain 逻辑变更

## Plan

1. 记录本批次目的与边界，明确仅删除主库中重复的 strategy migration
2. 将 `core_db.task_queue` 读取改为兼容老结构的显式列查询，用 `NULL` 回填 `err_msg/remark`
3. 删除 4 个误放在主库的 strategy migration 文件和 3 个开发阶段追加的 `task_queue` core migration
4. 补 `task_queue` core 兼容回归测试，并运行 `wallet-database` 的最小验证

## Validation Commands

- `cargo test -p wallet-database repositories::api_wallet::collect_strategy::tests -- --nocapture`
- `cargo test -p wallet-database repositories::api_wallet::withdraw_strategy::tests -- --nocapture`
- `cargo test -p wallet-database repositories::api_wallet::collect_strategy_chain_config::tests -- --nocapture`
- `cargo test -p wallet-database repositories::api_wallet::withdraw_strategy_chain_config::tests -- --nocapture`
- `cargo test -p wallet-database repositories::task_queue::tests -- --nocapture`

## Stop Condition

- 主库 `schema/migrations` 不再包含上述 4 个 strategy migration
- 主库 `task_queue` 的 3 个开发阶段补丁 migration 已删除
- `api_wallet` 与 `task_queue` 相关定向测试通过
  - 复用 HTTP client，避免每次请求重复建连
  - 将关键 panic 路径改为显式错误返回
  - 让 `wallet-oss` 默认测试在本地/CI 稳定可执行

## Batch Scope

### In

- `wallet-oss` 单 crate
- `PLANS.md`

### Out

- trait 借用化/大规模 clone 优化
- 新增外部 mock 服务依赖
- 跨 crate 接口改造

## Plan

1. 在 `Oss` 内持有可复用 `reqwest::Client`，对象 API 统一复用
2. `from_env`、签名缺失 Date、下载限速参数改为显式错误
3. 网络依赖测试默认 `ignore`，本地单测覆盖关键错误分支与 metadata 容错
4. 移除库实现中的 `println!`，统一走 `tracing`

## Validation Commands

- `cargo test -p wallet-oss`

## Stop Condition

- `cargo test -p wallet-oss` 默认通过
- `wallet-oss` 库代码不再包含本轮目标的 panic 路径

## Validation Notes

- 通过:
  - `cargo test -p wallet-oss -- --nocapture`

---

## Task

- Name: collect worker internal token-key convergence
- Goal:
  - `collect/process_collect_tx_send.rs`、`collect/shadow/worker/collect_worker.rs`、`collect/shadow/worker/side_effect_worker.rs`
    的内部余额/手续费估算参数统一为 `AssetTokenKey`
  - 仅在调用链适配器时转换为 `Option<String>`

## Batch Scope

### In

- `wallet-api/src/infrastructure/api_trans/collect/process_collect_tx_send.rs`
- `wallet-api/src/infrastructure/api_trans/collect/shadow/worker/collect_worker.rs`
- `wallet-api/src/infrastructure/api_trans/collect/shadow/worker/side_effect_worker.rs`

### Out

- 对外 request/response 结构
- 链适配器方法签名

## Plan

1. `query_balance/estimate_fee` 内部参数改为 `AssetTokenKey`
2. 日志字段统一使用 `token_key.as_db_str()`
3. 调用适配器前执行 `token_key.to_option_string_for_api()`
4. 编译 + 普通钱包账变回归验证

## Validation Commands

- `cargo check -p wallet-api --message-format short`
- `cargo test -p wallet-api --test mod acct_change_normal_wallet_syncs_by_token_when_symbol_mismatch -- --nocapture`

## Stop Condition

- 上述 collect worker 路径不再以 `Option<String>` 表达内部 token 身份
- 编译与关键回归通过

## Validation Notes

- 通过:
  - `cargo check -p wallet-api --message-format short`
  - `cargo test -p wallet-api --test mod acct_change_normal_wallet_syncs_by_token_when_symbol_mismatch -- --nocapture`

---

## Task

- Name: service native price query explicit token-key
- Goal:
  - `service/permission.rs`、`service/stake.rs`、`service/swap.rs` 中主币价格查询不再传 `None`
  - 显式使用 `AssetTokenKey::Native` 调 `TokenCurrencyGetter::get_currency_by_token_key`

## Batch Scope

### In

- `wallet-api/src/service/permission.rs`
- `wallet-api/src/service/stake.rs`
- `wallet-api/src/service/swap.rs`

### Out

- 对外 API/request/response 签名
- 交易适配器签名

## Plan

1. 引入 `AssetTokenKey`
2. 将 `TokenCurrencyGetter::get_currency(..., None)` 替换为 `get_currency_by_token_key(..., AssetTokenKey::Native)`
3. 编译并跑 API wallet 账变回归

## Validation Commands

- `cargo check -p wallet-api --message-format short`
- `cargo test -p wallet-api --test mod acct_change_syncs_sol_usdc_with_symbol_mismatch_by_token_address -- --nocapture`

## Stop Condition

- 上述 service 文件不再用 `None` 隐式表示主币 token
- 编译与关键回归通过

## Validation Notes

- 通过:
  - `cargo check -p wallet-api --message-format short`
  - `cargo test -p wallet-api --test mod acct_change_syncs_sol_usdc_with_symbol_mismatch_by_token_address -- --nocapture`

---

## Task

- Name: multisig params token-key helper convergence
- Goal:
  - 在 `TransferParams`、`MultisigQueueFeeParams` 增加 `token_key()` 强类型访问器
  - `MultisigTransactionService` 内部改用该访问器，减少 `Option<String> -> into()` 散点转换

## Batch Scope

### In

- `wallet-api/src/response_vo/standard_wallet/transaction.rs`
- `wallet-api/src/service/multisig_transaction.rs`

### Out

- DTO 字段类型变更（保持 `Option<String>` 兼容）
- 多签对外 API 签名调整

## Plan

1. 为多签请求参数结构增加 `token_key()` 方法
2. 替换 service 中 `req.token_address.clone().into()` 为 `req.token_key()`
3. 编译并跑普通钱包账变回归保证无副作用

## Validation Commands

- `cargo check -p wallet-api --message-format short`
- `cargo test -p wallet-api --test mod acct_change_normal_wallet_syncs_by_token_when_symbol_mismatch -- --nocapture`

## Stop Condition

- 多签 service 内部不再散落 `Option<String>` 到 token-key 的临时转换
- 编译与关键回归通过

## Validation Notes

- 通过:
  - `cargo check -p wallet-api --message-format short`
  - `cargo test -p wallet-api --test mod acct_change_normal_wallet_syncs_by_token_when_symbol_mismatch -- --nocapture`

---

## Task

- Name: request helper token-key input convergence
- Goal:
  - 保持 request DTO 字段兼容 `Option<String>`
  - `BaseTransferReq::with_token` 与 `ApiBaseTransferReq::with_token` 接口改为接收 `impl Into<AssetTokenKey>`

## Batch Scope

### In

- `wallet-api/src/request/transaction/transfer.rs`
- `wallet-api/src/request/api_wallet/trans.rs`

### Out

- request DTO 字段类型变更
- 对外协议改造

## Plan

1. 将 `with_token` 参数从 `Option<String>` 收敛为 `impl Into<AssetTokenKey>`
2. 内部统一用 `token_key.to_option_string_for_api()` 回填兼容字段
3. 编译 + API wallet 关键回归验证

## Validation Commands

- `cargo check -p wallet-api --message-format short`
- `cargo test -p wallet-api --test mod acct_change_syncs_sol_usdc_with_symbol_mismatch_by_token_address -- --nocapture`

## Stop Condition

- request helper 已支持 token-key 强类型输入
- 编译与关键回归通过

## Validation Notes

- 通过:
  - `cargo check -p wallet-api --message-format short`
  - `cargo test -p wallet-api --test mod acct_change_syncs_sol_usdc_with_symbol_mismatch_by_token_address -- --nocapture`

---

## Task

- Name: service internal signatures token-key first
- Goal:
  - 普通钱包与 API 钱包资产详情、链上余额 service 内部签名收敛为 `AssetTokenKey`
  - API 层保持 `Option<String>` 兼容，在入口做一次性转换

## Batch Scope

### In

- `wallet-api/src/service/asset.rs`
- `wallet-api/src/service/api_wallet/asset.rs`
- `wallet-api/src/service/transaction.rs`
- `wallet-api/src/api/asset.rs`
- `wallet-api/src/api/api_wallet/asset.rs`
- `wallet-api/src/api/transaction.rs`
- `wallet-api/src/api/multisig_transaction.rs`

### Out

- 对外 API 参数签名变更
- request/response DTO 字段变更

## Plan

1. `service::detail/chain_balance` 参数改为 `AssetTokenKey`
2. API 层入口将 `Option<String>` 转 `AssetTokenKey::from_raw(...)`
3. 编译并跑普通钱包账变回归

## Validation Commands

- `cargo check -p wallet-api --message-format short`
- `cargo test -p wallet-api --test mod acct_change_normal_wallet_syncs_by_token_when_symbol_mismatch -- --nocapture`

## Stop Condition

- 上述 service 内部不再以 `Option<String>` 表达 token 身份
- 编译和关键回归通过

## Validation Notes

- 通过:
  - `cargo check -p wallet-api --message-format short`
  - `cargo test -p wallet-api --test mod acct_change_normal_wallet_syncs_by_token_when_symbol_mismatch -- --nocapture`

---

## Task

- Name: api-wallet order services token-key boundary convergence
- Goal:
  - `CollectService/TransferFeeService/WithdrawService` 内部签名改为 `AssetTokenKey`
  - `api/api_wallet/*` 入口层保留 `Option<String>`，统一转换后再调用 service

## Batch Scope

### In

- `wallet-api/src/service/api_wallet/collect.rs`
- `wallet-api/src/service/api_wallet/fee.rs`
- `wallet-api/src/service/api_wallet/withdraw.rs`
- `wallet-api/src/api/api_wallet/collect.rs`
- `wallet-api/src/api/api_wallet/fee.rs`
- `wallet-api/src/api/api_wallet/withdraw.rs`

### Out

- `request/api_wallet/trans.rs` 字段签名变更
- 外部协议改造

## Plan

1. service 方法参数 `token_address: Option<String>` 改为 `token_key: AssetTokenKey`
2. 组装 request 时统一使用 `token_key.to_option_string_for_api()`
3. api 层入口使用 `AssetTokenKey::from_raw(token_address.as_deref())` 转换
4. 编译并跑 API+普通钱包关键账变回归

## Validation Commands

- `cargo check -p wallet-api --message-format short`
- `cargo test -p wallet-api --test mod acct_change_syncs_sol_usdc_with_symbol_mismatch_by_token_address -- --nocapture`
- `cargo test -p wallet-api --test mod acct_change_normal_wallet_syncs_by_token_when_symbol_mismatch -- --nocapture`

## Stop Condition

- `api_wallet collect/fee/withdraw` service 内部无 `Option<String>` token 身份表达
- 编译和两条关键回归通过

## Validation Notes

- 通过:
  - `cargo check -p wallet-api --message-format short`
  - `cargo test -p wallet-api --test mod acct_change_syncs_sol_usdc_with_symbol_mismatch_by_token_address -- --nocapture`
  - `cargo test -p wallet-api --test mod acct_change_normal_wallet_syncs_by_token_when_symbol_mismatch -- --nocapture`

---

## Task

- Name: transaction bill service token-key signature cleanup
- Goal:
  - `BillService::coin_currency_price` 内部签名改为 `AssetTokenKey`
  - `api/transaction` 保留 `Option<String>`，入口转换为 token-key

## Batch Scope

### In

- `wallet-api/src/service/bill.rs`
- `wallet-api/src/api/transaction.rs`

### Out

- 对外 API 参数签名变更
- response 协议变更

## Plan

1. 将 service 参数 `token_address` 改为 `token_key: AssetTokenKey`
2. api 层使用 `AssetTokenKey::from_raw(token_address.as_deref())` 转换
3. 编译 + API wallet 关键回归验证

## Validation Commands

- `cargo check -p wallet-api --message-format short`
- `cargo test -p wallet-api --test mod acct_change_syncs_sol_usdc_with_symbol_mismatch_by_token_address -- --nocapture`

## Stop Condition

- `coin_currency_price` service 内部不再以 `Option<String>` 表达 token 身份
- 编译和关键回归通过

## Validation Notes

- 通过:
  - `cargo check -p wallet-api --message-format short`
  - `cargo test -p wallet-api --test mod acct_change_syncs_sol_usdc_with_symbol_mismatch_by_token_address -- --nocapture`

---

## Task

- Name: api wallet adapters native-token query explicit key
- Goal:
  - `domain/api_wallet/adapter/*_tx.rs` 中主币价格查询不再传 `None`
  - 改为 `get_currency_by_token_key(..., AssetTokenKey::Native)`

## Batch Scope

### In

- `wallet-api/src/domain/api_wallet/adapter/btc_tx.rs`
- `wallet-api/src/domain/api_wallet/adapter/doge_tx.rs`
- `wallet-api/src/domain/api_wallet/adapter/eth_tx.rs`
- `wallet-api/src/domain/api_wallet/adapter/ltx_tx.rs`
- `wallet-api/src/domain/api_wallet/adapter/sol_tx.rs`
- `wallet-api/src/domain/api_wallet/adapter/sui_tx.rs`
- `wallet-api/src/domain/api_wallet/adapter/ton_tx.rs`
- `wallet-api/src/domain/api_wallet/adapter/tron_tx.rs`

### Out

- request/response 协议
- 交易适配器签名变更

## Plan

1. 将运行路径上的 `TokenCurrencyGetter::get_currency(..., None)` 改为 token-key 强类型入口
2. 显式传 `AssetTokenKey::Native`
3. 编译 + API wallet 账变回归验证

## Validation Commands

- `cargo check -p wallet-api --message-format short`
- `cargo test -p wallet-api --test mod acct_change_syncs_sol_usdc_with_symbol_mismatch_by_token_address -- --nocapture`

## Stop Condition

- 上述 adapter 的主币价格查询不再依赖 `None` 语义
- 编译和关键回归通过

## Validation Notes

- 通过:
  - `cargo check -p wallet-api --message-format short`
  - `cargo test -p wallet-api --test mod acct_change_syncs_sol_usdc_with_symbol_mismatch_by_token_address -- --nocapture`

---

## Task

- Name: bill service token-key call-path cleanup
- Goal:
  - `BillService::coin_currency_price` 保持边界 `Option<String>` 不变
  - 进入 domain 后立即转 `AssetTokenKey`，调用 token-key 强类型入口

## Batch Scope

### In

- `wallet-api/src/service/bill.rs`

### Out

- API 对外签名变更
- response 字段类型变更

## Plan

1. 在 `coin_currency_price` 中将 `token_address` 转成 `AssetTokenKey`
2. 改调用为 `TokenCurrencyGetter::get_currency_by_token_key(...)`
3. 编译并回归验证

## Validation Commands

- `cargo check -p wallet-api --message-format short`
- `cargo test -p wallet-api --test mod acct_change_normal_wallet_syncs_by_token_when_symbol_mismatch -- --nocapture`

## Stop Condition

- `service/bill` 不再通过 `Option<String>` 直接驱动 token 价格查询 domain 入口
- 编译和关键回归通过

## Validation Notes

- 通过:
  - `cargo check -p wallet-api --message-format short`
  - `cargo test -p wallet-api --test mod acct_change_normal_wallet_syncs_by_token_when_symbol_mismatch -- --nocapture`
  - `wallet-database/src/entities` 中已无 `token*/Option<String>` 与 `with_token(...Option<String>)`

---

## Task

- Name: wallet-api default coin token-key typing
- Goal:
  - 将 `wallet-api` 默认币配置模型 `DefaultCoin.token_address` 从 `Option<String>` 收敛为 `AssetTokenKey`
  - 统一默认币初始化链路中的 token 身份表达，避免内部模型退回 Option 语义

## Batch Scope

### In

- `wallet-api/src/default_data/coin.rs`
- `wallet-api/src/domain/coin/mod.rs`
- `wallet-api/src/domain/api_wallet/coin.rs`
- `PLANS.md`

### Out

- API 请求/响应 DTO 的 `token_address: Option<String>` 兼容语义调整

## Validation Commands

- `cargo check -p wallet-api --message-format short`

## Stop Condition

- `DefaultCoin` 内部类型改为 `AssetTokenKey`
- `wallet-api` 编译通过

## Validation Notes

- 通过:
  - `cargo check -p wallet-api --message-format short`

---

## Task

- Name: api-wallet repo upsert token-key param typing
- Goal:
  - 将 `wallet-database` API 资金流仓储 `upsert_*` 接口中的 `token_addr: Option<String>` 收敛为 token-key 入参
  - 通过 `impl Into<AssetTokenKey>` 保持调用层兼容，避免大规模改调用点

## Batch Scope

### In

- `wallet-database/src/repositories/api_wallet/collect.rs`
- `wallet-database/src/repositories/api_wallet/fee.rs`
- `wallet-database/src/repositories/api_wallet/withdraw.rs`
- `PLANS.md`

### Out

- API 请求协议 `token_address: Option<String>` 改造

## Validation Commands

- `cargo check -p wallet-database --message-format short`
- `cargo check -p wallet-api --message-format short`

## Stop Condition

- 上述 repo `upsert_*` token 参数不再声明为 `Option<String>`
- `wallet-database` 与 `wallet-api` 编译通过

## Validation Notes

- 通过:
  - `cargo check -p wallet-database --message-format short`
  - `cargo check -p wallet-api --message-format short`

---

## Task

- Name: api-assets dao update-* token-key convergence
- Goal:
  - `ApiAssetsDao::update_balance` / `update_status` 入参从 `Option<String>` 收敛为 `AssetTokenKey`
  - `ApiAssetsRepo` 调用链不再做 `to_option_string_for_api()` 回退
  - 补齐仓储测试中的调用类型，避免继续传播 Option token 语义

## Batch Scope

### In

- `wallet-database/src/dao/api_assets.rs`
- `wallet-database/src/repositories/api_wallet/assets.rs`
- `PLANS.md`

### Out

- API/HTTP 对外 DTO `token_address: Option<String>` 语义调整
- `wallet-api` service/domain 逻辑变更

## Validation Commands

- `cargo check -p wallet-database --message-format short`
- `cargo check -p wallet-api --message-format short`
- `cargo test -p wallet-database repositories::api_wallet::assets::tests -- --nocapture`

## Stop Condition

- API wallet 资产仓储 update 路径内部 token 身份不再使用 `Option<String>`
- 双 crate 编译通过，且 assets repo 测试通过

---

## Task

- Name: wallet-database tests token-key ambiguity cleanup
- Goal:
  - 修复 `wallet-database` tests 中 `None` 传入 `impl Into<AssetTokenKey>` 的类型推断歧义
  - 不改业务语义，只做测试与测试构造参数显式化（主币统一显式 `AssetTokenKey::Native`）

## Batch Scope

### In

- `wallet-database/src/dao/api_collect.rs`
- `wallet-database/src/dao/api_withdraw.rs`
- `wallet-database/src/repositories/api_wallet/collect.rs`
- `wallet-database/src/repositories/api_wallet/fee.rs`
- `wallet-database/src/repositories/api_wallet/withdraw.rs`
- `wallet-database/src/repositories/multisig_queue.rs`
- `PLANS.md`

### Out

- 业务代码逻辑调整
- API 接口签名调整

## Validation Commands

- `cargo test -p wallet-database --lib --no-run --message-format short`
- `cargo test -p wallet-database repositories::api_wallet::assets::tests -- --nocapture`

## Stop Condition

- `wallet-database` tests 编译通过（至少 `--lib --no-run` 通过）
- API assets repo 目标测试通过

---

## Task

- Name: dto token-address typed convergence (system-notification batch)
- Goal:
  - 将系统通知 DTO 的 `token_address` 从 `Option<String>` 收敛为 `AssetTokenKey`
  - 保持 JSON 反序列化兼容历史 `null/""/"token"` 输入

## Batch Scope

### In

- `wallet-database/src/entities/asset_token_key.rs`
- `wallet-api/src/messaging/system_notification/mod.rs`
- `PLANS.md`

### Out

- HTTP/MQTT 对外 request DTO 全量改造
- 普通钱包/API 钱包交易请求签名改造

## Validation Commands

- `cargo check -p wallet-database --message-format short`
- `cargo test -p wallet-database repositories::api_wallet::assets::tests -- --nocapture`
- `cargo check -p wallet-api --message-format short`

## Stop Condition

- 系统通知 Transaction DTO 已使用 `AssetTokenKey`
- `AssetTokenKey` 反序列化兼容 null/空值

---

## Task

- Name: notify dto token-key typing convergence
- Goal:
  - `messaging/notify` 中交易/资源/确认前端事件 DTO 的 token 字段统一为 `AssetTokenKey`
  - 保持前端拿到的 JSON 为字符串语义（Native 为 `""`）

## Batch Scope

### In

- `wallet-api/src/messaging/notify/transaction.rs`
- `wallet-api/src/messaging/notify/resource.rs`
- `wallet-api/src/messaging/mqtt/topics/order/acct_change.rs`
- `wallet-api/src/messaging/mqtt/topics/api_wallet/acct_change.rs`
- `PLANS.md`

### Out

- 对外 HTTP request/response DTO 改造
- 链适配器 `balance(..., token: Option<String>)` 签名改造

## Validation Commands

- `cargo check -p wallet-api --message-format short`

## Stop Condition

- 上述 notify 事件 DTO 不再使用 `Option<String>` 表达 token 身份
- `wallet-api` 编译通过

---

## Task

- Name: asset-calc internal check-price token-key convergence
- Goal:
  - 将 `asset_calc` 内部 `check_and_update_price` 的 `token_address: Option<String>` 收敛为 `AssetTokenKey`
  - 调用点改为显式 token-key，避免内部逻辑再依赖 Option 语义

## Batch Scope

### In

- `wallet-api/src/infrastructure/asset_calc/actor_model.rs`
- `PLANS.md`

### Out

- `AssetCalcHandle::update_price` 对外签名变更
- HTTP/MQTT 边界 token DTO 变更

## Validation Commands

- `cargo check -p wallet-api --message-format short`

## Stop Condition

- `asset_calc` 内部价格检查路径不再以 `Option<String>` 表达 token 身份
- `wallet-api` 编译通过

---

## Task

- Name: token-currency-id token-key typing
- Goal:
  - 将 `TokenCurrencyId.token_address` 从 `Option<String>` 收敛为 `AssetTokenKey`
  - 保持构造调用兼容（`new` 支持 `Into<AssetTokenKey>`），避免全量调用改造

## Batch Scope

### In

- `wallet-api/src/response_vo/standard_wallet/coin.rs`
- `wallet-api/src/service/api_wallet/chain.rs`
- `PLANS.md`

### Out

- API 对外 DTO 的 `token_address: Option<String>` 协议变更

## Validation Commands

- `cargo check -p wallet-api --message-format short`

## Stop Condition

- `TokenCurrencyId` 内部 token 身份改为 `AssetTokenKey`
- `wallet-api` 编译通过

## Validation Notes

- 通过:
  - `cargo check -p wallet-api --message-format short`

---

## Task

- Name: asset-calc token-key internal typing
- Goal:
  - 将 `asset_calc` Actor 内部价格消息/初始化数据中的 `token_address` 从 `Option<String>` 收敛为 `AssetTokenKey`
  - 保持 manager 对外方法签名不变（边界仍接收 `Option<String>`）

## Batch Scope

### In

- `wallet-api/src/infrastructure/asset_calc/actor_model.rs`
- `PLANS.md`

### Out

- 对外 API DTO 的 `token_address: Option<String>` 协议变化

## Validation Commands

- `cargo check -p wallet-api --message-format short`

## Stop Condition

- `asset_calc` 内部价格更新链路不再以 `Option<String>` 表达 token 身份
- `wallet-api` 编译通过

## Validation Notes

- 通过:
  - `cargo check -p wallet-api --message-format short`

---

## Task

- Name: bill token field typed to asset-token-key
- Goal:
  - 将 `wallet-database/src/entities/bill.rs` 的 `token: Option<String>` 收敛为 `AssetTokenKey`
  - 修复 `wallet-api` 在账单构建/同步路径上的最小联动（请求入参与事件入库赋值）

## Batch Scope

### In

- `wallet-database/src/entities/bill.rs`
- `wallet-database/src/dao/bill.rs`
- `wallet-database/src/repositories/bill.rs`
- `wallet-api/src/messaging/mqtt/topics/order/acct_change.rs`
- `wallet-api/src/request/transaction/swap.rs`
- `wallet-api/src/request/transaction/transfer.rs`
- `wallet-api/src/domain/bill.rs`
- `wallet-api/src/service/transaction.rs`
- `wallet-api/src/service/api_wallet/transaction.rs`
- `wallet-api/src/domain/chain/adapter/multisig_adapter.rs`
- `wallet-api/src/service/multisig_transaction.rs`
- `PLANS.md`

### Out

- `sync_assets_by_wallet` 手动接口语义变更
- 数据库 schema 变更

## Validation Commands

- `cargo check -p wallet-database --message-format short`
- `cargo check -p wallet-api --message-format short`

## Stop Condition

- `BillEntity/NewBillEntity` 不再使用 `Option<String>` 表达 token 身份
- `wallet-database` / `wallet-api` 编译通过

## Validation Notes

- 通过:
  - `cargo check -p wallet-database --message-format short`
  - `cargo check -p wallet-api --message-format short`

---

## Task

- Name: entities token-addr to asset-token-key convergence
- Goal:
  - 将 `wallet-database/src/entities` 中交易相关实体的 `token_addr: Option<String>` 统一改为 `AssetTokenKey`
  - 覆盖：`api_collect` / `api_fee` / `api_withdraw` / `multisig_queue`
  - 保留边界构建器入参兼容（必要时仍接收 `Option<String>`，内部立即转 `AssetTokenKey`）

## Batch Scope

### In

- `wallet-database/src/entities/api_collect.rs`
- `wallet-database/src/entities/api_fee.rs`
- `wallet-database/src/entities/api_withdraw.rs`
- `wallet-database/src/entities/multisig_queue.rs`
- 必要联动的最小编译修复（`wallet-database` + `wallet-api`）
- `PLANS.md`

### Out

- API 请求/响应 DTO 的 `token_address: Option<String>` 对外协议改造
- 全链路一次性重构（按批次逐步收敛）

## Validation Commands

- `cargo check -p wallet-database --message-format short`
- `cargo check -p wallet-api --message-format short`

## Stop Condition

- 上述实体不再使用 `token_addr: Option<String>` 存储 token 身份
- 双 crate 编译通过

---

## Task

- Name: api-wallet trans domain assets token-key convergence
- Goal:
  - `ApiChainTransDomain::assets` 参数从 `Option<String>` 收敛为 `AssetTokenKey`
  - 统一 domain 内部 token 身份表达，减少 `Option` 语义进入核心层

## Batch Scope

### In

- `wallet-api/src/domain/api_wallet/chain.rs`
- `PLANS.md`

### Out

- service/request 边界 `token_address: Option<String>` 改造

## Validation Commands

- `cargo check -p wallet-api --message-format short`

## Stop Condition

- API wallet trans domain 资产读取接口在 domain 内不再使用 `Option<String>` 表达 token 身份
- `wallet-api` 编译通过

## Validation Notes

- 通过:
  - `cargo check -p wallet-api --message-format short`

---

## Task

- Name: test helpers token-key cleanup
- Goal:
  - API wallet 相关 repo/domain 测试 helper 去掉 `&str token_address`，改为 `AssetTokenKey`
  - 避免测试样例继续传播旧的 token 字符串语义

## Batch Scope

### In

- `wallet-database/src/repositories/api_wallet/coin.rs`
- `wallet-api/src/domain/api_wallet/assets.rs`
- `PLANS.md`

### Out

- 业务接口签名变更

## Validation Commands

- `cargo check -p wallet-database --message-format short`
- `cargo check -p wallet-api --message-format short`

## Stop Condition

- API wallet 相关测试 helper 统一使用 `AssetTokenKey`
- 双 crate 编译通过

## Validation Notes

- 通过:
  - `cargo check -p wallet-database --message-format short`
  - `cargo check -p wallet-api --message-format short`

---

## Task

- Name: api-wallet assets batch balance update token-key convergence
- Goal:
  - `ApiAssetsDao::batch_update_balance_in_tx` 参数从 `Option<String>` 改为 `AssetTokenKey`
  - `ApiAssetsRepo::batch_update_balance` 对齐为 `AssetTokenKey`
  - `ApiAssetsDomain` 批量更新调用直接传 `assets_id.token_address`
  - `ApiAssetsRepo::update_status` 参数改为 `AssetTokenKey`（当前无调用，先做接口收敛）

## Batch Scope

### In

- `wallet-database/src/dao/api_assets.rs`
- `wallet-database/src/repositories/api_wallet/assets.rs`
- `wallet-api/src/domain/api_wallet/assets.rs`
- `PLANS.md`

### Out

- API/HTTP/service 边界接口 `token_address: Option<String>` 清理

## Validation Commands

- `cargo check -p wallet-database --message-format short`
- `cargo check -p wallet-api --message-format short`

## Stop Condition

- API wallet 资产同步主链路（单条+批量）均以 `AssetTokenKey` 传递 token 身份
- 双 crate 编译通过

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

- Name: type price-update coin key in api-wallet repo path
- Goal:
  - `ApiCoinRepo::update_price_unit1` 参数从字符串 token 改为 `AssetTokenKey`
  - API wallet 与普通钱包 repo 的价格更新接口保持一致的 token-key 语义

## Batch Scope

### In

- `wallet-database/src/repositories/api_wallet/coin.rs`

### Out

- `ApiCoinDao::update_price_unit1` 签名调整
- 上层业务调用改造（当前无 repo 级调用）

## Validation Commands

- `cargo check -p wallet-database --message-format short`
- `cargo check -p wallet-api --message-format short`
- `cargo test -p wallet-database repositories::api_wallet::coin::tests -- --nocapture`

## Stop Condition

- API wallet repo 层价格更新不再接收裸 token 字符串键
- 仓储测试通过

## Validation Notes

- 通过:
  - `cargo check -p wallet-database --message-format short`
  - `cargo check -p wallet-api --message-format short`
  - `cargo test -p wallet-database repositories::api_wallet::coin::tests -- --nocapture`

---

## Task

- Name: api-wallet assets delete token-key convergence
- Goal:
  - `ApiAssetsRepo::delete_assets` 参数从字符串 token 改为 `AssetTokenKey`
  - API wallet service 删除资产路径显式按 token-key 传递身份
  - 补一条 repo 落库回归，确认仅删除目标 token-key 资产

## Batch Scope

### In

- `wallet-database/src/repositories/api_wallet/assets.rs`
- `wallet-api/src/service/api_wallet/asset.rs`
- `PLANS.md`

### Out

- `ApiAssetsDao::delete_assets` 签名调整
- API/HTTP 请求层 `token_address: &str` 边界重构

## Validation Commands

- `cargo check -p wallet-database --message-format short`
- `cargo check -p wallet-api --message-format short`
- `cargo test -p wallet-database repositories::api_wallet::assets::tests::assets_repo_delete_assets_matches_by_token_key -- --nocapture`

## Stop Condition

- API wallet repo/service 删除资产路径不再传裸 token 字符串身份
- 仓储回归验证目标 token-key 删除且其他资产不受影响

## Validation Notes

- 通过:
  - `cargo check -p wallet-database --message-format short`
  - `cargo check -p wallet-api --message-format short`
  - `cargo test -p wallet-database repositories::api_wallet::assets::tests::assets_repo_delete_assets_matches_by_token_key -- --nocapture`

---

## Task

- Name: api-wallet assets balance update token-key convergence
- Goal:
  - `ApiAssetsRepo::update_balance` 改为接收 `AssetTokenKey`
  - `ApiAssetsDomain::update_balance` 改为接收 `AssetTokenKey`
  - API wallet service 调用路径直接透传 `coin.token_address`

## Batch Scope

### In

- `wallet-database/src/repositories/api_wallet/assets.rs`
- `wallet-api/src/domain/api_wallet/assets.rs`
- `wallet-api/src/service/api_wallet/asset.rs`
- `PLANS.md`

### Out

- `ApiAssetsDao::update_balance` 参数类型调整
- `batch_update_balance` 的 `(String, String, Option<String>, String)` 结构调整

## Validation Commands

- `cargo check -p wallet-database --message-format short`
- `cargo check -p wallet-api --message-format short`

## Stop Condition

- API wallet 余额更新主链路不再把 token 身份作为 `Option<String>` 传递
- 双 crate 编译通过

## Validation Notes

- 通过:
  - `cargo check -p wallet-database --message-format short`
  - `cargo check -p wallet-api --message-format short`

## Task

- Name: type price-update coin key in normal wallet repo path
- Goal:
  - `CoinRepo::update_price_unit1` 参数从字符串 token 改为 `AssetTokenKey`
  - 调用侧按边界规则把后端 `token_address` 转为 `AssetTokenKey`

## Batch Scope

### In

- `wallet-database/src/repositories/coin.rs`
- `wallet-api/src/service/swap.rs`

### Out

- `CoinDao::update_price_unit1` 签名变更（保持 DAO 仍接收 DB 文本键）
- API wallet price update 路径改造

## Validation Commands

- `cargo check -p wallet-database --message-format short`
- `cargo check -p wallet-api --message-format short`

## Stop Condition

- 普通钱包 repo 层价格更新不再接收裸 token 字符串键
- 调用侧显式使用 `AssetTokenKey::from_raw(...)`

## Validation Notes

- 通过:
  - `cargo check -p wallet-database --message-format short`
  - `cargo check -p wallet-api --message-format short`

---

## Task

- Name: normal wallet coin repo string lookup removal
- Goal:
  - 普通钱包 `CoinRepo` 删除 `coin_by_chain_address` / `coin_by_chain_address_opt`
  - 统一为 `coin_by_chain_token_key` / `coin_by_chain_token_key_opt`
  - `wallet-api` 调用侧迁移到 token-key 可选查询

## Batch Scope

### In

- `wallet-database/src/repositories/coin.rs`
- `wallet-api/src/service/coin.rs`

### Out

- `CoinDao` 层接口签名调整
- API wallet coin repo 改造（上一批已处理）

## Validation Commands

- `cargo check -p wallet-database --message-format short`
- `cargo check -p wallet-api --message-format short`

## Stop Condition

- `wallet-api`/`wallet-database` 不再出现 `coin_by_chain_address*` 调用
- 双 crate 编译通过

## Validation Notes

- 通过:
  - `cargo check -p wallet-database --message-format short`
  - `cargo check -p wallet-api --message-format short`

---

## Task

- Name: api-wallet coin repo optional lookup signature convergence
- Goal:
  - 删除 `ApiCoinRepo` 字符串可选查询入口，统一可选查询为 `coin_by_chain_token_key_opt`
  - `has_coin` 改为接收 `AssetTokenKey`，调用侧不再传裸 token 字符串

## Batch Scope

### In

- `wallet-database/src/repositories/api_wallet/coin.rs`
- `wallet-api/src/messaging/mqtt/topics/api_wallet/acct_change.rs`

### Out

- `wallet-database::dao` 层 `ApiCoinDao` 接口签名改造
- 普通钱包 `CoinRepo` 字符串入口改造

## Validation Commands

- `cargo check -p wallet-database --message-format short`
- `cargo check -p wallet-api --message-format short`
- `cargo test -p wallet-database repositories::api_wallet::coin::tests -- --nocapture`

## Stop Condition

- API wallet 业务路径不存在字符串 token 的 `has_coin` 调用
- `api_wallet::coin` 仓储回归测试通过

## Validation Notes

- 通过:
  - `cargo check -p wallet-database --message-format short`
  - `cargo check -p wallet-api --message-format short`
  - `cargo test -p wallet-database repositories::api_wallet::coin::tests -- --nocapture`

---

## Task

- Name: api-wallet optional coin lookup typed migration
- Goal:
  - 增加 `ApiCoinRepo::coin_by_chain_token_key_opt`（类型化 + `Option`）
  - `wallet-api` 中需要可选 coin 的调用点改为 token-key 入口

## Batch Scope

### In

- `wallet-database/src/repositories/api_wallet/coin.rs`
- `wallet-api/src/service/api_wallet/coin.rs`
- `wallet-api/src/messaging/mqtt/topics/api_wallet/acct_change.rs`

### Out

- `get_coin_by_chain_code_token_address` 的完全删除（仍保留给仓储测试/兼容使用）
- 对外协议字段变更

## Validation Commands

- `cargo check -p wallet-database --message-format short`
- `cargo check -p wallet-api --message-format short`

## Stop Condition

- `wallet-api` 业务路径不再直接用字符串 token 查询 API coin
- 双 crate 编译通过

## Validation Notes

- 通过:
  - `cargo check -p wallet-database --message-format short`
  - `cargo check -p wallet-api --message-format short`

---

## Task

- Name: api-wallet typed coin lookup convergence
- Goal:
  - API wallet 内部调用改用 `ApiCoinRepo::coin_by_chain_token_key`
  - 删除 `ApiCoinRepo::coin_by_chain_address` 字符串入口，避免 token 查询退化

## Batch Scope

### In

- `wallet-database/src/repositories/api_wallet/coin.rs`
- `wallet-api/src/service/api_wallet/asset.rs`
- `wallet-api/src/domain/api_wallet/coin.rs`

### Out

- 对外 API 协议字段变更
- 手动同步接口语义调整

## Validation Commands

- `cargo check -p wallet-database --message-format short`
- `cargo check -p wallet-api --message-format short`

## Stop Condition

- `wallet-api` 中不再有 `ApiCoinRepo::coin_by_chain_address(...)` 调用
- `ApiCoinRepo` 仅保留 token-key 类型化 coin 查询入口

## Validation Notes

- 通过:
  - `cargo check -p wallet-database --message-format short`
  - `cargo check -p wallet-api --message-format short`

---

## Task

- Name: wallet-api callsite convergence to typed coin lookup
- Goal:
  - 将普通钱包内部调用点从 `CoinRepo::coin_by_chain_address(..., &str)` 收敛到
    `CoinRepo::coin_by_chain_token_key(..., AssetTokenKey)`
  - 减少 token 字符串化入口，统一 token-key 类型语义

## Batch Scope

### In

- `wallet-api/src/service/transaction.rs`
- `wallet-api/src/domain/coin/token_price.rs`
- `wallet-api/src/domain/assets/mod.rs`
- `wallet-api/src/service/swap.rs`

### Out

- API wallet 路径（`ApiCoinRepo::coin_by_chain_address`）改造
- 对外手动同步接口签名变更

## Validation Commands

- `cargo check -p wallet-api --message-format short`

## Stop Condition

- 以上普通钱包内部调用点不再依赖字符串 token 查询入口
- `wallet-api` 编译通过

## Validation Notes

- 通过:
  - `cargo check -p wallet-api --message-format short`

---

## Task

- Name: remove dead symbol-based coin detail dao methods
- Goal:
  - 删除 `CoinDao` 中未被调用且带 symbol 过滤的 `detail/detail_by_token_key` 入口
  - 避免后续路径误回退到 symbol 作为匹配条件

## Batch Scope

### In

- `wallet-database/src/dao/coin.rs`

### Out

- `CoinRepo`/`CoinDomain` 对外方法签名变更
- 手动接口 symbol 兼容语义调整

## Validation Commands

- `cargo check -p wallet-database --message-format short`
- `cargo check -p wallet-api --message-format short`

## Stop Condition

- `CoinDao` 不再保留 symbol 条件的 coin detail 查询入口
- 双 crate 编译通过

## Validation Notes

- 通过:
  - `cargo check -p wallet-database --message-format short`
  - `cargo check -p wallet-api --message-format short`

---

## Task

- Name: normal wallet multisig default asset sync switch to token-key
- Goal:
  - 将 `init_default_multisig_assets` 内部余额同步从 symbol 列表切到 token-key
  - 保持外部手动同步接口（按 symbol）不变

## Batch Scope

### In

- `wallet-api/src/domain/assets/mod.rs`

### Out

- `sync_assets_by_wallet(wallet_address, account_id, symbol)` 行为改造
- ACCT_CHANGE 事件接口结构变更

## Validation Commands

- `cargo check -p wallet-api --message-format short`

## Stop Condition

- 多签默认资产初始化后，内部同步调用不再依赖 symbol 列表
- 使用 `token_key` 去重并逐个触发精确同步

## Validation Notes

- 通过:
  - `cargo check -p wallet-api --message-format short`

---

## Task

- Name: remove request-symbol dependency in normal wallet balance refresh
- Goal:
  - 普通钱包 `chain_balance` 内部不再依赖请求入参 `symbol` 作为刷新键
  - 余额刷新统一使用 `token_key` 查得的 coin 元数据（`coin.symbol`）

## Batch Scope

### In

- `wallet-api/src/service/transaction.rs`
- `wallet-api/src/domain/chain/transaction.rs`

### Out

- 对外接口签名调整（`chain_balance` 继续保留 `symbol` 入参做兼容）
- `sync_assets_by_wallet` 兼容接口语义调整

## Validation Commands

- `cargo check -p wallet-api --message-format short`

## Stop Condition

- `ChainTransDomain::update_balance` 不再接收“请求 symbol”语义参数
- `TransactionService::chain_balance` 在更新本地/后端余额时使用 `coin.symbol`

## Validation Notes

- 通过:
  - `cargo check -p wallet-api --message-format short`

---

## Task

- Name: remove residual symbol-based coin dao entrypoints
- Goal:
  - 删除 `CoinDao::get_coin` / `ApiCoinDao::get_coin` 这两个仍带 `symbol` 参数的冗余入口
  - 统一 DAO 层只保留 `chain_code + token_key` 精确查询入口，避免后续误用 symbol 条件

## Batch Scope

### In

- `wallet-database/src/dao/coin.rs`
- `wallet-database/src/dao/api_coin.rs`

### Out

- `wallet-api` 对外接口参数语义变更（例如 `sync_assets_by_wallet`）
- 交易/手动同步路径的 symbol 兼容逻辑调整

## Validation Commands

- `cargo check -p wallet-database --message-format short`
- `cargo check -p wallet-api --message-format short`
- `cargo test -p wallet-database repositories::coin::tests -- --nocapture`
- `cargo test -p wallet-database repositories::api_wallet::coin::tests -- --nocapture`

## Stop Condition

- DAO 层不再提供带 `symbol` 的 `get_coin(...)` 入口
- coin 仓储相关回归测试通过

## Validation Notes

- 通过:
  - `cargo check -p wallet-database --message-format short`
  - `cargo check -p wallet-api --message-format short`
  - `cargo test -p wallet-database repositories::coin::tests -- --nocapture`
  - `cargo test -p wallet-database repositories::api_wallet::coin::tests -- --nocapture`

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

---

## Task

- Name: token-currency getter token-key path convergence
- Goal:
  - 在 `TokenCurrencyGetter` 增加 token-key 强类型入口，减少内部 `Option<String>` 传递
  - 迁移普通钱包链适配器中“主币价格查询”的 `None` 调用到 `AssetTokenKey::Native`
  - 保留旧 `get_currency/get_balance_info` 接口兼容，避免破坏外部调用

## Batch Scope

### In

- `wallet-api/src/domain/coin/token_price.rs`
- `wallet-api/src/domain/chain/adapter/transaction_adapter.rs`
- `wallet-api/src/domain/chain/adapter/multisig_adapter.rs`
- `wallet-api/src/domain/assets/mod.rs`（修复签名调整后的单测联动）

### Out

- request/response 层 `Option<String>` 协议语义改造
- API wallet adapter 全量迁移（本批只改普通钱包链适配器）

## Plan

1. 新增 `TokenCurrencyGetter::get_currency_by_token_key` 与 `get_balance_info_by_token_key`
2. 保留旧入口，内部转发到新入口
3. 将 `transaction_adapter` / `multisig_adapter` 里主币价格查询由 `None` 改为 `AssetTokenKey::Native`
4. 修复 `AssetsDomain` 单测中 `select_assets_for_sync` 新签名调用

## Validation Commands

- `cargo check -p wallet-api --message-format short`
- `cargo test -p wallet-api --lib api_wallet_acct_change_syncs_sol_usdc_by_token_address_when_symbol_differs -- --nocapture`
- `cargo test -p wallet-api --test mod acct_change_normal_wallet_syncs_by_token_when_symbol_mismatch -- --nocapture`

## Stop Condition

- `TokenCurrencyGetter` 已提供 token-key 入口且普通钱包链适配器主币调用不再传 `None`
- 关键 API wallet 与普通钱包账变回归均通过

## Validation Notes

- 通过:
  - `cargo check -p wallet-api --message-format short`
  - `cargo test -p wallet-api --lib api_wallet_acct_change_syncs_sol_usdc_by_token_address_when_symbol_differs -- --nocapture`
  - `cargo test -p wallet-api --test mod acct_change_normal_wallet_syncs_by_token_when_symbol_mismatch -- --nocapture`
  - `cargo test -p wallet-api --test mod acct_change_normal_wallet_syncs_native_by_empty_token_when_token_missing -- --nocapture`

---

## Task

- Name: api-wallet test fixtures token-key compile convergence
- Goal:
  - 修复 `wallet-api` lib test 在 `AssetTokenKey` 收敛后的编译断点
  - 消除 `upsert_*` 调用里 `None` 的类型推断歧义
  - 统一测试构造体中的 `token_addr` 字段赋值为 `AssetTokenKey`

## Batch Scope

### In

- `wallet-api/src/domain/api_wallet/trans/confirm_tx_tests.rs`
- `wallet-api/src/infrastructure/api_trans/collect/diagnose/engine.rs`
- `wallet-api/src/infrastructure/api_trans/collect/shadow/predicate.rs`
- `wallet-api/src/infrastructure/api_trans/collect/shadow/stage.rs`
- `wallet-api/src/infrastructure/api_trans/collect/shadow/worker/collect_worker.rs`
- `wallet-api/src/infrastructure/api_trans/collect_fee/diagnose/engine.rs`
- `wallet-api/src/infrastructure/api_trans/collect_fee/shadow/predicate.rs`
- `wallet-api/src/infrastructure/api_trans/withdraw/diagnose/engine.rs`
- `wallet-api/src/infrastructure/api_trans/withdraw/shadow/predicate.rs`

### Out

- 生产业务逻辑改动（仅测试与测试夹具）
- API 请求/响应协议改动

## Plan

1. 将测试用 `token_addr: None/Some(String)` 改为 `AssetTokenKey::Native/Contract`
2. 将 `ApiCollectRepo::upsert_api_collect` / `ApiFeeRepo::upsert_api_fee` / `ApiWithdrawRepo::upsert_api_withdraw` 的 `None` 入参改为显式 `AssetTokenKey::Native`
3. 回归运行 API wallet 三条 token-key 相关用例

## Validation Commands

- `cargo test -p wallet-api --lib api_wallet_acct_change_syncs_sol_usdc_by_token_address_when_symbol_differs -- --nocapture`
- `cargo test -p wallet-api --lib api_wallet_acct_change_syncs_native_asset_by_empty_token_without_symbol_matching -- --nocapture`
- `cargo test -p wallet-api --lib api_wallet_acct_change_does_not_sync_other_assets_with_different_token_address -- --nocapture`

## Stop Condition

- `wallet-api` lib test 不再因为 `token_addr` 类型不匹配或 `None` 推断歧义而失败
- 三条 API wallet token-key 回归均通过

## Validation Notes

- 通过:
  - `cargo test -p wallet-api --lib api_wallet_acct_change_syncs_sol_usdc_by_token_address_when_symbol_differs -- --nocapture`
  - `cargo test -p wallet-api --lib api_wallet_acct_change_syncs_native_asset_by_empty_token_without_symbol_matching -- --nocapture`
  - `cargo test -p wallet-api --lib api_wallet_acct_change_does_not_sync_other_assets_with_different_token_address -- --nocapture`

---

## Task

- Name: assets-domain sync filter enum convergence
- Goal:
  - 收敛普通钱包 `AssetsDomain` 内部同步过滤参数，不再使用 `Option<AssetTokenKey> + symbol` 双参数表达模式分支
  - 改为显式 `SyncFilter::{Token, Symbol}`，避免内部 `None` 语义分叉
  - 不改变外部接口与行为（手动 `symbol` 同步兼容语义保持不变）

## Batch Scope

### In

- `wallet-api/src/domain/assets/mod.rs`
- `wallet-api/tests/collect/mod.rs`（修复类型收敛带出的 test 夹具断点）

### Out

- API 层 `sync_assets_by_wallet(..., symbol)` 签名与行为
- 账变事件分发与分组语义

## Plan

1. 引入 `SyncFilter` 私有枚举并替代 `do_async_balance` 的 `Option<AssetTokenKey> + symbol` 参数
2. `sync_assets_by_wallet` / `sync_assets_by_addr_chain` 走 `SyncFilter::Symbol`
3. `sync_assets_by_addr_chain_token` 走 `SyncFilter::Token`
4. 更新日志输出，显示统一 `filter` 描述
5. 修复 `wallet-api/tests/collect/mod.rs` 中 `ApiCollectEntity.token_addr` 的 `AssetTokenKey` 赋值

## Validation Commands

- `cargo check -p wallet-api --message-format short`
- `cargo test -p wallet-api --test mod acct_change_normal_wallet_syncs_by_token_when_symbol_mismatch -- --nocapture`

## Stop Condition

- 普通钱包 `AssetsDomain` 内部不再以 `Option<AssetTokenKey>` 表达过滤模式
- 普通钱包 symbol mismatch 集成回归通过

## Validation Notes

- 通过:
  - `cargo check -p wallet-api --message-format short`
  - `cargo test -p wallet-api --test mod acct_change_normal_wallet_syncs_by_token_when_symbol_mismatch -- --nocapture`

---

## Task

- Name: normal wallet symbol-free coin lookup cleanup
- Goal:
  - 删除 `CoinRepo::coin_by_symbol_chain` 兼容路径，统一使用 `coin_by_chain_token_key`
  - 迁移普通钱包 `CoinDomain` 与 `TokenCurrencyGetter` 到 token-key 查询
  - 迁移 `multisig_*` 剩余调用并删除 `CoinDomain::get_coin(chain_code, symbol, token_key)` 兼容方法

## Batch Scope

### In

- `wallet-database/src/repositories/coin.rs`
- `wallet-api/src/domain/coin/mod.rs`
- `wallet-api/src/domain/coin/token_price.rs`
- `wallet-api/src/domain/assets/mod.rs`
- `wallet-api/src/service/asset.rs`
- `wallet-api/src/service/transaction.rs`
- `wallet-api/src/service/multisig_account.rs`
- `wallet-api/src/service/multisig_transaction.rs`
- `wallet-api/src/domain/chain/transaction.rs`

## Validation Commands

- `cargo check -p wallet-database --message-format short`
- `cargo test -p wallet-database repositories::coin::tests -- --nocapture`
- `cargo check -p wallet-api --message-format short`

## Validation Notes

- 通过:
  - `cargo check -p wallet-database --message-format short`
  - `cargo test -p wallet-database repositories::coin::tests -- --nocapture`
  - `cargo check -p wallet-api --message-format short`
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

---

## Task

- Name: wallet-database dao token bind strict typing cleanup
- Goal:
  - DAO 层 token 参数/绑定尽量直接使用 `AssetTokenKey`
  - 移除 `unwrap_or_default` 形式的 token SQL 绑定回退
  - 在不改行为前提下清理字符串中间态

## Batch Scope

### In

- `wallet-database/src/dao/assets.rs`
- `wallet-database/src/dao/coin.rs`
- `wallet-database/src/dao/api_coin.rs`
- `wallet-database/src/dao/api_collect.rs`
- `wallet-database/src/dao/api_fee.rs`
- `wallet-database/src/dao/api_withdraw.rs`

### Out

- SQL schema 迁移
- 对外 API/request/response 签名改造

## Plan

1. `coin/api_coin` DAO 直接 `.bind(token_address: AssetTokenKey)`
2. `assets` DAO 更新类语句去掉 `as_db_str().to_string()` 中间态，直接绑定 `AssetTokenKey`
3. `api_collect/api_fee/api_withdraw` 查询将 `bind_token + unwrap_or_default` 改为 `if let Some(token) { bind(token) }`
4. 运行最小编译验证

## Validation Commands

- `cargo check -p wallet-database --message-format short`
- `cargo check -p wallet-api --message-format short`

## Stop Condition

- 上述 DAO 路径不再使用 `unwrap_or_default` 做 token 绑定
- `wallet-database` 与 `wallet-api` 编译通过

## Validation Notes

- 通过:
  - `cargo check -p wallet-database --message-format short`
  - `cargo check -p wallet-api --message-format short`

---

## Task

- Name: wallet-api option-field alignment after funds schema semantics update
- Goal:
  - 对齐 `wallet-api` 对 `ApiCollectEntity/ApiFeeEntity/ApiWalletEntity` 的新语义（`Option<String>` 与 attempted 字段移除）
  - 修复当前 `wallet-api` 编译错误（E0308/E0599/E0609）

## Batch Scope

### In

- `wallet-api/src/domain/api_wallet/wallet.rs`
- `wallet-api/src/infrastructure/api_trans/collect/shadow/scanner.rs`
- `wallet-api/src/infrastructure/api_trans/collect/shadow/worker/side_effect_worker.rs`
- `wallet-api/src/infrastructure/api_trans/collect_fee/shadow/worker/side_effect_worker.rs`
- `wallet-api/src/test/collect.rs`

### Out

- 业务流程重构
- 新增迁移或 schema 变更

## Plan

1. 将 `merchant_id` 调用点从 `&String`/`&Option<String>` 统一改为 `as_deref().unwrap_or_default()` 或显式空串语义
2. 将 `notes/err_msg` 判空逻辑从 `is_empty()` 改为 `as_deref().unwrap_or("").is_empty()`
3. 删除 attempted 字段访问，改为仅依赖现有 `*_attempt_count` 与 `*_acked_at` 语义
4. 运行 `cargo check -p wallet-api --message-format short` 回归

## Validation Commands

- `cargo check -p wallet-api --message-format short`

## Stop Condition

- `wallet-api` 编译错误清零
- `Option` 字段与 attempted 字段移除后的调用链一致

---

## Task

- Name: api_funds fee-withdraw attempted field removal
- Goal:
  - 移除 `api_fee/api_withdraw` 的 `*_attempted_at` 字段（schema + entity + dao/repo + 调用方）
  - 保持事实驱动只依赖 sent/uploaded/received 等推进事实

## Batch Scope

### In

- `wallet-database/schema/api_funds/migrations/20250901071722_api_fee.sql`
- `wallet-database/schema/api_funds/migrations/20250815110217_api_withdraw.sql`
- `wallet-database/src/entities/api_fee.rs`
- `wallet-database/src/entities/api_withdraw.rs`
- `wallet-database/src/dao/api_fee.rs`
- `wallet-database/src/dao/api_withdraw.rs`
- `wallet-database/src/repositories/api_wallet/fee.rs`
- `wallet-database/src/repositories/api_wallet/withdraw.rs`
- `wallet-api/src/infrastructure/api_trans/collect_fee/*`（仅 attempted 兼容）
- `wallet-api/src/infrastructure/api_trans/withdraw/*`（仅 attempted 兼容）

### Out

- 新增 migration
- 业务流程重构

## Plan

1. 从 fee/withdraw 建表 schema 中删除 `*_attempted_at`
2. 删除实体字段，并清理默认构造/测试样例中的 attempted 字段初始化
3. 将 DAO 里 attempted 写入方法改为兼容空操作（或仅刷新 `updated_at`）并移除列更新 SQL
4. 清理 wallet-api scanner/log 注释中 attempted 依赖
5. 运行最小回归：`wallet-database` 定向测试 + `wallet-api` 编译

## Validation Commands

- `cargo test -p wallet-database dao::api_fee::tests -- --nocapture`
- `cargo test -p wallet-database dao::api_withdraw::tests -- --nocapture`
- `cargo check -p wallet-api --message-format short`
- `cargo test -p wallet-api --no-run`

## Stop Condition

- fee/withdraw 不再有 attempted 列和字段
- wallet-database 定向测试通过，wallet-api 编译与 no-run 通过

---

## Task

- Name: wallet-api log noise reduction (timer and scanner first pass)
- Goal:
  - 降低终端 `info` 级别日志噪音，尤其是定时器 tick / scanner 空转 / 批处理细节
  - 保留关键启动、停止、状态变化和扫描汇总日志
  - 文件日志仍保留更细粒度内容，便于排障

## Batch Scope

### In

- `wallet-api/src/infrastructure/log/mod.rs`
- `wallet-api/src/infrastructure/expand_address/scanner.rs`
- `PLANS.md`

### Out

- 大范围全项目日志重写
- 业务逻辑改动
- 新增配置项或 schema 改动

## Plan

1. 将 logger 调整为 `stdout` 默认至少 `info`，文件层继续沿用传入 `log_level`
2. 将 `ExpandScanner` 中高频的 tick / 分组 / 空转 / 批量发送细节日志从 `info` 下调到 `debug`
3. 保留 scanner 启停、单轮汇总、事实修复、错误/告警日志
4. 运行 `wallet-api` 最小编译验证

## Validation Commands

- `cargo check -p wallet-api --message-format short`

## Stop Condition

- 终端日志不再打印高频 timer/scanner 细节
- 文件日志仍保留细节
- `wallet-api` 编译通过

# Architecture

## Scope

本文件定义仓库级实现边界与依赖方向，用于约束改动范围与测试注入点。

## Layering

- API / Manager: 对外入口与编排入口
- Service: 对外用例门面，负责入口适配、简单转发、错误适配与查询入口
- Application: 用例编排层，负责组合领域能力完成一个入口场景
- Domain: 领域规则、领域行为与业务能力内部流程
- Repo / DB: 持久化访问
- Transport / External: backend、MQTT、外部系统

## DDD Layering Rules

本项目采用渐进式 DDD。历史代码中 `domain` 可能同时包含用例编排、
通知、任务推进与基础设施调用；新功能优先按本节规则组织，旧流程不为
分层命名做大规模搬迁。

### API / Manager

- 只暴露 SDK 或内部调用入口。
- 不查库、不组交易、不判断业务状态。
- 调用对应 Service 并返回 DTO。

### Service

- 是 API 与用例实现之间的门面，不是主要业务实现层。
- 适合放方法分组、简单参数转发、少量日志、错误适配、分页/列表查询。
- 不应直接承载复杂业务规则、链上交易构建、私钥解密、backend ACK 或
  Shadow 推进流程。

### Application

- 负责“一个具体用例如何完成”的流程编排。
- 可以组合多个 Domain 能力、Repository 查询、外部服务调用和返回值组装。
- 适合放入口相关、场景相关、会因调用来源不同而变化的流程，例如：
  - App 主动操作前的密码校验。
  - 后台任务消息的 ACK、落库、通知与推进顺序。
  - 是否写操作记录、是否通知前端、如何组装 response。
- Application 可以有流程，但流程表达的是用例，不表达领域规则本身。

### Domain

- 负责稳定的业务规则、领域行为和业务能力内部流程。
- Domain 中可以有流程；该流程应是业务能力自身必须遵守的规则，换一个
  入口仍然成立。
- 适合放：
  - 状态是否合法、金额是否合法、钱包类型是否允许操作。
  - 资源类型、链类型、交易类型等业务枚举转换。
  - 构建业务命令、计算费用、判断余额、状态流转规则。
  - 不自然属于单个 Entity 的领域服务。
- 不适合放：
  - “这个请求来自 App 还是后台”的入口差异。
  - 前端通知、backend ACK、MQTT ACK、Shadow 快速通道等用例副作用编排。
  - 分页展示、接口 response 组装、兼容旧接口的适配逻辑。

### Repo / DB

- DAO 负责 SQL / QueryBuilder、绑定参数、Entity 映射和数据库读写实现；生产 SQL 只写在 DAO。
- Repo 调用 DAO，管理连接池选择、事务、写入锁及已有重试职责，不直接编写 SQL。
- 不承载业务决策；例如 Repo 可以 `find_by_uid`，但不判断“该钱包是否能
  执行资源质押”。

### Decision Rule

- 如果流程换一个入口就会变，优先放 Application。
- 如果流程无论来自 App、后台任务还是测试入口都必须一致，优先放 Domain。
- 如果只是把外部 API 调用接入内部用例，放 Service。
- 如果只是数据读写，放 Repo。

### Context Passing

- 新增代码优先显式传递 `Context` 或更小的依赖，不新增隐式
  `CONTEXT.get()`。
- 旧代码中已经存在的全局 `CONTEXT.get()` 可保留，避免为了分层纯净扩大
  归集、提币、后台任务等稳定流程的改动面。
- 同一条新流程内不要混用“显式传 `ctx`”和“内部再取全局 CONTEXT”。
- 当测试或复用需要更细粒度依赖时，再把 `Context` 逐步拆成 pool、
  backend、chain adapter 等参数。

### API Wallet Resource Example

- `api/api_wallet/resource.rs`: 暴露 `WalletManager` 主动质押/解质押方法。
- `service/api_wallet/resource.rs`: 薄门面，调用 Application。
- `application/api_wallet_resource.rs`: 编排主动操作用例，例如密码校验、
  调用资源领域能力、组装返回结果。
- `domain/api_wallet/resource.rs`: 出款钱包规则、资源数量规则、Energy /
  Bandwidth 映射、stake/unstake 余额与交易构建规则。
- `wallet-database/repositories`: 只做钱包、交易、资源记录的查询和更新。

### Resource Gate Model

- `EvalResourceGate` 是操作步骤；它负责评估资源是否足够，并把结果落成事实。
- `resource_ready` / `need_platform_delegate` 是评估结果事实，不是独立
  intent，也不是可调度操作。
- `BuildTx` 只能在 `resource_gate_released_at` 已存在后继续推进。
- 平台资源代理任务是共享副链，`collect` 和 `withdraw` 都复用
  `api_resource_delegation`，但 scanner 必须按 `origin_trade_type` 分流，
  避免两个主流程扫描同一批资源任务。
- 这条共享副链还有第二层边界：`source`
  - `Platform`：后端下发给平台钱包执行的真实资源任务
  - `Local`：collect 在平台代理失败后切换到出款地址本地代理 fallback
  - 当前 `withdraw` 仍只消费 `Platform`
- 商户侧资源不足时只调用 `/resourceDl/apply` 并把原订单标记为等待
  平台结果；不得在商户钱包本地创建 `rsc_delegate_*` 这类 platform
  placeholder。若后端返回 `dlTradeNo`，原单通过
  `resource_dependency_trade_no` 等待该资源结果；若未返回 `dlTradeNo`，
  后端承诺使用原单号推送资源结果，SDK 通过原单号回写 gate。
- 资源代理成功时：
  - collect / withdraw 都在 `SendResourceResultAck` 成功后释放原订单 gate
  - `resource_gate_result = resource_delegation_success`
- 资源代理失败时：
  - collect / withdraw 都在 `UploadResourceTxExecReceipt` 成功后做 bypass 释放
  - 失败事实保留在 `api_resource_delegation`
  - `resource_gate_result = resource_delegation_failed_bypass`
- 资源代理失败不会阻塞 collect / withdraw 主流程；释放 gate 后由 scanner
  基于事实重新推进到 `BuildTx`。

### Local Reclaim Model

- `collect/shadow` 只负责归集主链，以及 `collect finished -> 创建本地回收任务`
  这一处事实投影。
- 本地回收执行/恢复不再属于 `collect/shadow`，而是由独立的
  `api_trans::resource_reclaim::local_shadow` 承接。
- `local_shadow` 只消费：
  - `source = Local`
  - `operation_type = Undelegate`
  - `origin_trade_type = Collect`
- 本地回收是资源善后副链，不是 collect 成功/失败的一部分：
  - 不走 backend task ACK / result ACK / receipt upload
  - 不依赖 MQTT 结果事件
  - 失败后只写 `recover_status / next_retry_at / retry_count`
  - 由本地 scanner 按 capped exponential backoff 无限重试
- collect 与本地回收之间只通过 `api_resource_delegation` 事实连接；
  `collect finished` 之后只负责幂等创建确定性的 local undelegation task，
  不直接驱动执行。

## Dependency Direction

- 允许：API -> Service -> Application -> Domain -> Repo/Transport
- 避免：低层反向依赖高层
- 变更优先级：先做注入点与测试护栏，再考虑重构

## Testability Seams

- Backend seam: trait + fake backend
- Storage seam: temp sqlite / test repo setup
- Async side effects seam: task noop / worker off
- Global state seam: serial + singleton fixture

## Change Policy

- 功能改动优先最小影响路径
- 非必要不改公共接口与跨模块契约
- 任何流程改动必须补测试与断言矩阵

## Confirmed Project Context

实现前读取与当前任务相关的条目。以下为用户业务澄清，不代表功能已经联调或上线验收；记录日期为 2026-10-05。未来确认变化时更新事实与来源，未知值保持待确认。

| 已确认事实 | 来源与日期 | 适用范围 / 时效 |
| --- | --- | --- |
| 本版本普通钱包与 API 钱包都支持 Polygon、Avalanche、Arbitrum、Base、Scroll 五条新增链；允许共用链节点配置，各自链、币、账户表保持边界。 | 用户业务澄清，2026-10-05 | 本版本加链需求；配置共用不意味着业务表合并，实际实现与验收状态另查。 |
| API 子账户数量很大；具体上限未提供，待确认，不编造数量。查询范围、分页、内存和请求量需评估规模。 | 用户规模澄清，2026-10-05 | API 账户查询、同步和扩链；复用 [规模与能力清单](checklists/account-sync-scale.md)，仅在方案需要时询问具体规模。 |
| API 测试目标为 **50 万地址**，不是 50 万逻辑账户，也不是已证明的承载指标；实际多链数据行数与逻辑账户映射需在测试中核实，旧 8 万行试验不能替代此目标验证。 | 用户语音原话“测试的话是测五十万的地址”，2026-10-05 13:31 UTC | API 规模测试目标；记录不等于测试已完成或承载验收通过。 |
| dev/test/prod 由编译确定，没有 App 内环境切换。跨环境覆盖安装并保留数据库不是支持的产品场景，而是开发者个人误操作。 | 用户场景澄清，2026-10-05 | 环境与旧库兼容讨论；不据误操作假设扩大支持场景或自动实施修复。 |
| 后台 API 五条新增链尚未配置。 | 用户当前状态说明，2026-10-04—05；记录于 2026-10-05 | 临时环境状态；下次依赖后台配置时重新核实，不能作为永久限制。 |
| 生产 SQL 在 DAO，Repo 委托 DAO，保留连接池、事务、写入锁及已有重试职责。 | 用户分层澄清，2026-10-05 | 新增或修改持久化路径，遵循本文件 Repo / DB 边界。 |

上述事实只提供方案前提，不授权新的生产修复、提交或发布；待确认假设与修复建议仍需按工作流报告和取得相应批准。

### API message-center scope (user confirmation, 2026-10-06)

The user confirms public API message-center parameters do not require a UID;
for now aggregate existing business reminder records across active API Withdrawal
wallets only. Records retain internal UID ownership. Existing pagination remains;
this is not a wallet-list API. Read state remains independent of resolution.
Ordinary wallets, SubAccount wallets and paused phase-two chain work are excluded.
Source: user voice confirmation relayed by coordinating thread; applies to this
message-center change, not permission to commit/push/release or transact.

### API business notification summary/detail (user confirmation, 2026-10-07)

User authorizes lightweight message/banner response items that retain id/isRead and UI-required fields, plus a separate by-id typed detail query. Detail displays balance/coin, merchant, address/chain, address type and notification receipt time; recharge navigation uses address+chain. Message history must not be globally filtered to unresolved records; the balance banner stays unresolved-only. Detail reads do not imply mark-read, and resolved history is accessible within the existing active withdrawal-wallet scope. No tradeNo order contract or device-database changes. Source: coordinating-thread explicit approval; implementation/verification only, no feature commit/push/release authorization. Exact summary UI fields pending clarification.

Confirmed notification presentation (2026-10-07): screenshot shows balance quantity plus coin and relative notification time. Summary is id/kind/isRead/remaining/symbol/notifiedAt, using payload.notifiedAt instead of first-insert createdAt (valid legacy records without that timestamp fall back). Detail retains merchant/address/chain/type/token/threshold and resolvedAt. Only AWM_ADDRESS_INSUFFICIENT is currently persisted; future kinds are not reinterpreted as balance. App writes/submissions are excluded from this feature; dependency/call instructions are handed to frontend.


### Ordinary-wallet bill decimal presentation/storage (2026-10-13)

User confirms transaction amounts must display as ordinary decimals, new bill
amounts must be stored without scientific notation, and existing scientific
notation values must remain readable without clearing the database. Source:
user instruction in this chat, 2026-10-13. Scope: ordinary-wallet bill amount
presentation/storage; no schema migration, historical rewrite, commit or release
authorization. Existing display truncation to eight decimals remains unchanged.

### Ordinary-wallet EVM maximum transfer (2026-10-13)

User confirms the failing Base transfer was entered via the App maximum button,
and authorizes SOL-style SDK calculation for EVM native maximum plus inspecting
and fixing the App maximum intent. Source: this chat, 2026-10-13. Scope: ordinary
wallet EVM native transfer; manual amounts and ERC20 behavior remain unchanged.
User subsequently approves committing and pushing this fix to SDK develop and
App dev in this chat (2026-10-13); plugin dependency publication, release and real
transactions remain outside that approval. App source inspected this date: maximum deducts selected maxFee,
but spendAll excludes EVM; the captured request also reports spend_all=false.
Actual running binary dependency revision remains unverified.

### App collaboration boundary (user clarification, 2026-10-13)

App changes are owned by the frontend developer. SDK/plugin work must provide
frontend integration instructions rather than directly edit, commit or push
sf-wallet-app. This later clarification supersedes the earlier App authorization
in this chat. Source: user explicitly states that they will coordinate App
changes with the frontend developer, 2026-10-13. The user separately authorizes
only this incident's rollback of App dev to df33195d398189b978040f79be5861fcf7d678c3,
including a protected remote history rollback; that exception does not authorize
future App modifications or pushes. SDK changes remain intact.

### API USDT case-duplicate repair scope (user confirmation, 2026-10-13)
User requests fixing coin sync/startup supplementation treating EVM contract case variants as separate identities, including existing duplicate data. Scope is SDK/API-wallet coin and asset identity plus the current com.sevenwallet.app simulator's duplicate records; no App source/dependency publication or transaction authorization. Source: direct user instruction in USDT12-versus6 investigation; applies to this repair.

User further confirms that only they currently test the new chains (2026-10-13, direct follow-up in this investigation). For this unreleased test rollout, existing duplicate data is handled once on their simulator; retain future duplicate prevention, without general historical cleanup at startup. This supersedes the prior repair migration design for this incident only, and does not broaden compatibility exceptions to other features/releases.

Follow-up scope (2026-10-13, user screenshot of remaining token-key UPDATEs): remove automatic old-key conversion from this unreleased fix. Current already-cleaned test DB and newly created DBs are the supported deployment inputs for this change; other unnormalized legacy DBs need separately authorized preparation before adoption. Retain canonical DAO writes and future uniqueness only; do not relocate conversion into another startup hook. This incident-specific testing scope does not authorize discarding compatibility for unrelated features.

### Scroll maximum residual optimization (user confirmation, 2026-10-14)
User requests reducing Scroll native maximum residual by optimizing fee-rate rounding and L1 reservation, with App modifications explicitly excluded. The user authorizes Core→SDK→plugin commits/pushes after validation so they can rebuild and perform the real-device retry themselves. Scope is this Scroll optimization; no authorization to sign or broadcast real transactions from Codex. Source: direct instruction in this chat, 2026-10-14.

### Collect fee payment limit (user confirmation, 2026-10-14)

The backend supports only one service-fee payment per collect tradeNo. Source: direct user answer during the first-USDT-collect failure investigation, 2026-10-14. Applies to the current SDK/backend collect protocol; do not reopen a completed fee cycle to request another payment without a separately confirmed backend protocol change. Runtime evidence for this incident shows Base/Arbitrum/Scroll fee transfers were visible in balance queries, but funding was below the renewed quote.


### Self-hosted RPC authentication (user confirmation, 2026-10-14)

The user confirms `apprpc.77wapp.vip` is their self-hosted RPC gateway and requires
an HTTP `token` header. A diagnostic request without that header returning401
is not evidence of an application/node authentication failure. Scope: this
Arbitrum collect-fee incident and the current app/gateway protocol. Source: direct
user clarification during this investigation, 2026-10-14; verify changes if the
protocol/hosts are replaced. The user requests code-side investigation without
App interaction, and explicitly prohibits real order changes and nonce rollback.

Code and live read-only evidence: `Context::get_rpc_header` obtains tokens through
`BackendApi::rpc_token` (`app/rpc/token`) with existing client headers, decrypts
using configured transport AES, then supplies `token` to RPC. Cache TTL is30min.
The code-side authenticated query succeeded without a connected device; secrets
were kept out of reports. Current chain evidence and recovery limits are recorded
in [the Arbitrum recovery note](arbitrum-collect-fee-nonce-recovery.md).

### Local Arbitrum test incident cleanup
- Source: user confirmation2026-10-14, this local emulator incident only. User
  approves backup/targeted failed-record and payer nonce cleanup, original address
  reuse and repaired local runtime; no code commit. Do not introduce a general
  recovery mechanism for this disposable test incident. This scope supersedes
  prior read-only data boundary for these local records only, not production.

### Collect-fee payer shortage policy (user confirmation, 2026-10-14)

User explicitly selects a local SDK fix: insufficient payer transfer/gas
balance during fee construction is terminal for that fee child and must be
reported; later funding must not restart the failed order. Network/timeouts
remain retryable and already-broadcast uncertainty remains observable. Source:
direct user instruction in this chat, 2026-10-14. Applies to the common
collect-fee workflow; this does not confirm a parent association field or
authorize backend/App changes, transaction execution, commit or push.

### Withdrawal failure and nonce reuse contract (2026-10-15)

User confirms persisted withdrawal error records (`err_code`/`err_msg`) represent
**deterministic failures**, not RPC timeouts or unknown broadcast outcomes.
Source: direct instruction in this chat on2026-10-15. Apply to the backend EVM
withdrawal nonce-gap correction; do not infer reusability from error wording.
User selects failure upload for withdrawals blocked by a terminal predecessor
nonce gap and safe reuse for subsequent orders; failed orders must not be revived.
Release requires no broadcast, uncertainty or chain-execution evidence. A failed
on-chain transaction can consume nonce, so chain next nonce is authoritative,
not the last business-success row. Reuse existing error/uncertainty facts and the
sender+chain gate; do not blindly reset shared allocation counters. Unknown or
other-flow reservations remain protected. No authorization to alter device orders,
sign/broadcast real transactions, commit/push or publish this next change.

Implementation safety qualification (2026-10-15): existing code also has a future
EVM uncertainty timeout that reports manual handling. Keep its uncertainty marker
when writing that report, so err_code alone never releases a potentially sent
signature. Likewise Broadcast/Recover local/observation failures after a durable
send claim retain uncertainty. This qualifies future lifecycle behavior; the
user-confirmed current incident errors remain deterministic failures.

### Collect main build nonce policy (user confirmation, 2026-10-16)
User requests that a failed collect-main build must not consume an EVM nonce,
and retries must not skip numbers. Source: direct instruction after Scroll
C2111021523509907456 used nonce5 on a failed build and nonce6 on retry while
chain next remained5. Reuse read-only candidate observation and atomic signed
facts/counter publication already used by fee orders. This authorizes the SDK
source fix and regression validation, not historical order/counter cleanup,
real transaction execution, dependency publication or App changes.

### Unsigned nonce holes and historical test recovery (2026-10-18)
Source: user confirmation in this chat. New collect/fee/withdraw orders must use
the first available unsigned slot from the observed chain frontier; a failed
build must not reserve it. Persisted signed/broadcast higher orders are retained,
not cancelled or declared invalid. For the specific historical Scroll test
incident (missing5, saved6), user explicitly accepts that filling5 can release6.
This is not a statement that signed transactions expire or that failed local
orders are revoked. Terminal signed or unknown reservations remain guarded.
No authorization to sign/broadcast real transactions, mutate runtime data,
commit or publish is implied.

### Collect gap failure reporting (2026-10-18)
Source: user confirmation in this chat after SDK d4839028. Backend does not issue
a new collect until the previous order receives a failure. A missing-receipt EVM
collect with an unsigned lower nonce gap must report business failure through
the existing receipt path and stop retrying. Signed raw/hash and nonce ownership
remain retained; this report is not on-chain cancellation. The user accepts the
old signed6 may execute in this historical test recovery. No real signing,
broadcast, runtime DB mutation, or dependency publication authorized by this fix.

### Query-uncertain collect recovery follow-up (2026-10-18)
Source: user-requested read-only check on the running Android test App. Old
C2111021523509907456 retained nonce6 and no failure receipt; recovery returned
QueryUncertain. Local high-water was153 with only signed6 above chain frontier5;
new C2111383973761585152 stayed unsigned. These are time-specific incident facts.
User requests recovery for this branch: independently verify nonce gap and reuse
the existing atomic business-failure report; preserve signed6/counter and allow
new5. Do not sign/broadcast or mutate runtime DB. Receipt failure alone is not
proof; fresh/failed nonce observation must retain the order.

### 2026-10-19：当前测试地址的归集替换授权

来源：用户本会话明确确认。仅 Scroll 测试 sender `0x5A99406CE8D9F8B3527a38408582872144C8b890`，允许新真实归集业务替换旧订单 `C2111021523509907456` 的 nonce=6，并在 canonical finalized 消耗证据后恢复从 7 分配；遗留计数器 153 保留审计。授权仅本地址、本事故，不能泛化为任意生产地址或全局计数器重置。当前阶段只改代码及离线验证，禁止真实签名、广播和运行库修改。时效：配置激活前须以当时链上结果决定赢家；历史签名不能因本地失败视为失效。

### 2026-10-19：手机 App 自动识别归集替换

来源：用户本会话澄清 App 指手机钱包，明确要求检测到同类 nonce 故障由 App 自动处理，不能每个地址改代码、配开关或制作专用包。此确认替代上段“只有单地址手动配置入口”的产品限制；不授权执行真实交易。范围：新有效归集与已上报 nonce gap 失败的旧签名属于同一用户、链、发送地址、原生资产、收款目标时，自动发现并复用持久化替换关联；费用、签名身份、共享 nonce 占用及 canonical finalized 对账保护保留。只有签名唯一占号和链上独立观测满足条件才可继续；未知结果不可视为取消。构建失败不占号的归集/手续费/提币公共规则不变。本轮只改代码和离线验证，仍禁止真实签名、广播和设备数据库修改。历史配置字段保留兼容，但正常自动识别不依赖配置。

### 2026-10-19：Android 栈溢出修复与验证边界

来源：本会话用户明确要求先核对崩溃前签名广播，再修复并验证手机端恢复，禁止执行真实交易。范围：归集命令分发的异步栈占用；复用原worker逻辑，用堆边界避免大型命令状态被每个dispatcher分支内嵌，不改变nonce、金额、签名、广播、取消或并发语义。Android验证只允许运行隔离原生测试程序、临时数据库、合成身份和本地假RPC，不启动实际钱包App，不修改设备运行库。此次实现授权不包含提交推送。

### 2026-10-19：替换归集恢复沿用既有确认流程（用户纠正）

来源：用户本会话明确拒绝备用RPC切换方案，指出既有系统没有此机制，并要求修复此前额外finalized门槛引入的卡单。此确认替代前文针对已知旧/新签名身份要求L1 finalized后才对账的设计；未提交的备用节点实现撤回。范围：当前选定RPC查到已知旧/新hash的有效上链回执，核对发送地址、nonce、区块身份、canonical成员与区块复读后，按普通归集的上链确认语义进入原有落库/上报流程，不等L1 finalized、不切备用节点。不把pending/latest nonce或缺失/错误回执当作成功；未知第三方消费冲突仍保留既有独立finalized证明。遗留153按已有last-used语义在对账事务校正到已消耗6，使下一候选为7，只有签名落库才占7；原153保留审计，更高签名/不确定占用不能被覆盖。本轮只改代码和隔离验证，禁止真实签名广播、App启动、直接修改手机运行库；未授权提交推送。


### 2026-10-19：nonce恢复必须使用通用事实，不保留指定地址入口

来源：用户本会话确认，要求核查整轮恢复改动，禁止把nonce=6/7、153、地址或订单号写成生产特例；能够通用的复用公共流程。范围：本次审查基线bef68f7c及相关nonce历史实现。

- 已核实生产候选、旧签名和计数器恢复来自链观测、持久化订单与替换关联，无测试地址/订单号/nonce数值条件。回归用例可保留历史6/153，新增不同链、地址和nonce的SQLite证据。
- 移除已无必要的`collect_replacement_recovery`配置字段及worker优先选择分支；恢复只从订单自动发现或已有持久化关联继续。旧YAML字段按既有serde未知字段行为忽略，不能选择地址、链或nonce。此确认替代早期保留指定dev/Scroll配置入口的说明。
- 归集、手续费和提币共享nonce候选和签名占用检查、事务提交；未签名构建失败不占号。自动替换与赢家对账只针对同用户/链/发送地址/收款目标的原生EVM归集，其他业务不能据此被自动替换或判定取消。
- 历史已签名跳号不是可直接释放的未签名失败；保留签名身份，符合关联条件才允许业务替换。有可靠链上赢家后恢复最后使用nonce N，下一候选N+1；并发更高占用和未决签名不得抹除。证据不足继续保守处理，不自动发送补号/取消交易。
- 本轮仅源码与离线回归，不含提交推送、真实签名广播、运行库修改或真实订单执行。


### 2026-10-20：移除未签名预分配nonce的遗留接口

来源：用户在本会话审查结论后要求处理旧公开加号入口。范围：原NonceEngine allocate_nonce/allocate_nonce_synced及其专用fast/slow调用链、ApiNonceRepo递增接口与DAO对应SQL。当前业务未调用这些入口，但公开保留会使未来调用者重新引入构建前占号风险，因此删除而非新增配置或平行恢复器。

生产入口继续只读观测候选，由归集/手续费/提币既有签名事务复核共享占用并持久化；reconcile/自愈追平链前沿的单调floor逻辑保留。公开预分配API已移除，外部旧调用者应改走既有业务签名流程，不能仅改成手工counter+1。无schema迁移，无手机订单数据修补。编译失败文档用例证明旧公开方法不可调用；并发候选相同是预期行为，事务只允许一个签名占用，同一候选的失败重试不应增加counter。

本轮授权仅修改和离线验证，不含提交推送、真实签名广播或实际App执行；自动替换仍限于已有原生EVM归集业务边界。

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

- 只负责 SQL、Entity 映射、持久化读写和幂等更新。
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
- 资源代理成功时：
  - collect / withdraw 都在 `SendResourceResultAck` 成功后释放原订单 gate
  - `resource_gate_result = resource_delegation_success`
- 资源代理失败时：
  - collect / withdraw 都在 `UploadResourceTxExecReceipt` 成功后做 bypass 释放
  - 失败事实保留在 `api_resource_delegation`
  - `resource_gate_result = resource_delegation_failed_bypass`
- 资源代理失败不会阻塞 collect / withdraw 主流程；释放 gate 后由 scanner
  基于事实重新推进到 `BuildTx`。

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

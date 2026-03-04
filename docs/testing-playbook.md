# Testing Playbook

## A. Purpose & Scope

### 目的

- 统一 feature 流程的增量测试覆盖方法，避免每次重新设计测试方案。
- 让新增 flow 或模块都可以按固定步骤推进到“可验证、可回归”状态。

### 适用范围

- 适用于所有包含 Service / Domain / Repo / Backend 交互的 feature flow。
- 适用于单元测试、集成测试、smoke 测试的分层推进。

### 非目标

- 不要求为测试覆盖做业务重构。
- 不替代业务设计文档与架构设计文档。

## B. Hard Constraints

- 不引入新的业务逻辑。
- 不做大规模 Service / Domain 结构重构。
- 测试不可依赖真实 backend、真实网络或外部不稳定环境。
- 优先复用项目现有测试基建（fake backend、task noop、temp sqlite、serial）。
- 涉及失败路径时，必须验证不变性（DB/状态不被污染）。

## C. Iteration Workflow

### Iteration 0：基线稳定

- 固定测试命令与执行入口（本地与 CI 一致）。
- 固定串行策略（serial）与全局单例隔离策略（例如 OnceCell）。
- 识别并消除随机性来源（时间、随机值、全局状态、后台任务）。
- 确保测试离线可跑（无 TCP mock server、无真实 backend）。

### Iteration 1：黄金路径最小闭环

- 先选一条最核心、依赖最少的成功路径打通。
- 补齐最小断言矩阵：DB 变化 + backend 调用记录。
- 建立可复用 fixture/helper，避免重复 setup 代码。

### Iteration 2：错误路径覆盖

- 每条 flow 至少补 1 个错误路径。
- 错误来源至少覆盖一个：backend error / db not found / binding 缺失 / uid status mismatch。
- 每个错误用例都验证：错误返回 + 不变性 + 调用边界。

### Iteration 3：编排回归锁定

- 不改 Service 编排逻辑，使用测试锁定现有行为。
- 重点验证：调用顺序、调用次数、远端/本地先后顺序、失败不落库。
- 至少新增 2 个编排回归用例。

### Iteration 4：轻量 Domain 请求组装验证（可选）

- 仅对请求组装/转发函数做轻量验证。
- 对强 DB 事务行为，不强制做 domain 纯单测，优先由 service 集成测试覆盖。

## D. Test Case Template

每个测试用例使用以下模板：

- 用例名：
- 入口函数：
- 前置数据/夹具：
- fake/mock 配置：
- 执行步骤：
- 断言（至少包含 DB 状态 + backend 调用记录）：
- 覆盖的分支/风险点：
- 预计新增代码范围：

## E. Assertion Matrix Template

每个 flow 至少填写一行矩阵：

| Flow      | 输入组合（关键参数） | 预期 backend 调用（接口/次数/关键字段） | 预期 DB 变化（表/字段） | 失败时不变性（必须保持不变字段） |
|-----------|----------------------|-----------------------------------------|-------------------------|----------------------------------|
| 示例 flow | 参数组合             | API + count + fields                    | 表与字段变更            | 不变字段列表                     |

### 填写方式

- 先写主成功路径（Happy Path）。
- 再补失败路径（至少 1 条）。
- 每条失败路径必须明确“不变字段”，用于防止回归污染。

## F. Fixture/Helper Standard

通用 fixture/helper 建议职责如下：

- `ensure_env`：构建并复用测试环境（单例上下文）。
- `prepare_*`：构造测试数据与最小依赖数据。
- `reset_fake`：每个测试开始前清理 fake 状态。
- `snapshot_*`：采集执行前后关键状态快照。
- `assert_*_call`：断言 backend 调用次数与关键字段。

约束：

- helper 只负责数据准备与断言支持。
- helper 不承载业务判断，不替代业务逻辑本身。

## G. Stability Checklist

每次提测前逐项确认：

- serial 是否开启（若存在全局单例状态）。
- 是否全部使用 fake backend。
- task 执行是否为 noop 或可控模式。
- 断言是否依赖时间戳/随机值（应避免）。
- 是否包含至少一个失败路径不变性断言。
- 是否确认无真实网络调用、无 TCP listener 依赖。

## H. Definition of Done (DoD)

- 基线 smoke 测试稳定通过（建议重复执行 3 次）。
- 每条 flow 至少有 1 条错误路径测试。
- 至少有 2 个编排回归测试（适用时）。
- 全部测试不依赖真实网络/backend。
- 已完成 flow 级断言矩阵文档化，可供后续复用。

# API Wallet Testing Playbook

## Purpose

- 提供 API Wallet flow 的可复用测试覆盖流程。
- 保证新增/修改流程时，按固定迭代推进，不遗漏回归风险。

## Hard Constraints

- 不引入新业务逻辑。
- 不做大规模 Service / Domain 重构。
- 测试不可依赖真实 backend 或真实网络。
- 优先复用已有测试基建（fake backend、task noop、temp sqlite、serial）。
- 失败路径必须验证不变性（DB/状态不被污染）。

## Iteration Workflow

### Iteration 0：基线稳定

- 固定测试命令与执行入口（本地/CI一致）。
- 使用 serial 与单例隔离（如 OnceCell CONTEXT）。
- 消除随机性：不对时间戳/随机值做直接断言。
- 确保离线可跑（无 TCP mock server、无真实 backend 依赖）。

### Iteration 1：黄金路径闭环

- 先覆盖 1 条核心成功链路。
- 完成最小断言矩阵：DB 变化 + backend 调用记录。
- 抽出公共 fixture/helper。

### Iteration 2：错误路径覆盖

- 每条 flow 至少 1 条错误路径。
- 错误来源至少覆盖一类：backend error / db not found / binding 缺失 / uid status mismatch。
- 验证错误返回 + 调用边界 + 不变性。

### Iteration 3：编排回归锁定

- 不改 Service 编排逻辑，用测试锁行为。
- 验证调用顺序、调用次数、远端/本地先后与失败不落库。
- 至少 2 个编排回归用例。

### Iteration 4：Domain 轻量验证（可选）

- 仅验证请求组装/转发函数。
- 强 DB 行为优先由 Service 集成测试覆盖，不强制做纯 Domain 单测。

## Test Case Template

- 用例名：
- 入口函数：
- 前置数据/夹具：
- fake/mock 配置：
- 执行步骤：
- 断言（至少包含 DB 状态 + backend 调用记录）：
- 覆盖的分支/风险点：
- 预计新增代码范围：

## Assertion Matrix Template

| Flow      | 输入组合（关键参数） | 预期 backend 调用（接口/次数/关键字段） | 预期 DB 变化（表/字段） | 失败时不变性（必须保持不变字段） |
| --------- | -------------------- | --------------------------------------- | ----------------------- | -------------------------------- |
| 示例 flow | 参数组合             | API + count + fields                    | 表字段变化              | 不变字段列表                     |

### 填写规则

- 先写主成功路径，再补失败路径。
- 每条失败路径必须明确“不变字段”。

## Fixture / Helper Standard

- `ensure_env`：构建并复用单例测试环境。
- `prepare_*`：准备测试数据与依赖数据。
- `reset_fake`：每个测试前重置 fake 状态。
- `snapshot_*`：采集执行前后状态快照。
- `assert_*_call`：断言 backend 调用次数与字段。

> Helper 只做数据准备与断言支持，不承载业务判断。

## Stability Checklist

- serial 已开启（若有全局单例状态）。
- 全部 backend 调用走 fake。
- task 执行为 noop 或可控模式。
- 断言不依赖时间戳/随机值。
- 包含失败路径不变性断言。
- 无真实网络调用、无 TCP listener 依赖。

## Definition of Done (DoD)

- 基线 smoke 建议连续跑 3 次稳定通过。
- 每条 flow 至少 1 条错误路径用例。
- 至少 2 个编排回归用例（适用时）。
- 全部测试离线可跑。
- 断言矩阵已更新。

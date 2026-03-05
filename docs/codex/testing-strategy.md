# Testing Strategy

## Goal

建立统一、低波动、可复用的测试覆盖方法，保证 feature 变更可以：

- 快速验证（短反馈周期）
- 离线验证（不依赖真实网络/backend）
- 回归可控（覆盖成功路径 + 失败不变性）

## Hard Constraints

- 不引入新的业务逻辑语义。
- 不做大规模 Service / Domain 重构，优先最小改动。
- 测试默认离线运行，使用 fake/mock 与本地临时数据。
- 失败路径必须验证不变性（DB/状态不被污染）。

## Standard Iteration Model

### Iteration 0 — Baseline Stability

- 固定命令与执行入口（本地/CI一致）。
- 固定隔离策略（serial / OnceCell / task noop）。
- 消除随机性断言（不直接断言时间戳与随机值）。

### Iteration 1 — Golden Path

- 先覆盖 1 条核心成功链路。
- 完成最小断言矩阵：DB 变化 + backend 调用记录。

### Iteration 2 — Error Paths

- 每条 flow 至少 1 条失败路径。
- 优先覆盖 backend error / not found / status mismatch / binding 缺失。

### Iteration 3 — Orchestration Regression

- 锁定调用顺序、调用次数、远端/本地先后顺序。
- 至少 2 个“失败不落库”或“边界顺序”回归用例。

### Iteration 4 — Lightweight Domain Forwarding (Optional)

- 仅验证请求组装与转发逻辑。
- 强 DB 行为优先由 service/integration tests 覆盖。

## Test Case Template

- 用例名：
- 入口函数：
- 前置数据/夹具：
- fake/mock 配置：
- 执行步骤：
- 断言（至少 DB 状态 + backend 调用记录）：
- 覆盖分支/风险点：
- 预计改动范围：

## Assertion Matrix Template

| Flow      | 输入组合（关键参数） | 预期 backend 调用（接口/次数/字段） | 预期 DB 变化（表/字段） | 失败不变性（必须保持不变字段） |
|-----------|----------------------|-------------------------------------|-------------------------|--------------------------------|
| 示例 flow | 参数组合             | API + count + fields                | 表字段变化              | 不变字段列表                   |

填写原则：

- 先写成功路径，再写失败路径。
- 每条失败路径明确“不变字段”。

## Fixture / Helper Standard

- `ensure_env`: 构建并复用测试环境
- `prepare_*`: 准备测试数据
- `reset_fake`: 每个测试前重置 fake 状态
- `snapshot_*`: 采集前后快照
- `assert_*_call`: 调用次数与字段断言

约束：helper 只做数据准备与断言支持，不承载业务判断。

## API Wallet Example (Reference)

对于 `import_api_wallet` / `scan_bind` / `import_bind` 这类流程：

- 默认使用 `FakeApiWalletBackend + temp sqlite + serial + task noop`
- 核心断言包括：
  - wallet relation / app_id / merchant_id / sn 等关键字段落库
  - backend 调用接口、次数与请求字段准确
  - 失败路径下字段保持不变

## Definition of Done

- 基线 smoke 连续执行 3 次稳定通过（建议）
- 每条关键 flow 至少 1 条错误路径测试
- 至少 2 个编排回归测试（适用时）
- 断言矩阵已更新
- 不依赖真实网络/backend

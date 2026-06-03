# Testing Backend Boundaries

## Purpose

本文档记录 `wallet-api` 集成测试里的外部 backend 边界。
目标不是描述所有测试细节，而是回答三个问题：

- 哪些调用已经可以被 fake backend 控制。
- 哪些调用仍然可能绕过 fake backend。
- 后续新增或迁移测试时，应如何扩展 fake backend。

## Core Rule

标准集成测试必须离线可运行。
测试可以调用真实业务入口，例如 `WalletManager` 或 worker step，但这些入口
背后的外部依赖必须被 fake、mock 或本地测试资源接管。

也就是说：

- 业务流程是真的。
- 本地 DB 和测试数据是真的。
- backend、链 RPC、通知通道等外部世界必须是假的或可记录的。

## Current Api-Wallet Boundary

`wallet-api` 目前已有一条比较清楚的 API wallet backend 边界：

- Production boundary:
  `wallet-api/src/context/api_wallet_backend.rs`
- Test fake:
  `wallet-api/tests/harness/mod.rs`
- Test injection:
  `WalletManager::new_for_test(...)`

`ApiWalletBackend` 当前覆盖这些接口：

- `wallet_bind_appid`
- `init_api_wallet`
- `old_keys_init`
- `appid_import`
- `appid_import_recharge_wallet`
- `keys_uid_check`
- `query_uid_bind_info`
- `appid_uid_usage`

对应的 `FakeApiWalletBackend` 已经实现这些接口，并提供三类能力：

- 返回值队列，例如 uid 状态、绑定信息、使用情况。
- 错误注入，例如绑定失败、导入失败、查询失败。
- 调用记录，例如绑定参数、导入参数、uid 查询参数。

这条边界适合 `import_bind` 这类集成测试。测试调用 manager 方法时，业务代码
会走 trait，测试环境注入 fake，因此不会访问真实 backend。

## Current Gaps

目前还没有做到所有 backend 调用都统一经过 `ApiWalletBackend`。
代码里仍有大量直接访问：

```rust
CONTEXT.get().unwrap().get_global_backend_api()
```

这类调用拿到的是 `wallet_transport_backend::api::BackendApi`，不是当前
`FakeApiWalletBackend`。如果某个集成测试触发了这条路径，测试就可能依赖真实
backend 配置，或者只能靠另一个 worker fake 去截获请求。

需要特别注意的区域：

- `wallet-api/src/domain/api_wallet/wallet.rs`
  大部分 import/bind 方法已经走 `get_api_wallet_backend()`，但
  `query_wallet_activation_info` 仍然走 `get_global_backend_api()`。
- `wallet-api/src/application/api_wallet_withdraw.rs`
  withdraw orchestration 当前直接使用 global backend。
- `wallet-api/src/infrastructure/api_trans/**`
  collect、withdraw、fee、resource operation、resource reclaim 的 worker
  里还有多处 global backend 调用。
- `wallet-api/src/service/**`
  app、wallet、multisig、jpush 等 service 仍有多处 global backend 调用。
- `wallet-api/src/domain/assets/mod.rs`
  assets 查询和同步路径仍有 global backend 调用。

因此，现在的结论是：

- `import_bind` 的部分测试边界较清楚。
- 整个 `wallet-api` 还没有完成统一 backend fake 化。
- 后续扩展集成测试时，不能默认认为所有 manager 调用都已经离线可信。

## Design Standard

新增或迁移 backend 相关集成测试时，按下面规则处理。

### 1. Define A Boundary Before Writing Scenarios

先确认被测 flow 会触发哪些外部接口。
如果接口还在走 `get_global_backend_api()`，不要直接写场景断言。
先决定本轮是否要：

- 把该接口纳入一个 trait boundary。
- 或者确认已有 worker-level fake 能稳定截获该请求。
- 或者把用例降级为 `smoke/live`，手动运行。

### 2. Fake The Interface, Not The Scenario

fake backend 应该对应业务接口，而不是对应单个测试场景。

推荐：

```text
ApiWalletBackend
  FakeApiWalletBackend
    records calls
    queues configured responses
    injects scoped errors or delays
```

不推荐：

```text
fake_scan_bind_success_for_test_a
fake_scan_bind_timeout_for_test_b
fake_scan_bind_error_for_test_c
```

场景差异应通过配置 fake 的返回、错误、延迟、调用记录来表达。

### 3. Unconfigured Calls Should Be Visible

fake backend 不应该悄悄返回过于宽松的默认值。
如果某个接口在测试中没有被配置，但业务代码调用了它，测试应能暴露这个事实。

可接受的方式：

- 明确 panic，提示哪个接口没有配置。
- 返回明确测试错误。
- 记录调用并由 `then` 断言调用次数和参数。

### 4. Scoped Network Behavior

网络特征应该按 backend 行为命名，而不是按某个业务接口命名。

推荐：

```rust
let _network = given.backend_network_is_slow();
```

不推荐：

```rust
let _delay = given.appid_import_delay();
```

原因是慢请求、超时、重试、乱序响应是网络接口的通用特征。
这些能力成熟后，可以抽到 harness 的通用 backend fake 配置里。

### 5. Assert Both Result And Boundary

集成测试不能只断言返回值。
凡是触发 backend 的 flow，至少要断言：

- 业务返回值。
- DB 状态或事实字段。
- backend 调用次数。
- backend 调用参数。
- 失败路径下不应发生的调用。

例如 `scan_bind` 的 remote-first 场景，不只断言结果是错误，还要断言 fake
backend 确实被调用一次，并且参数是预期的钱包对和 app id。

## Migration Order

不要一次性把所有 `get_global_backend_api()` 都改掉。
按 flow 迁移更安全：

1. 保持 `import_bind` 作为当前参考样板。
2. 把通用 error、delay、call assertion 从 flow-local support 逐步上移到
   `tests/harness`，前提是至少两个 flow 需要同一能力。
3. 优先处理仍在 API wallet domain 内的漏网接口，例如
   `query_wallet_activation_info`。
4. 再按 flow 处理 withdraw、collect、fee、resource operation、resource
   reclaim worker。
5. 对真实环境联通性测试，统一放到 `smoke/live`，默认测试和 CI 主链不运行。

## Review Checklist

迁移或新增 backend 相关测试时，review 先看这些问题：

- 测试入口是否可能触发真实 backend。
- 被测 flow 的所有 backend 接口是否有 fake 或明确 smoke 标记。
- fake 是否记录了调用。
- `then` 是否断言了调用次数和关键参数。
- 失败路径是否断言没有多余副作用。
- 测试正文是否仍然是业务剧本，而不是 backend mock 细节堆叠。

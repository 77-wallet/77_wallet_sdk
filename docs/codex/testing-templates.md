# Testing Templates

本文件是可复制的测试模板手册。先用 `docs/codex/testing.md` 判断测试层级，
再从本文复制对应模板。

## One-Screen Contract

新增或重写测试时，先满足这几条：

- Unit / Component / Smoke 使用 Arrange-Act-Assert。
- Integration 使用 Given-When-Then。
- 默认测试必须离线，不碰真实 backend、真实链 RPC、固定 `test_data`。
- 真实环境验证只能放到 `tests/smoke/`，并且必须 `#[ignore]`。
- Integration 正文不能暴露 SQLite、JSON、channel、backend 解密等底层噪音。
- 每个关键 flow 至少有成功路径和一个失败不变性断言。
- 涉及 flow 覆盖变化时，同步 `docs/codex/assertion-matrices/`。

## Layer Decision

选择能证明行为的最低层级。

| Layer | 用来证明 | 允许 | 禁止 |
| --- | --- | --- | --- |
| Unit | 一个函数、规则、状态判断 | 小 fixture、纯断言 | 网络、DB、manager、全局 context |
| Component | 一个模块和本地依赖协作 | 临时 SQLite、真实 DAO/repo | 真实 backend、真实链 RPC |
| Integration | 业务 flow 编排 | fake backend/chain、临时数据 | 真实远端、固定 `test_data` |
| Smoke | 真实环境联通性或手工验证 | 真实 backend、真实链 RPC、固定地址 | 默认执行、打印敏感信息 |

不要因为某个 helper 已经存在就升层。能用 Unit 证明的，不写 Integration。

## Read Order

读复杂 integration flow 时按这个顺序：

1. 看目录名：确认业务模块和 flow。
2. 看 `mod.rs`：只读测试名和 Given-When-Then 正文。
3. 看 assertion matrix：确认覆盖了哪些风险。
4. 再看 `support/scenario.rs`：理解每个业务步骤背后的动作。
5. 最后才看 `support/db.rs`、`fixtures.rs`、payload、backend recorder。

如果必须先读 helper 才知道测试在测什么，模板就失败了。

## Directory Contract

```text
<crate>/
  src/...                         # unit / component tests near code
  src/testkit/                    # crate-private test entrypoints only
  tests/
    harness/                      # cross-flow environment and fakes
    integration/<module>/<flow>.rs
    integration/<module>/<flow>/
      mod.rs                      # read-first test cases
      support.rs                  # small facade, if support stays small
      support/
        mod.rs                    # facade, if support needs families
        scenario.rs               # given / when / then public surface
        fixtures.rs               # immutable test inputs
        db.rs                     # SQL / repository setup
        assertions.rs             # low-level assertions
    smoke/<module>/<flow>.rs
```

Use a single `<flow>.rs` only when the whole file stays easy to scan.
Once fixture, SQL, fake backend, payload, or assertions become noisy, split to
`<flow>/mod.rs` plus `support`.

## Naming Contract

Test names use behavior, not implementation:

```text
<flow>_<condition>_<expected_result>
```

Good:

- `withdraw_tx_ack_backend_failure_keeps_fact_unset`
- `collect_receipt_duplicate_upload_is_idempotent`
- `import_bind_backend_reject_does_not_persist_relation`

Avoid:

- `test_success`
- `test_transfer`
- `test_1`
- `mock_test`

Helper names are a small vocabulary:

| Prefix | Meaning | Where |
| --- | --- | --- |
| `given_*` | 准备业务事实或 fake 行为 | Integration scenario |
| `when_*` | 执行一个业务入口、worker step、scanner step | Integration scenario |
| `then_*` | 断言业务结果、DB、backend、通知、副作用 | Integration scenario |
| `seed_*` | 插入 DB 事实 | support detail |
| `load_*` | 读取当前状态 | support detail |
| `count_*` | 统计副作用次数 | support detail |
| `assert_*` | 底层字段断言 | support detail |

`given/when/then` 是测试正文语言。`seed/load/count/assert` 是底层工具。

## Integration Template

Integration 的目标是让 `mod.rs` 像业务剧本，而不是像搭环境脚本。

### `mod.rs`

```rust
mod support;

use serial_test::serial;

use support::{FlowFixture, FlowScenario};

#[tokio::test]
#[serial]
async fn flow_happy_path_writes_facts_and_notifies_backend() {
    let scenario = FlowScenario::new().await;
    let fixture = FlowFixture::new("happy");

    scenario.given_target_order(&fixture).await;
    scenario.given_backend_accepts_target_call(&fixture);

    let result = scenario.when_target_step_runs(&fixture).await;

    scenario.then_step_succeeds(result);
    scenario.then_db_facts_are_complete(&fixture).await;
    scenario.then_backend_called_once(&fixture).await;
    scenario.then_notification_was_emitted(&fixture);
}

#[tokio::test]
#[serial]
async fn flow_backend_failure_keeps_fact_unset_and_retryable() {
    let scenario = FlowScenario::new().await;
    let fixture = FlowFixture::new("backend_fail");

    scenario.given_target_order(&fixture).await;
    scenario.given_backend_rejects_target_call(503, "temporary failure");

    let result = scenario.when_target_step_runs(&fixture).await;

    scenario.then_step_is_retryable(result);
    scenario.then_backend_called_once(&fixture).await;
    scenario.then_success_fact_is_not_persisted(&fixture).await;
    scenario.then_retry_scanner_can_pick_it_again(&fixture).await;
}
```

`mod.rs` 只允许出现：

- `mod support;`
- 少量 `use`
- 测试用例
- 非常轻的结果断言 helper

不允许出现：

- SQL
- `SqliteContext`
- `serde_json::to_value`
- backend recorder 解密
- channel 创建
- 大段 payload 构造

### `support.rs`

小 flow 可以用一个 `support.rs`：

```rust
use crate::harness::{ensure_worker_env, next_unique_id, WorkerTestEnv};

pub(super) struct FlowFixture {
    pub uid: String,
    pub trade_no: String,
}

impl FlowFixture {
    pub(super) fn new(prefix: &str) -> Self {
        let id = next_unique_id();
        Self {
            uid: format!("uid_{prefix}_{id}"),
            trade_no: format!("T_{prefix}_{id}"),
        }
    }
}

pub(super) struct FlowScenario {
    env: &'static WorkerTestEnv,
}

impl FlowScenario {
    pub(super) async fn new() -> Self {
        let env = ensure_worker_env().await;
        env.recorder.reset();
        Self { env }
    }

    pub(super) async fn given_target_order(&self, fixture: &FlowFixture) {
        seed_target_order(self.env.db_dir(), fixture).await;
    }

    pub(super) fn given_backend_accepts_target_call(
        &self,
        fixture: &FlowFixture,
    ) {
        self.env.recorder.expect_target_call(&fixture.trade_no);
    }

    pub(super) fn given_backend_rejects_target_call(
        &self,
        status: u16,
        body: &str,
    ) {
        self.env.recorder.fail_next_api_backend_call(status, body);
    }

    pub(super) async fn when_target_step_runs(
        &self,
        fixture: &FlowFixture,
    ) -> Result<(), ServiceError> {
        run_target_worker_step(&fixture.trade_no).await
    }

    pub(super) fn then_step_succeeds(
        &self,
        result: Result<(), ServiceError>,
    ) {
        result.expect("target step should succeed");
    }

    pub(super) fn then_step_is_retryable(
        &self,
        result: Result<(), ServiceError>,
    ) {
        result.expect("retryable failure should not crash worker");
    }

    pub(super) async fn then_db_facts_are_complete(
        &self,
        fixture: &FlowFixture,
    ) {
        let saved = load_target_order(self.env.db_dir(), &fixture.trade_no).await;
        assert!(saved.success_fact_at.is_some());
    }

    pub(super) async fn then_success_fact_is_not_persisted(
        &self,
        fixture: &FlowFixture,
    ) {
        let saved = load_target_order(self.env.db_dir(), &fixture.trade_no).await;
        assert!(saved.success_fact_at.is_none());
    }
}
```

If `support.rs` grows past one easy screen, turn it into a facade:

```rust
mod assertions;
mod db;
mod fixtures;
mod scenario;

pub(super) use fixtures::FlowFixture;
pub(super) use scenario::FlowScenario;
```

### Integration Rules

- One test should have one primary `when_*`.
- A second `when_*` is allowed only for retry, idempotency, recovery, or time.
- `Scenario` owns environment, fake backend, DB pools, and notification capture.
- `Fixture` owns unique immutable input data.
- Reset fake state in `Scenario::new`.
- Assert both DB facts and external calls when the flow has both.
- Keep business assertions visible through `then_*` names.
- Move only proven cross-flow capability into `tests/harness`.

## Component Template

Use Component when the behavior needs SQLite/repository/domain collaboration
but not backend, chain RPC, manager, or global context.

```rust
#[cfg(test)]
mod tests {
    use tempfile::TempDir;

    struct TestDb {
        _dir: TempDir,
        pool: TargetDbPool,
    }

    impl TestDb {
        async fn new() -> Self {
            let dir = tempfile::tempdir().expect("tempdir");
            let pool = open_temp_pool(dir.path()).await;
            Self { _dir: dir, pool }
        }

        async fn seed_target(&self, trade_no: &str) {
            seed_target_row(&self.pool, trade_no).await;
        }
    }

    #[tokio::test]
    async fn confirm_success_writes_expected_fact() {
        let db = TestDb::new().await;
        let trade_no = "T_COMPONENT_CASE";
        db.seed_target(trade_no).await;

        let outcome = TargetDomain::confirm(&db.pool, trade_no)
            .await
            .expect("confirm step");

        assert!(outcome.should_notify);
        let saved = load_target_row(&db.pool, trade_no).await;
        assert!(saved.confirmed_at.is_some());
        assert!(saved.failed_at.is_none());
    }
}
```

Component rules:

- Use temp SQLite only.
- Do not call `WalletManager::new`.
- Do not depend on `tests/harness`.
- Do not read or write fixed `test_data`.
- Assert persisted DB fields, not only return values.

## Unit Template

Use Unit for pure rules, mapping, validation, parser behavior, or one
single-step decision.

```rust
#[test]
fn diagnose_withdraw_waits_for_audit_when_tx_ack_sent() {
    let withdraw = WithdrawFixture::new()
        .tx_ack_sent()
        .without_audit_result()
        .build();

    let diagnosis = diagnose_withdraw(&withdraw);

    assert_eq!(diagnosis.stage, AdvancementPoint::CanBuild);
    assert_eq!(diagnosis.next_fact, Some("audit_passed_at"));
}
```

Unit rules:

- No SQLite.
- No network.
- No manager.
- No global context.
- Prefer small fixtures over large object graphs.

## Smoke Template

Use Smoke only for real backend, real chain RPC, fixed addresses, local config,
or operator checks.

```rust
#[tokio::test]
#[ignore = "requires live backend, chain RPC, and local smoke config"]
async fn live_flow_reaches_remote_system() -> anyhow::Result<()> {
    wallet_utils::init_test_log();
    let (manager, _params) = get_manager_with_config("client4.toml").await?;
    manager.init_api_swap().await?;

    let result = manager.live_entrypoint().await;

    tracing::info!("live flow result: {result:?}");
    Ok(())
}
```

Smoke rules:

- Every smoke test must have `#[ignore = "..."]`.
- The ignore reason must say what real dependency is required.
- Do not print private keys, mnemonics, tokens, or production config.
- Local smoke config must be ignored by git.
- Smoke does not replace Unit, Component, or Integration coverage.

## Helper Ownership

Use the narrowest owner.

| Owner | Put here | Do not put here |
| --- | --- | --- |
| Same test file | one-flow seed/assert helpers | shared environment |
| `<flow>/support` | scenario、fixture、SQL、assertions | other flow logic |
| `tests/harness` | fake backend、temp env、collector | business decisions |
| `src/testkit` | crate-private step entrypoint | environment setup |

`harness` is professional test terminology for the shared test rig. It should
feel boring: environment, fake, recorder, collector, fixture primitives.

`testkit` is code-side access for tests. It exposes internal steps; it does not
own the test environment.

## Coverage Checklist

For each important Integration flow, add coverage in this order:

1. Happy path writes final DB facts and sends expected external calls.
2. Input failure returns/records error and does not send side effects.
3. Backend failure keeps success facts unset and leaves the flow retryable.
4. Chain failure records the correct failed fact and avoids success side effects.
5. Duplicate message or retry is idempotent.
6. Recovery resumes from persisted facts.
7. Concurrent execution sends the critical side effect once.

Do not add all seven in one batch by default. Start with happy path plus the
most likely failure invariant.

## Assertion Matrix

When a flow changes, update or add a file under:

```text
docs/codex/assertion-matrices/
```

Each entry should include:

- Flow
- Test entrypoint
- Layer
- Expected DB facts
- Expected backend calls
- Failure invariant
- Remaining coverage gaps

Keep the matrix close to the tests. It is the map; tests are the proof.

## Anti-Patterns

Do not add tests that only do this:

```rust
let res = manager.some_call().await;
tracing::info!("res: {res:?}");
Ok(())
```

Classify them:

- If they need real backend or fixed local state, move to Smoke.
- If they can be asserted offline, turn them into Unit, Component, or
  Integration tests.

Also avoid:

- `common` dumping ground modules.
- `support.rs` shared across unrelated flows.
- `withdraw` importing helpers from `collect`.
- fixed IDs without uniqueness.
- hidden assertions inside `given_*`.
- business behavior hidden in `tests/harness`.

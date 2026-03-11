# PLANS

Current task execution plan.
Refs: `docs/codex/testing.md`, `docs/codex/checklists/pr-definition-of-done.md`.

## Task

- Name: wallet-api collect rebuild latest-strategy address consistency fix
- Goal:
  - 让归集 `BuildTx/Rebuild` 在未最终上链前始终使用最新归集策略地址作为 `to`
  - 让同一轮构建产生的 `to_addr` / `raw_tx` / `tx_hash` / 回执上报 `to` 保持一致
  - 用最小回归测试锁定“重建地址未刷新”和“上报地址与链上 hash 不一致”这两个缺陷

## Batch Scope

### In

- `wallet-api/src/infrastructure/api_trans/collect/shadow/worker/collect_worker.rs`
- `wallet-api/src/infrastructure/api_trans/collect/shadow/worker/side_effect_worker.rs`
- `wallet-api/src/infrastructure/api_trans/collect/process_collect_tx_send.rs`
- `wallet-api/src/infrastructure/api_trans/collect/process_collect_tx_report.rs`
- `wallet-api` 中与归集 shadow/report flow 对应的最小测试补充
- `PLANS.md`

### Out

- `wallet-database` schema / migration / entity 字段新增
- 后端策略服务实现改动
- 非归集 flow（提现 / 手续费归集）语义调整

## Constraints

- Keep this round within one crate and one flow
- Reuse existing `api_collect.to_addr` as the current built execution address
- Do not introduce schema changes in this batch
- Prefer offline-stable tests that avoid real chain / real backend

## Plan

1. Change collect BuildTx/Rebuild logic to always refresh `to_addr` from the latest strategy before fee/build steps
2. Ensure tx-exec-receipt/report paths always upload the persisted `to_addr` that matches the current built `raw_tx`/`tx_hash`
3. Add focused regression tests for address refresh and report payload consistency

## Validation Commands

- `cargo test -p wallet-api collect_rebuild_refreshes_to_addr -- --nocapture`
- `cargo test -p wallet-api collect_blockhash_rebuild_clears_stale_build_facts_and_persists_new_to_addr -- --nocapture`
- `cargo test -p wallet-api collect_tx_exec_receipt_uses_persisted_to_addr -- --nocapture`

## Stop Condition

- Stop after collect BuildTx refreshes `to_addr` on rebuild and tx-exec-receipt payload uses the persisted execution address consistently
- Do not expand into database schema additions or backend strategy-service changes in this round

## Assertion Matrix

| Flow | 输入组合（关键参数） | 预期 backend 调用（接口/次数/字段） | 预期 DB 变化（表/字段） | 失败不变性（必须保持不变字段） |
|---|---|---|---|---|
| 归集 BuildTx 重建刷新地址 | `trade_no` 已进入重建，最新策略地址与旧 `to_addr` 不同 | 无需真实 backend；构建前读取到最新地址 | `api_collect.to_addr` 更新为最新地址，并作为后续 build 使用 | 未进入重建时不得无故篡改 `tx_hash` / `raw_tx` |
| 归集 blockhash 恢复重建 | `sol` 已持有旧 `raw_tx/tx_hash/to_addr`，随后触发 rebuild | 无需真实 backend；先作废旧构建事实，再进入下一轮 build | 先清空 `api_collect.raw_tx/tx_hash`，后续重建再把 `api_collect.to_addr` 更新为最新地址 | 作废旧构建事实时不得伪造新地址，也不得继续保留旧 `tx_hash/raw_tx` 参与上报 |
| 归集执行回执上报 | `api_collect.to_addr` 已持久化为当前执行地址，`tx_hash` 非空 | `upload_tx_exec_receipt.to` 必须等于持久化 `to_addr` | 无额外 DB 变更，仅消费现有事实 | 不允许回退到原请求地址或重新查询策略地址 |

## Progress Checklist

- [x] Update plan for this batch
- [x] Implement collect address refresh + report consistency fix
- [x] Add focused regression tests
- [x] Run focused validation

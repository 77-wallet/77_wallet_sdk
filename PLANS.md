# PLANS

Current task execution plan.
Refs: `docs/codex/testing.md`, `docs/codex/workflows.md`.

## Task

- Name: wallet-ecdh phase-1 hardening
- Goal:
  - 在不改协议语义/对外接口的前提下，先完成低风险稳定性与安全性收敛
  - 去掉敏感日志与 panic 路径，补齐关键失败路径测试

## Scope

### In

- `wallet-ecdh/src/lib.rs`
- `wallet-ecdh/src/encryption.rs`
- `wallet-ecdh/src/data.rs`
- `PLANS.md`

### Out

- 后端协议字段变化
- 跨 crate 重构
- 非必要 API 语义调整

## Constraints

- No new business logic
- Keep public API behavior compatible
- Offline tests only
- No real network dependency

## Plan

1. Remove sensitive secret logging and replace panic-prone lock access with non-panicking handling
2. Keep current crypto wire-compat behavior; add explicit compatibility comments where needed
3. Add/strengthen failure-path tests (missing shared secret, invalid AAD, invalid payload)
4. Run targeted formatting and `wallet-ecdh` tests

## Validation Commands

- `cargo fmt --all`
- `cargo test -p wallet-ecdh`

## Expected Results

- 无 shared secret 明文日志
- 不再因 `RwLock` poison 在关键路径 panic
- 失败路径测试可复现并稳定通过

## Progress Checklist

- [x] Update code with minimal hardening changes
- [x] Add failure-path tests
- [x] Run validation commands
- [x] Delivery notes

## Delivery Notes

- Changed files:
  - `wallet-ecdh/src/lib.rs`
  - `wallet-ecdh/src/encryption.rs`
  - `wallet-ecdh/src/data.rs`
  - `PLANS.md`
- Validation:
  - `cargo fmt --all` (passed)
  - `cargo test -p wallet-ecdh` (passed: 13/13)
- Notes:
  - 保持了当前链路的 nonce 派生兼容行为，仅补注释说明
  - 删除 shared secret 明文日志
  - 为 `set_sn/sn` 去除 `unwrap` panic 路径

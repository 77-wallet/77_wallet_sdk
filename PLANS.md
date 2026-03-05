# PLANS

Current task execution plan.  
Refs: `docs/codex/testing.md`, `docs/codex/workflows.md`.

## Task

- Name: wallet-transport-backend low-risk cleanup
- Goal: 修复已识别的低风险质量问题（panic 风险、GET 参数编码、无用变量）
- Deliverables:
  - 去除 `unwrap` panic 点
  - 修复 GET query 编码
  - 清理无用局部变量

## Scope

### In

- `wallet-transport-backend/src/api_request.rs`
- `wallet-transport-backend/src/http.rs`
- `PLANS.md`

### Out

- 业务接口语义变更
- 跨 crate 改动
- 大规模重构

## Constraints

- No new business semantics
- No large refactor
- Offline-test requirement

## Plan

1. Analysis
2. Minimal implementation
3. Validation
4. Delivery notes

## Validation Commands

- `cargo check -p wallet-transport-backend`

## Expected Results

- 无 `unwrap` panic 路径
- GET 请求参数走标准 query 编码
- 本轮修改文件编译通过

## Progress Checklist

- [x] Analysis
- [x] Minimal implementation
- [x] Validation
- [x] Delivery notes

## Delivery Notes

- Changed files:
- `wallet-transport-backend/src/api_request.rs`
- `wallet-transport-backend/src/http.rs`
- `PLANS.md`
- Key decisions:
- `ApiBackendRequest::new` 移除 `unwrap` 调试序列化，避免潜在 panic
- GET 参数改为 `reqwest` 标准 `query` 编码，避免手工拼接风险
- 本轮仅做低风险最小改动，不触达未使用类型/模块的大范围清理
- Risks / follow-ups:
- `wallet-transport-backend` 仍有 dead_code 警告（`ChainRpc`、`send_request`、`etherscan.rs`），建议下一轮单独清理

# PLANS

Current task execution plan.  
Refs: `docs/codex/testing.md`, `docs/codex/workflows.md`.

## Task

- Name: ecdh-backend integration documentation
- Goal:
  - 解释 `wallet-ecdh` 在工程中的实际职责和与 backend 的集成方式
  - 产出可维护文档（含流程图、状态模型、异常与验证指引）

## Scope

### In

- `docs/ecdh-backend-flow.md`
- `wallet-ecdh/README.md`
- `PLANS.md`

### Out

- 任何加密行为、对外 API、协议字段变更
- 跨 crate 业务代码重构

## Constraints

- No new business semantics
- Keep public API unchanged
- Offline-test requirement
- No real network dependency

## Plan

1. Create `docs/ecdh-backend-flow.md` with fixed sections and 3 Mermaid diagrams
2. Document real call points in `wallet-api` and `wallet-transport-backend` (no pseudo flow)
3. Add short link in `wallet-ecdh/README.md`
4. Run validation commands and record results

## Validation Commands

- `cargo test -p wallet-ecdh`
- `cargo test -p wallet-transport-backend --lib`

## Expected Results

- 文档完整覆盖握手、请求出站、响应入站三段流程
- 流程图与真实代码调用点一致
- 两条验证命令通过

## Progress Checklist

- [x] Write documentation content and diagrams
- [x] Add README pointer
- [x] Run validation commands
- [x] Delivery notes

## Delivery Notes

- Changed files:
  - `docs/ecdh-backend-flow.md`
  - `wallet-ecdh/README.md`
  - `PLANS.md`
- Validation:
  - `cargo test -p wallet-ecdh` (passed: 9/9)
  - `cargo test -p wallet-transport-backend --lib` (passed: 7/7)
- Key decisions:
  - documentation is implementation-oriented and references real call sites
  - kept existing API and crypto behavior unchanged, only added docs and references
  - included 3 Mermaid diagrams for handshake, crypto pipeline, and runtime state

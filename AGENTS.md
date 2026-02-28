# AGENTS.md

## 中文版

### 项目结构与模块边界
- 工作区根目录由以下核心 crate 组成：
  - `wallet-api`：业务聚合层（WalletManager/API/service/domain），负责钱包功能编排。
  - `wallet-transport-backend`：后端 HTTP 接口封装与请求/响应模型。
  - `wallet-database`：SQLite 数据层（entities/repositories/migrations）。
  - `wallet-oss`：对象存储相关能力。
  - `wallet-tree`：密钥树、助记词与地址派生能力。
  - `wallet-ecdh`：密钥交换与安全通信相关能力。
- 文档位于 `docs/`，测试主要位于各 crate 的 `tests/`。
- 仅将构建产物写入 `target/`，不要把运行临时文件提交到仓库。

### 常用开发命令
- `cargo check`：首选的快速静态检查。
- `cargo build`：构建整个 workspace。
- `cargo test -p wallet-api`：运行 wallet-api 测试。
- `cargo test -p wallet-transport-backend`：运行后端传输层测试。
- `cargo test -p wallet-database`：运行数据库层测试。
- `cargo fmt --all`：统一格式。
- `cargo clippy --all-targets --all-features`：lint 检查。

### 代码风格与命名
- 使用 Rust 2024 默认风格：4 空格缩进，`snake_case` 函数/模块，`PascalCase` 类型，`SCREAMING_SNAKE_CASE` 常量。
- 不提交未格式化代码；提交前至少运行 `cargo fmt --all`。
- 优先修复警告而非屏蔽：不要随意添加 `#[allow(...)]`。
- 注释写“为什么”，避免重复“代码做了什么”。

### 测试与变更要求
- 修改 `wallet-api` 业务流程（多签/质押/交易/归集/提币）时，补充或更新对应集成测试。
- 修改 `wallet-database/schema/**/migrations` 时，必须同步更新实体与仓储逻辑，并验证迁移可执行。
- 交易、签名、状态推进相关逻辑变更，需覆盖：
  - 成功路径
  - 重试或恢复路径
  - 失败路径（尤其是链上未确认/超时/广播不确定）

### 提交与 PR 建议
- 建议采用约定式提交：`feat(scope): ...`、`fix(scope): ...`、`refactor(scope): ...`。
- PR 说明至少包含：
  - 变更摘要
  - 影响模块
  - 测试证据（命令与结果）
  - 是否涉及数据库迁移/兼容性

### 安全与配置
- 禁止提交真实私钥、助记词、设备凭据、生产环境配置。
- `wallet-api` 中涉及密码、私钥、签名的逻辑必须保持最小暴露面，不打印敏感数据。
- 若新增后端接口，优先在 `wallet-transport-backend` 中封装，避免在业务层散落 HTTP 细节。

---

## English Version

### Project Structure and Module Boundaries
- The workspace is composed of these core crates:
  - `wallet-api`: business orchestration layer (WalletManager/API/service/domain).
  - `wallet-transport-backend`: backend HTTP client wrappers and DTOs.
  - `wallet-database`: SQLite data layer (entities/repositories/migrations).
  - `wallet-oss`: object storage capabilities.
  - `wallet-tree`: key tree, mnemonic, and derivation logic.
  - `wallet-ecdh`: key exchange and secure communication utilities.
- Documentation lives in `docs/`; tests are mainly under each crate's `tests/`.
- Keep generated artifacts in `target/` only.

### Common Development Commands
- `cargo check`: preferred fast feedback loop.
- `cargo build`: build the entire workspace.
- `cargo test -p wallet-api`: run wallet-api tests.
- `cargo test -p wallet-transport-backend`: run transport backend tests.
- `cargo test -p wallet-database`: run database tests.
- `cargo fmt --all`: format code.
- `cargo clippy --all-targets --all-features`: lint checks.

### Coding Style and Naming
- Follow Rust 2024 defaults: 4-space indentation, `snake_case` for functions/modules, `PascalCase` for types, and `SCREAMING_SNAKE_CASE` for constants.
- Do not commit unformatted code; run `cargo fmt --all` before commit.
- Prefer fixing warnings instead of suppressing them; avoid casual `#[allow(...)]`.
- Write comments for intent/reasoning, not obvious behavior.

### Testing and Change Requirements
- When changing wallet-api business flows (multisig/stake/transaction/collect/withdraw), update or add integration tests.
- For changes in `wallet-database/schema/**/migrations`, update entities/repos accordingly and verify migrations.
- For transaction/signature/state-transition changes, cover:
  - happy path
  - retry/recovery path
  - failure path (especially uncertain broadcast/timeout/on-chain confirmation lag)

### Commit and PR Guidance
- Recommended conventional commits: `feat(scope): ...`, `fix(scope): ...`, `refactor(scope): ...`.
- PRs should include:
  - change summary
  - affected modules
  - test evidence (commands + outcomes)
  - migration/compatibility impact

### Security and Configuration
- Never commit real private keys, mnemonics, device credentials, or production secrets.
- Keep password/private-key/signing logic minimally exposed; do not log sensitive values.
- For new backend endpoints, encapsulate them in `wallet-transport-backend` instead of scattering HTTP details in business code.

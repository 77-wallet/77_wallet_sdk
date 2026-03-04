# AGENTS.md (root)

## 项目边界

- workspace crates：wallet-api / wallet-transport-backend / wallet-database / wallet-oss / wallet-tree / wallet-ecdh
- 文档在 docs/；测试在各 crate/tests/
- 仅写构建产物到 target/；禁止提交运行临时文件

## 安全红线

- 禁止提交或打印私钥、助记词、凭据、生产配置等敏感信息

## 质量门槛（最小化）

- 优先修复告警，避免随意 #[allow(...)]
- 最小验证顺序：cargo fmt --all -> cargo check -> 受影响 crate tests
- 修复缺陷必须补 1 个回归用例
- DB 迁移/事务/交易状态相关改动必须补测试并通过

## Code Quality

- 所有 pub API 必须写 Rust doc comments（说明用途、参数、错误语义/注意事项）。
- 复杂逻辑必须补“解释性注释”（why/约束/边界/隐含假设），避免只描述代码做了什么。
- 测试必须写明测试场景（// Scenario: ...），并在文件头说明为何需要串行/隔离策略。

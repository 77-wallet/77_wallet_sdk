# Architecture

## Scope

本文件定义仓库级实现边界与依赖方向，用于约束改动范围与测试注入点。

## Layering

- API / Manager: 对外入口与编排入口
- Service: 业务编排与事务边界
- Domain: 领域规则与流程片段
- Repo / DB: 持久化访问
- Transport / External: backend、MQTT、外部系统

## Dependency Direction

- 允许：API -> Service -> Domain -> Repo/Transport
- 避免：低层反向依赖高层
- 变更优先级：先做注入点与测试护栏，再考虑重构

## Testability Seams

- Backend seam: trait + fake backend
- Storage seam: temp sqlite / test repo setup
- Async side effects seam: task noop / worker off
- Global state seam: serial + singleton fixture

## Change Policy

- 功能改动优先最小影响路径
- 非必要不改公共接口与跨模块契约
- 任何流程改动必须补测试与断言矩阵

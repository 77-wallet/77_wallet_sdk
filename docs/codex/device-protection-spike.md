# 设备保护接口：第一轮可行性验证

日期：2026-09-12。状态：仅验证接口形态，未接入生产 SDK 或 SEP/TEE。

## 已验证

临时程序位于 `target/device-protection-spike/`，不会作为生产代码提交。采用 `Arc<dyn DeviceProtection>` 注入异步 `protect(context, bytes)` / `unprotect(context, blob)`；示例存储层负责文件读写。输入仅为合成的 keystore 字节，不含真实钱包数据。

软件 AES-GCM 适配器仅用于验证契约，不具备硬件不可导出性，不应作为生产降级方案。

- 实际文件写入外层密文；重新构造存储对象并使用同一适配器可读回原字节。此项不是进程重启后的硬件密钥持久化测试。
- 同一输入重复加密使用新 nonce，密文不同。
- 不同适配器密钥、不同用途标识、密文篡改均被认证拒绝。
- 适配器失败或超时不覆盖已有文件；读取失败不会退回原始字节。
- 旧格式被拒绝，正式接入需要显式升级流程。

## 验证命令与结果

```sh
CARGO_TARGET_DIR=target/device-protection-spike/build cargo test --offline --manifest-path target/device-protection-spike/Cargo.toml
cargo fmt --manifest-path target/device-protection-spike/Cargo.toml -- --check
```

4 passed, 0 failed。无网络、无真实钱包访问。临时程序在 target 清理后消失。

## 正式接入的位置

- SDK：Context 中注入保护接口；上层存储流程等待返回后再写入文件或 SQLite BLOB。
- 插件：平台适配器实现设备密钥操作，初始化时提供给 SDK。SDK 不导入 Flutter、Swift 或 Android 类型。
- Flutter 页面：继续调用原有钱包业务入口，处理需要展示的错误。
- 普通钱包 `wallet-tree::IoStrategy` 与 `KeystoreApi` 当前是同步接口，且包含直接文件操作。需要按单一流程拆分序列化/加密与 I/O，明确异步边界，不能直接在运行时线程里 block_on 原生回调。
- API 钱包的 phrase/seed 编码与数据库操作已在异步流程内，更适合下一批先验证一条字节保护链路。

## 后续批次

1. 原生适配器：选一个平台，用合成数据验证真实硬件密钥、重启加载、锁屏/缺失/错误传播。平台桥接尚未证明可用。
2. 一条 SDK 实际存储流程：覆盖写入、读取、失败不落旧格式、并发和原子替换。
3. 旧数据升级：新密文读回校验后再切换，崩溃恢复与回滚策略；不得自动覆盖失效密钥。
4. 覆盖另一平台及其余普通/API 钱包数据、password_proof 和备份副本。

尚未完成：真实 SEP/TEE、FFI 跨语言调用、进程重启、SQLite、迁移、原子替换、取消后的原生操作生命周期、设备恢复。四项软件测试不能作为审计 H2 关闭依据。


## 第二批：真实 Secure Enclave 验证

本机 Apple Silicon macOS 上以 Swift/CryptoKit 完成真实 SEP 验证。临时源码、可执行文件及合成数据均在 `target/device-protection-sep/`。没有读取实际钱包，也没有创建生产 Keychain 条目。

受限执行环境返回 SecureEnclave unavailable；经工具授权在沙箱外重跑后，SecureEnclave.isAvailable 为 true，以下验证通过：

1. SEP P-256 KeyAgreement 密钥参与 ECDH，HKDF-SHA256 派生 AES-256-GCM 密钥，合成数据往返成功。
2. 将 CryptoKit dataRepresentation（设备绑定的包裹表示，不是明文私钥）和密文保存到 target，退出后由另一个进程恢复并解密成功。
3. 变更用途 AAD、篡改密文、使用另一把真实 SEP 密钥均被拒绝。

```sh
xcrun swiftc -module-cache-path target/device-protection-sep/module-cache target/device-protection-sep/probe.swift -o target/device-protection-sep/probe
target/device-protection-sep/probe target/device-protection-sep write
target/device-protection-sep/probe target/device-protection-sep read
```

后两条需要本环境的沙箱外权限才能访问 SEP。测试未使用软件密钥替代 SEP 长期密钥；临时 ECDH 私钥、共享秘密、派生 AES 密钥和解密结果仍会存在应用内存。

边界：这是 macOS 硬件验证，不是 iPhone 实测；没有证明文件拷到另一台物理设备的行为，也没有验证 Keychain 保存、WhenUnlockedThisDeviceOnly ACL、生物认证、重启设备、换机恢复或插件 FFI。测试重启的是进程，不是操作系统。下一批需在插件接入单一 iOS 设备保护适配器，并验证实际 SDK 异步调用及 iPhone 行为。


## 正式实现第一批：SDK 接口

新增 `wallet_api::device_protection`：实例级 `Arc<dyn DeviceProtectionProvider>`、异步字节加解密、稳定记录身份/用途的 AAD 编码，以及不携带原始平台错误的类型化错误。默认无 provider 时拒绝操作，不返回输入作为降级。

5 项契约测试通过，包含 Pending 到 Ready 的延迟响应；代码审查无阻塞问题。断言矩阵见 `assertion-matrices/device-protection.md`。此批不包含 Context 初始化注入、插件注册或钱包读写接入；这些都仍是后续工作。

`cargo check -p wallet-api --offline` 通过（1m36s）；存在仓库原有警告，未扩大范围修复。

## Android 模拟器运行验证

2026-09-12：构建独立包 com.sevenwallet.protectionprobe（源码和 APK 在 target/android-device-protection/apk），在 emulator-5554 安装运行。未读取钱包数据或修改锁屏设置。测试结束已卸载独立测试 App。

- Android 运行时的合成数据 AES-GCM 往返、随机 IV、AAD、篡改、错密钥、格式和缺失密钥断言通过。
- 设备状态 secureLock=false、locked=false。
- 独立 AndroidKeyStore 能力探测 key 的 isInsideSecureHardware=false；探测 key 已删除。
- 正式适配器 provision 返回 UNAVAILABLE，未降级。由于首先被无安全锁屏检查拒绝，本次不构成其软件密钥等级拒绝分支的动态覆盖。
- 无连接的 Android 真机，因此 TEE/StrongBox 成功路径、锁屏变化、重启恢复仍待真机测试。未接通 Flutter/FRB。

## Android 真机 TEE 验证

2026-09-12，ANA-AN00，API 31：独立测试包运行通过。设备安全锁屏已开启且当前解锁；实际适配器报告 TEE（非 StrongBox）。真实 AndroidKeyStore AES-GCM 往返、持久化合成密文、显式 force-stop 后新进程解密，以及真实密钥下篡改/错误 AAD 拒绝全部通过。进程 PID 已变化，未重启手机。测试结束卸载独立测试包并清理测试密钥及合成数据，未接触钱包。

仍未验证：StrongBox、锁屏状态切换/移除锁屏、系统重启、另一台设备解密拒绝、Flutter/FRB 及实际 SDK 存储集成。本次证据证明该真机上原生 TEE 适配工作的成功路径，不代表完整钱包已启用设备保护。

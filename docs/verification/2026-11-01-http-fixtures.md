# Windows 并行 HTTP 回归修复

修改前 `86839f3`，工作区干净。沿用本地引擎后续开发授权；本阶段只修测试服务，不改生产网络行为。

## 原因与证据

旧服务使用非阻塞 listener 轮询 accept，但没有显式设置接收的 TcpStream 模式。Windows 上接收的连接继承非阻塞模式。临时诊断观察到 accept 后立即 read 返回 WouldBlock / OS 10035、请求字节数为 0；旧夹具把它当 EOF，关闭了仍在发送请求的连接。客户端表现为 IncompleteMessage、10053 中断、未观察到 GET 或后续连接被拒绝。客户端构造仅微秒到毫秒级，原先增加等待期限、排查沙箱或客户端初始化均未解决根因。

新增原创用例：建立 TCP 连接，等待 100 ms 再发送完整 HTTP 头。修复前确定性失败（10035 后连接中断），修复后正常返回完整响应。该用例不请求商业平台。

## 修复与验证

- `web.rs` 与 `fanqie_probe.rs` 的测试服务器 accept 后调用 `set_nonblocking(false)`，随后使用既有读取超时。
- 请求完整性、响应大小、重试次数、限流、验证、取消断言保留。没有扩大生产超时、重试或访问权限。
- 临时诊断日志从源码移除，日志只留在忽略目录。
- 新用例红绿验证通过；core lib 默认并行 30 项通过。
- `cargo test --workspace` 默认并行：139 通过，0 失败，4 个既有显式忽略项；不再依赖单线程绕开该问题。
- Rustfmt、全 targets Clippy（warnings 为错误）通过。
- 本阶段没有修改前端或打包，因此不重复前端 lint/typecheck/build，也不将旧包视为新增交付。

诊断证据 `.artifacts/http-diagnostic-probe-20261101.log`，最终全量结果 `.artifacts/http-fixed-workspace-20261101.log`。此前桌面接入报告中的并行失败是历史状态；本记录给出后续定位与修复，不覆盖真实全书、账户或原生窗口仍未验收的边界。

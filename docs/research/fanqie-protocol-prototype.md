# 番茄官方单章协议探测原型

日期：2026-10-31。修改前基线：f3c00b2。目标是先证明一个正常官方渠道能取得完整章节，再考虑独立实现、全书队列和桌面入口。本轮没有达到完整正文获取目标。

## 实际执行结果

针对用户此前授权的测试样本，作品 ID `7672011528131857470`，章节 ID `7672702368512754238`，分别显式调用一次两种模式，没有继续整书请求：

| 模式 | HTTP | 原型判断 | 本轮结论 |
| --- | --- | --- | --- |
| `app-content` | 200 | `empty_response`，0 字节 | 普通最小参数请求没有取得正文；不能据此断言已知道全部协议要求或该渠道永久不可用 |
| `web-api` | 200 | `access_challenge`，响应含验证标记 | 未读取验证响应正文，不重试、不自动验证；报告的 0 字节指本工具未读正文，不代表服务器响应长度为零 |

两次命令均退出 3，表示未通过可读候选校验。没有候选正文、逐字核对、全文导出或全书成功证据，也没有更新实际书库。HTTP 200 不作为成功依据。

原始报告在忽略目录 `.artifacts/fanqie-prototype-20261031/`：`rust-app-content-report.json`、`rust-web-api-report.json` 和 `rust-exit-codes.json`。这些报告不含小说正文、标题、作品/章节 ID、任意响应头或服务端消息。

## 独立实现范围

新增核心研究模块 `crates/core/src/sources/fanqie_probe.rs` 与独立 example `crates/core/examples/fanqie_protocol_probe.rs`。它没有注册为 SourceHub 来源，没有桌面 IPC、下载队列或数据库入口，不扩大平台能力清单。

- 输入只接受两种固定模式及 10～24 位数字 ID，不接受任意 URL、请求头、Cookie 或密钥。
- 固定 HTTPS 主机与路径，使用项目自身标识的 User-Agent。保持 TLS 校验，禁止重定向，显式禁止 reqwest 内部重试；应用层也无重试、身份轮换或自动回退。
- 连接超时 8 秒，请求超时 15 秒，解压后读取上限 2 MiB 加一个判溢出字节。未改变系统或环境代理设置。
- 不读取浏览器凭据，不注册设备，不调用外部解析服务，不复制签名、设备池或解密实现。正文仅在内存中检查，不保存到磁盘或书库。
- 区分访问挑战、限流、错误、重定向、空响应、超限、加密标记、未知结构、身份不符、预览/付费标记、受保护字形及字数异常。
- 仅识别已经核对的网页 JSON 结构；未知 App 数据结构保持 `unknown_schema`，没有猜测字段后算成功。
- `readable_candidate` 需要作品/章节身份一致、明确开放标记、普通段落、声明字数与实际字数比例合理、既有导入文档校验通过。它仍然不是完整正文验收，需要进一步核对目录标题及实际原文，字数比例不能证明没有截断。

### 运行方式

在项目根目录构建；联网探测必须由操作者显式执行，不加入普通测试或 CI：

```powershell
cargo build -p opennovel-core --example fanqie_protocol_probe --locked
.\target\debug\examples\fanqie_protocol_probe.exe web-api <作品ID> <章节ID>
.\target\debug\examples\fanqie_protocol_probe.exe app-content <作品ID> <章节ID>
```

退出码：0 为可读候选，3 为有报告但未通过，1 为输入/连接/读取错误，2 为参数数量错误。不要把循环调用此 example 当作全书下载器。

## 协议证据及边界

### 官网实际网页客户端

本轮读取官方阅读页及其实际引用的两个应用脚本，没有执行脚本，也没有请求验证码 SDK：

- [官方阅读页](https://fanqienovel.com/reader/7672702368512754238)
- [Reader.63252b77.js](https://lf-fe.fqnovelstatic.com/obj/novel-fanqie-fe/toutiao/muye/Reader.63252b77.js)
- [muye_dab5cb12.js](https://lf-fe.fqnovelstatic.com/obj/novel-fanqie-fe/toutiao/muye/js/muye_dab5cb12.js)

可以确认网页使用 `/api/reader/full`、`itemId` 参数、`ismobile` 请求头，并从 `data.chapterData` 读取正文数据。这不能证明网页对后续 App 章节开放，也不能证明 App 使用同一响应结构。

### 新发现的公开本地客户端线索

发现 [ch6vip/fqapp](https://github.com/ch6vip/fqapp/tree/07cfb2249d4fd57d19bf4420af0e97621ec92df9) 的公开实现，研究固定在提交 `07cfb2249d4fd57d19bf4420af0e97621ec92df9`。这更新了上一份研究中尚未找到可审查本地协议实现的状态，但不代表已审查安全或已经可以集成。

- [开发说明](https://github.com/ch6vip/fqapp/blob/07cfb2249d4fd57d19bf4420af0e97621ec92df9/DEVELOPMENT.md)描述进程内 Rust 核心及回环 HTTP 调用入口。本地运行不等于没有外部请求，仍需审查完整出站数据流。
- [DH 内容端点](https://github.com/ch6vip/fqapp/blob/07cfb2249d4fd57d19bf4420af0e97621ec92df9/rust/src/endpoints/dhcontent.rs)与[上游客户端](https://github.com/ch6vip/fqapp/blob/07cfb2249d4fd57d19bf4420af0e97621ec92df9/rust/src/upstream/client.rs)提供 `/api/novel/book/reader/content/v1` 的请求线索；其实现还涉及签名、握手及其他路径的设备状态，不能只抄一个地址便宣称复现正常客户端。
- 本轮 `app-content` 使用少量通用参数做普通 GET；没有实现上述签名/握手，也没有观察到成功的正常 App 请求，所以这次失败不足以定位具体缺失字段。
- 该仓库为 [GPL-3.0](https://github.com/ch6vip/fqapp/blob/07cfb2249d4fd57d19bf4420af0e97621ec92df9/LICENSE)。OpenNovel 使用 Apache-2.0，且用户要求不复制代码；本轮仅研究协议角色，没有复制、链接、运行或分发其实现，没有读取其设备池或密钥文件。

## 验证与下一阶段门禁

4 项新增原创单元测试覆盖候选及报告脱敏、错误/限制分类、身份/权限/字形/字数拒绝、固定地址与输入拒绝。所有内容为项目原创；普通测试不请求商业平台。

本轮 `cargo fmt --all -- --check`、workspace 全 targets Clippy（warnings 为错误）及 example 构建通过；`cargo test --workspace --locked` 为 124 项通过、0 失败、3 项显式性能测试忽略。日志保存在上述忽略目录的 `workspace-tests.log` 和 `clippy.log`。解析单测和构建不等于联网全文成功，也不证明原生界面行为。本轮没有修改界面或桌面入口，因此未重复前端 lint/typecheck/build 或桌面打包，没有使用 Computer Use。

后续工作的具体缺口是：建立可审查、可复现的正常官方客户端请求契约与成功章节样本，弄清签名/会话及内容编码分别承担的角色，再独立实现。不能把传输加密本身等同于付费权限，也不能把客户端能展示等同于所有章节都允许免费获取。任何验证、付费或阅读权益限制继续保留。

准入全书队列前仍需：至少一个原先网页不完整的免费章节取得完整正文、目录/身份/原文对照、预览与未授权内容拒绝、稳定会话和限流/取消/恢复验证。未达到这些门禁，不新增“番茄 App 全文已支持”的声明，不将本次研究版替换桌面成品。

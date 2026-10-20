# v0.2 多来源实现与验证

日期：2026-10-21。环境：当前 Windows 11 x64、Rust 1.96、Node 24 / npm 11、WebView2 已安装。工作起点 main `ad046c7` 干净；先提交实施方案 `73d64e2`，再实现业务代码。本文记录本机候选版证据，不代表远端 CI 或干净系统验收。

## 实现结果

- 七猫：详情、完整目录、明确免订阅的官网正文、选章、暂停/继续、持久恢复、离线预览与共用导出。
- 书旗：详情与完整目录。正文两次 429，第二次 Retry-After=62 秒；无正文适配器，GUI 禁用下载、CLI/core 建任务前拒绝。
- 来源识别与能力注册共用核心，恢复按持久任务来源路由；跨来源相同 ID 隔离。番茄和本地 JSON 保留。
- 无新增依赖，无 schema 迁移，当前仍为 schema 4。来源研究和决策见 [研究记录](../research/qimao-shuqi-web.md)、[ADR 0005](../adr/0005-source-registry-and-capabilities.md)。

## 自动化与构建

| 检查 | 结果 |
| --- | --- |
| cargo fmt --all -- --check | 通过 |
| cargo clippy --workspace --all-targets -- -D warnings | 通过，无警告 |
| cargo test --workspace | 43 通过：CLI 1、来源单元 8、下载 13、书库 10、多来源 6、v0.1 回归 5 |
| npm run lint | 通过 |
| npm run typecheck | 通过 |
| npm run build | 通过，JS 89.52 kB、CSS 10.87 kB，均为未压缩构建大小 |
| npm run release:notices | 通过，352 项锁定依赖元数据，未增加依赖 |
| npm run desktop:package | 通过，Rust release + 前端 + Windows x64 NSIS |

本次多来源原创样例覆盖：官方链接/前缀和主机路径限制、非默认端口/身份信息拒绝、目录数量与顺序、明确权限和未知权限、主章节与推荐正文分离、错章/截断/字形拒绝、书旗只读目录、跨平台相同 ID 分离、错误适配器拒绝、取消后重开继续、尾部增章及旧导出保留。七猫 HTTP 折行头兼容另有本地受控服务器回归，不改变其他客户端严格模式或 URL 白名单。普通测试不请求商业平台。

## CLI 真实来源

均为官方公开网页、无账户。CLI 临时独立书库与日志位于忽略目录 `.artifacts/v02-live`。

| 场景 | 观察结果 |
| --- | --- |
| sources | 番茄/七猫 can_download=true；书旗 false |
| 七猫 inspect 222767 | 完整目录 4100，is_vip=0 为 14、受限 4086 |
| 七猫选择 1,3,15 | 保存 2、受限 1、失败 0；任务 partial，退出码 2 |
| 七猫选择 1,3,5 | 复用 1/3，获取 5；completed 3/3，退出码 0；全书仍只有 3/4100 |
| 书旗 inspect 9147568 | 作品名/作者/简介与完整目录 11 章 |
| 书旗 download | 错误退出码 1，创建任务前拒绝 |
| 番茄纯数字 7400799554502675518 | 旧输入语义保持，首章 completed 1/1 |

七猫部分任务 `89f7c6a5-64bb-4eae-bd3d-cce9fb031e88`，书籍 `032f8321-32cf-48cd-bc59-6b8e23984425`。保存第 1/3/5 章不表示其他 11 个明确免订阅章节都已经实取验证，也不表示全站或 App 可用。

两个 CLI EPUB（partial-2-of-3、complete-3）均通过 EPUBCheck 5.4.0，0 fatal/error/warning。命令使用 Java 21 与 `.tools/epubcheck-maven/lib/*`；Java 仅为开发校验工具，不是应用运行依赖。

## 已安装原生应用

使用本轮 NSIS 静默安装到 `.artifacts/install-v02`，安装进程退出 0；通过该安装目录中的实际 EXE 启动，通过 WebView2 CDP 连接原生窗口。未使用浏览器预览代替 IPC 证据。

1. 来源页显示真实能力。书旗官网链接得到 11 章目录，正文未验证提示可见，开始下载按钮禁用；截图 `output/playwright/v02-shuqi-directory.png` 已检查。
2. 七猫 `qimao:222767` 读取 4100 项，选择 1/3/5/15。保存 2 章后暂停，持久状态为 paused 2/4；截图 `output/playwright/v02-qimao-paused.png`。
3. 关闭该原生进程并从安装路径重启。原任务 `632e3b04-07e3-4998-8f12-9b5836241bd7` 继续，以七猫适配器补齐第 5 章，最终 partial：保存 3/4、受限 1、失败 0。
4. 任务范围 EPUB 导出成功；ZIP 内 OPF spine 与章节 h1 对应 1/3/5，不含 15；EPUBCheck 5.4.0 为 0 fatal/error/warning。
5. 本地详情显示全书 3/4100，目录每页 100 项，首章离线预览显示 20 段以上正文；截图 `output/playwright/v02-qimao-restored-preview.png` 已检查。控制台无错误和警告，窗口布局无可见溢出。

### 用户书库和卸载保留

原生测试前用 SQLite backup API 备份已有默认书库至忽略目录 `.artifacts/v02-backups/pre-native-library.db`，记录每本已存正文哈希。原有 7 本书、22018 章，schema 4；测试后 8 本、22021 章，原有 7 本的正文哈希均不变，integrity_check=ok、foreign_key_check 无记录。新增的一本为本次七猫所选三章。

停止本轮拥有的进程，只对本轮安装目录执行默认卸载，退出 0；检查应用 EXE 和卸载注册项已移除。书库仍为 8 本/22021 章，导出记录对应文件仍存在。没有清理用户书库，也没有回滚用户数据；备份未放入交付安装包目录。

机器可读结果位于忽略目录 `.artifacts/v02-release/native-result.json`、`paused-native.json`，原书籍哈希单独保留；这些包含本机数据路径的证据不提交 Git。

## 交付文件

| 文件 | 字节数 / SHA-256 |
| --- | --- |
| `.artifacts/v02-release/OpenNovel-Downloader-0.2.0-windows-x64-setup.exe` | 4145069；`f3e9c18e00d22c885ddda6301cdcdb59911d9bde7d5dbcdbdcca2c49c2c0fdd1` |
| `target/release/opennovel-desktop.exe` | 14917632；`be9d35a01c587dda660e2337a0692c7d9e069cca2a09ce87d2207660b253ba4d` |

交付目录另含 THIRD_PARTY_NOTICES.txt、SHA256SUMS.txt 和绑定源码 commit 的 manifest.json。安装器未签名，不提交 Git。原 v0.1 安装包 `.artifacts/release/OpenNovel-Downloader-0.1.0-windows-x64-setup.exe` 保留且 SHA-256 未变：`e8d1f2e5057221b7beb8dd81259129763f8373d31162f31b3d67ccee964645a1`。

## 未完成的验证与路线

- **AC-20 继续未通过**：独立干净 Windows 与缺失 WebView2 时的安装分支尚无证据；本机安装成功不能替代。其他操作系统未验收，远端 CI 未运行。
- 七猫只验证所列作品和选章；HTML/API 变化、更多类型作品、真实远端增章和完整性抽验待扩大。目录与详情不同步会安全拒绝，长度启发式存在适用边界。
- 书旗正文仍未验证，应先在正常访问条件下重新观察正文、身份、完整性和限流，未通过前保持目录能力；不将 429 视为永久不可用或已修复。
- 起点、晋江尚未接入。后续先公开元数据/目录和免费试读，再评估账户与已购内容；未购买 VIP 获取不在本轮能力中。
- App 专属内容、账户、搜索、动态插件、签名、自动更新和移动端留后续，不作为当前交付承诺。

本轮功能与本机验证已形成闭环，版本仍标记候选版；不自动打 tag、push 或公开发布。

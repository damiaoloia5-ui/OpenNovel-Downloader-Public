# 全量工程检验

日期：2026-10-31。基线：`c93793e`（main）。开始前 `git status --short --branch` 为干净；HEAD 已提供回退点，无需备份提交。图工具返回项目未索引后读取源码、CI、测试及配置。全程未使用 Computer Use、真实账户、商业来源正文或用户书库。日志与原创受控产物保留在 `.artifacts/full-check-20261031`，不提交 Git。

## 检查范围与结果

| 检查 | 本轮结果 |
| --- | --- |
| `cargo fmt --all -- --check` | 通过 |
| `cargo clippy --workspace --all-targets --locked -- -D warnings` | 通过 |
| `cargo test --workspace --locked` | 101 通过，0 失败，3 项性能基准默认忽略 |
| `cargo test --workspace --locked -- --ignored --nocapture --test-threads=1` | 上述 3 项基准全部通过，无剩余忽略项 |
| `npm run lint` / `npm run typecheck` | 通过，无 lint 警告 |
| `npm run test:frontend` | 40 通过，1 项可选旧新脚本对照跳过 |
| 设置既有 `OPENNOVEL_CAPTURE_BASELINE` 后重跑前端测试 | 41 通过，0 失败，0 跳过 |
| `npm run build` | 通过；60 模块，JS 187.36kB / gzip 62.52kB |
| `npm run signing:test` | 10 项检查通过；未使用项目私钥、未实际签名 |
| 原创 CLI 端到端检查 | 2 章导入、逐章精确正文比较、TXT 顺序、EPUB 内容、备份及新目录恢复通过 |
| 原创测试库 SQLite | `integrity_check=ok`，无外键错误 |
| EPUBCheck 5.4.0 | 原创 CLI EPUB：0 错误、0 警告 |
| 第三方许可通知生成 | 通过，无缺失许可包 |
| `npm run desktop:package` | 调整工具缓存并排除沙箱工具启动限制后，完整命令退出 0；Windows x64 NSIS 4.47MiB |

工具版本实测为 Node 24.16.0、Rust/Cargo 1.96.0。依赖锁文件保持不变。Rust 全工作区包含 CLI 强退后恢复、正文与状态事务、导出失败保护、来源隔离与能力限制、账号窗口 IPC 守卫、稳定读取及自动 TXT 后台测试。前端测试使用受控 DOM 和 Vue 内存宿主，不能作为原生窗口或真实服务证明。

## 打包阻塞与最小调整

首次 `npm run desktop:package` 在应用编译完成后失败，日志依次为 NSIS 下载、哈希校验、解压和 `拒绝访问 (os error 5)`。失败后重新检查 Git 状态和差异，业务源码没有改动。当前安装器配置默认将工具缓存置于用户级目录；本轮执行环境不能完成该路径的解压。

仅在 `apps/desktop/src-tauri/tauri.installer.conf.json` 加入 `bundle.useLocalToolsDir=true`，让构建工具使用项目 target 下的缓存。已对照已安装 CLI 的 `config.schema.json` 与 [Tauri 官方配置文档](https://v2.tauri.app/reference/config/#uselocaltoolsdir)。保留官方工具下载与哈希检查，不修改安装目录、权限、数据库 schema、用户会话或正文逻辑，不删除既有缓存。安装器构建重新完整执行，而不是把成功编译 EXE 当作打包通过。

调整后官方 NSIS 和插件下载、哈希验证及项目内解压全部通过。随后启动 `makensis.exe` 出现 Windows 错误 623（无效系统 DLL 重定位）；沙箱内直接查询 `/VERSION` 同样无法启动，沙箱外运行完全相同路径的文件则退出 0、返回 `v3.11`。据此将该层阻塞定位为执行环境限制，未更换 NSIS、关闭工具校验或修改 Windows DLL。经自动审批允许，在沙箱外继续同一项目的完整打包命令，产物仍写入项目目录，不执行安装。

最终 `npm run desktop:package` 退出 0，包含前端生产构建、原生 release 和 NSIS 打包。核对生成的 NSIS 脚本，包含主 EXE、Apache-2.0 `LICENSE` 和 `THIRD_PARTY_NOTICES.txt` 的打包指令；许可通知覆盖锁定图的 357 个包，缺失清单为空。该核对与构建结果不等于实际安装后验证。

## 性能基准

全部为原创、本地、debug 构建的合成观测，运行时与安装器编译并行，不用于推导真实官网吞吐量或发布性能承诺。

- 40 本 × 1000 章，30 次任务查询：全库快照 392.849ms，定向查询 8.366ms。
- 三轮各 1000 章完整任务：2611.433 / 2797.649 / 2831.916ms；复用 1000 章并新增 1 章：29.614 / 30.087 / 32.909ms；总请求计数均为 1001，缓存章节未重复获取。
- 原创 500 章各读取两次：重复解码 1017.949ms，复用已校验结果 535.084ms。
- 旧新 DOM 对照检查正文等值，并验证单次祖先样式缓存减少查询；原有身份、标题、字数、页面完成和两次稳定读取要求不变。

## 保留的验收边界

本次全量指当前工程提供的自动化检查、原创 CLI 闭环和构建链路，不代表所有平台、所有作品或所有未来阶段完成。未请求商业平台，故不更新 `docs/sources.md` 的真实来源实测日期或支持结论。

以下仍待独立验收：原生窗口交互和焦点、真实起点人工验证后连续恢复、本人已购章节长篇连续保存、书旗正文、其他操作系统、独立干净 Windows 与缺失 WebView2 安装分支、远端 CI。未自动操作验证码，未绕过订阅。正式 CA 证书仍缺失，签名测试成功不代表交付物已签名。

未安装或卸载本轮产物，未更新桌面快捷方式，未操作既有小说库和导出文件。后续本地受控构建产物单独保留；不自动 tag、push 或发布。

## 交付与版本

安装器独立另存为 `.artifacts/full-check-20261031/delivery/OpenNovel-Downloader-0.3.0-full-check-20261031-setup.exe`，4691459 字节，SHA-256：

`F48FFF6C1BEEC4047F71C655E1FFF06045E73080FFBB9D6A9F531C2EA0D47540`

副本与最终构建输出哈希一致，Authenticode 状态为 `NotSigned`。同目录包含许可证与第三方通知；`delivery.json` 记录范围、哈希及未验收项。旧交付版本保留。CLI 原创检查脚本、测试库和测试日志位于忽略目录，不进入版本控制。

本轮跟踪变更仅为安装器工具缓存配置、此验证报告及 `task.md`。提交前检查完整 diff、staged diff 和文件清单；不包含凭据、真实正文、书库、依赖目录或构建产物。稳定阶段本地提交，不创建 tag 或推送。独立 Windows 与原生账户验收通过后，再由用户决定版本 tag。

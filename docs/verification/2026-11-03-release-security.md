# v0.3.19 发布前功能与安全验收

日期：2026-11-03。基线提交：`89276a7f68b2f2bcc82b19c8f6b93947a2759652`。目标：Windows x64 一体化候选。未使用 Computer Use；真实原生窗口由 Playwright CLI 连接本轮独立 WebView2 检查。

结论：本轮已修复发现的依赖安全问题、加强并验证本地/远程权限边界，完成受控全量回归和候选打包。**仍不具备“所有平台完整下载、真实 VIP 全书、正式稳定发布全部验收完成”的证据。** 不将其改名为 1.0 稳定版，也没有公开发布。

## 发现与修复

| 问题 | 处理与证据边界 |
| --- | --- |
| npm minimatch 10.2.6 DoS 公告，经依赖链计为 12 个中等级条目 | 固定 11.0.1；lint、类型检查、前端测试及构建通过，复扫 0 条。属于开发依赖，未把它描述成运行时正文漏洞 |
| ZIP 4.6.1 的 4 条 RustSec 公告 | 升级 9.2.3，适配新 API 的 EPUB 测试；当前产品只生成 EPUB，未发现调用公告涉及的解压接口，仍完成升级和导出回归 |
| 内置引擎旧锁有 3 条证书相关漏洞、Tokio unsound 和撤回的 yoke-derive | 只更新独立 workspace 锁：rustls-webpki 0.103.16、webpki-roots / webpki-root-certs 1.0.10、Tokio 1.53.3、yoke-derive 0.8.4。993 项已审查上游文件不变；228 个依赖完整 vendor、许可表达式核对、离线重建及新摘要固定。新锁扫描无漏洞和告警 |
| 主界面内容策略可以更明确 | 显式 script-src、frame-src、form-action 限制；原生实际执行无害 DOM 探针证实被阻止 |
| 书旗部分免费作品 aut=0 被身份函数误拒绝 | 只允许该字段显式 0 或原有合法数字；固定域名、作品/章节身份、明确免费标识、重复参数拒绝保留。新增原创回归；实际原失败目录 11/11 项识别成功。正文复测随后遭 429，未标为通过 |
| 安全验证不具备统一入口 | 新增 `verify_security.py`、6 项检查契约测试、CI 依赖审计和 SECURITY.md。缺工具、无结果、报告错误不判通过；公告库网络失败最多重试三次，保留每次原始结果 |

公告参考：[minimatch GHSA](https://github.com/advisories/GHSA-g3hv-75wg-m249)、[ZIP RUSTSEC-2026-0429](https://rustsec.org/advisories/RUSTSEC-2026-0429.html)、[webpki RUSTSEC-2026-0382](https://rustsec.org/advisories/RUSTSEC-2026-0382.html)。具体适用版本、原始公告和退出码保存在本地审计记录中。

## 安全审计结果

使用 cargo-audit 0.22.2。npm 最终联网扫描 0 条；主程序和新引擎的 Rust 锁定依赖均为 0 条漏洞。公告库最终复核明确使用本轮成功获取的固定提交 `263a4d951a0867d4bddc29dddd6d59427c78f9a7`，含 1402 条公告，提交时间 `2026-11-02T12:45:44+01:00`。后续联网刷新多次失败，原始失败记录保留；**最终结果是指定快照复核，不宣称最新联网状态**。快照必须工作区干净、提交匹配且不超过三天，并在扫描前后核验。

仍保留三项主锁告警：`fxhash 0.2.1` 已停止维护且在 Windows 依赖树中，需跟进 scraper/selectors 上游替换；`proc-macro-error 1.0.4` 未维护和 `glib 0.18.5` unsound 不在本次 Windows 编译依赖树中。它们没有从报告抹去；本轮不为 Linux/macOS 提供安全或发布结论。没有忽略已知漏洞清单来制造通过结果。

原始报告：`.artifacts/release-security-20261103/audit-release/report.json`，包含主锁、npm 锁、引擎源归档和引擎锁 SHA-256，完整原始扫描结果与 Windows 依赖树。最终门禁强制包含 npm 开发/可选依赖，并拒绝 Rust 审计中的忽略列表、严重级别和平台过滤配置。扫描通过不是零漏洞保证或第三方安全认证。

## 已执行的功能与原生验证

- 最终统一检查 14/14 通过：fmt、Clippy、180 个 Rust 测试（9 个显式手动测试忽略）、lint、typecheck、78 个前端测试（1 个手动性能测试跳过）、80 个 Python 测试、前端构建、签名流程契约、组件准备、CLI 构建及原创导出、Windows NSIS 打包、原生原创内容安装自检。报告：`.artifacts/release-security-20261103/full-checks-delivery/report.json`。
- 上述测试覆盖事务、断点、取消、崩溃恢复、目录变更、同名保护、路径处理、导出转义及账号队列受控场景；不把它们当成真实已购验证。
- 新引擎真实 Windows AppContainer 验收通过：1000 次原创请求复用同一进程、正常退出清理、验证/限流停止、取消后新会话恢复、错书/付费标识拒绝、文件读写/网络/子进程拒绝、父进程强制退出后 Job 杀进程、清空 PATH 后内置 Python 工作进程可运行。报告：`.artifacts/release-security-20261103/native-sandbox/report.json`。1000 次的约 1.06 秒仅为原创本地夹具时间，不是网站下载速度。
- 最终 release EXE 的实际主窗口正确渲染 0.3.19。无害内联脚本未执行；iframe、外站表单及主窗口外站跳转被阻止。匿名飞卢官网窗口调用 `library_snapshot` 实际被拒绝：`Command library_snapshot not allowed by ACL`；关闭后会话状态为 closed。没有使用真实登录。回执：`.artifacts/release-security-20261103/native-ui/security-acceptance.json`；截图：`output/playwright/release-security-main.png`。
- 内置 Python EXE 签名为有效 Python Software Foundation；内置 WebView2 离线安装器签名为有效 Microsoft Corporation。没有将它们的签名误说成主程序已经签名。

## 真实来源复测

| 范围 | 本轮结果 |
| --- | --- |
| 番茄官网、七猫、晋江、纵横、17K 各两本 | 10 个样本共保存 30 个开放章节；8 个限制项正确标注；10 组 TXT 全段落检查及 EPUB ZIP/mimetype 检查通过。每本抽样约 2.1–3.3 秒，不是全书耗时 |
| 书旗两本固定样本 | 第一本发现 aut=0 误拦截并修复；目录复测 11/11 开放项识别成功。下载时返回 429（先 7 秒、后 87 秒冷却），停止该来源，第二本未继续请求；保留失败证据。本轮不能声称两本正文通过 |
| 番茄本地增强免费样本 7689165288515849240 | 新库 117/117 章，0 受限、0 失败；任务处理约 10.34 秒，自动 TXT 完整标记且逐段顺序核对通过。明确免费目录、不使用账号。由原生宿主验收入口调用与交付相同的更新组件，并非 GUI 每个按钮的全链路测试 |
| 起点及其他官网会话入口 | 入口探测/匿名窗口不足以证明已购权益；本轮没有真实账号登录或 VIP 全书验收 |

公开抽样：`.artifacts/release-security-20261103/public-sources/report.json`；书旗保留 `shuqi-fixed` 与 `shuqi-after-cooldown` 报告；新引擎样本：`free-book/acceptance.json` 及 `free-book/txt-verification.json`。这些书库和正文只留本地，不提交 Git 或随软件分发。没有任意作品三分钟承诺。

## 交付与正式发布剩余条件

候选目录：`.artifacts/security-delivery-20261103/`，含 GUI、分目录 CLI、一体化组件及完整对应源码、安装器、使用/安全说明、许可材料和 SHA-256 回执。用户书库保持原位置，验证不写入原库。程序和依赖变化不涉及新的数据库 schema。

最终 GUI SHA-256：`4e735a29268508fdab3bdd50a14fae966c9db17b8905db57647041db9c6db60c`；安装器 `OpenNovel Downloader_0.3.19_x64-setup.exe`，269972322 字节，SHA-256 `ad46fc25d362b12c601b3643b29e64de9d7d8457bb67878b5c42f477bcb686db`。交付副本在仅系统 PATH 下的原创安装自检、组件就绪检查通过，GUI 摘要与实际原生窗口验收相同。

原库 SHA-256 仍为 `8dc845a3bab0e23a4ba6845794268a88acf105008d20d97ede2980aded1c4518`，7 本、2136 章、10 个任务。只读一致性备份在 `.artifacts/security-private-backup-20261103/`，不混入可分发目录。桌面原快捷方式备份后更新，继续使用同一书库；验证没有启动原书库。恢复脚本会在快捷方式或书库发生后续变化时拒绝直接回退。

| 条件 | 状态与完成方法 |
| --- | --- |
| 本轮工程与安全回归 | 已通过上述明确范围；修复后重新构建，不沿用旧 EXE 的证据 |
| 书旗当前正文可用性 | 本轮受限流阻塞；按平台冷却要求另时复测，不将 429 改成正文或完成 |
| 真实已购全书与不同平台账号 | 未完成；必须使用用户本人拥有权益的账号逐平台验收，当前没有可用已购样本。不能代为购买或虚构登录成功 |
| 七猫/咪咕 App 正文、图片正文等未实现通道 | 不包含在当前发布候选支持承诺中，见 sources.md；需要单独研究、实现和验收 |
| 独立干净 Windows 安装/卸载/重启 | 未完成。本轮只读检查显示固件虚拟化未启用、Windows Sandbox 不存在；未改 BIOS/系统功能。清空 PATH 运行不等于干净系统 |
| 正式代码签名 | 未完成。本机仅发现另一项目的开发自签证书，不适用于本软件公开发布；未使用它签名，未申请或购买证书 |
| 公开发布批准与许可证材料复核 | 未执行发布、push 或 tag。分发新引擎必须同时提供其固定对应源码与许可文件 |

当前可交付的是可复核的 Windows 候选版本。正式稳定发布应在选定并如实声明支持范围后，补齐对应账号/环境条件、处理待复测项，再决定签名与发布方式；不能靠把未完成项改为勾选来“完结”。

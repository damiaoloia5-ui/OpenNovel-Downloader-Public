# OpenNovel Downloader

v0.3.3 离线一体化本机候选版：附带独立引擎镜像和完整对应源码，首次启用时校验并从本地包准备；不再要求用户下载镜像或建立开发构建卷。仍需 Docker Linux 后端，不安装系统组件、不绕过付费或验证。见[操作说明](docs/local-engine.md)及[本轮验证](docs/verification/2026-11-01-offline-engine.md)。以下保留历史进展。

v0.3.2 一体化本机候选版：内置固定官方 Python 与组件脚本，自动发现资源；首次主动启用后记住选择，下次粘贴链接直接保存，无需环境变量或启动脚本。启用时可尝试启动已安装的 Docker Desktop。Docker、固定镜像和引擎构建卷仍为系统依赖，不能称全依赖离线包。见[使用说明](docs/local-engine.md)和[本轮验证](docs/verification/2026-11-01-integrated.md)。以下保留历史进展。

2026-11-01 · v0.3.1 本机候选版：番茄本地组件已从空书库一次保存真实免费作品 102/102 章，约 152 秒并自动生成完整 TXT；前 101 章与参考结果相符，未导入参考正文。修复短暂故障误报验证和完整章字数元数据偏差，增加直接启用入口及原创样本安装检查。单一免费样本不代表全站或 VIP，组件仍依赖本机 Python/Docker 固定构建。见 [配置与操作](docs/local-engine.md) 和 [验收记录](docs/verification/2026-11-01-daily-delivery.md)。下文保留历史版本进展。

项目主线是完善软件自身的多来源正文获取、自动保存、进度恢复与 TXT 交付。具体作品是测试样本；本地文件导入作为辅助功能，不代替真实来源下载验收。当前共用网页下载稳定性改进见 [验证记录](docs/verification/2026-10-31-download-stability.md)。

2026-10-31 番茄补齐候选：修复网页受限时无效的“继续保存”和错误的官网登录引导；番茄已有目录可选择同书完整 UTF-8 TXT，核对全部章节标题后只补缺章并自动生成 TXT，保留原有正文。该功能不是番茄 App 自动下载，App 获取渠道仍待独立适配。见 [操作说明](docs/QUICK_START.md) 和 [诊断与验证](docs/verification/2026-10-31-fanqie-channel.md)。

2026-10-31 三家官网会话候选版：刺猬猫、飞卢、咪咕可进入独立临时官网窗口，由本人登录后识别当前页。刺猬猫/飞卢的普通可见文字可确认保存为独立 TXT，文件明确注明未核验全章/全书；图片、画布、限制或未知呈现拒绝。咪咕已找到可访问的悦看官网，当前仅作品页识别，正文仍待适配。没有完成三家自动全书或真实 VIP 验收。见 [操作与验证](docs/verification/2026-10-31-official-reader.md)。

2026-10-31 新来源候选版：新增纵横、17K 官网完整分卷目录、公开免费章节与共用 TXT 保存流程。刺猬猫、飞卢和咪咕列为待适配，链接提示不会创建下载任务。样本、边界与后续接入路线见 [五平台调研](docs/research/platform-expansion.md) 和 [验证记录](docs/verification/2026-10-31-source-expansion.md)。

2026-10-30 起点恢复修复候选版：验证或页面变化中断后可显示原官网窗口，由本人处理验证；继续前检查页面/会话状态，不重载已打开的当前章，沿原任务补齐缺章。验证仍在时不重新启动任务，提示原因并保留进度；不自动操作滑块。见 [恢复验证记录](docs/verification/2026-10-30-qidian-recovery.md)。

2026-10-30 简明向导候选版：粘贴链接后按当前提示操作；起点登录后自动尝试读取目录，核对书名及阅读权限后直接开始保存。已有作品回到原进度，书架直接打开 TXT 文件夹；高级管理、选章和速度折叠。使用说明见 [第一次使用](docs/QUICK_START.md)，受控交互及原生构建边界见 [验证记录](docs/verification/2026-10-30-simple-flow.md)。

2026-10-30 起点专项候选版：单次正文提取复用祖先样式结果，相同正文的第二次稳定确认复用已校验内容；新增默认 0.5 秒极速档，保留 1/3/6 秒档位。仍等待页面完成并执行两次稳定读取。受控千段样本样式查询减少约 94.6%，不是官网整书耗时测量。见 [起点专项验证](docs/verification/2026-10-30-qidian-speed.md)。

2026-10-30 队列算法候选版：批量建立缓存索引、待保存队列一次装载、预编译 SQL 复用与直接正文校验，保留逐章事务。原创千章本地任务三轮中位耗时减少约 30%，不含官网等待或导出，不能等同线上全书加速。见 [算法验证](docs/verification/2026-10-30-queue-algorithm.md)。

2026-10-30 长篇性能候选版：起点可选 1/3/6 秒保存节奏；网页默认 0.5 秒，服务器限流退避保留；任务/作品详情改为定向查询并减少后台任务重复分组。局部受控基准与官网整书耗时分开记录，见 [性能验证](docs/verification/2026-10-30-performance.md)。

2026-10-28 自动 TXT 版：粘贴支持的官网链接，点“识别并保存为 TXT”，保存结束后后台自动生成 TXT，工作台顶部直接打开文件所在目录。起点仍需本人登录与确认保存；部分内容明确标注，原手动导出/EPUB 保留。见 [快速开始](docs/QUICK_START.md) 与 [本轮验证](docs/verification/2026-10-28-automatic-txt.md)。

2026-10-28 可获取范围与缺章说明版：添加作品先看目录开放/受限/待验证数量，一键选择全部标记开放章；工作台查看缺章分类、原始原因和相关任务。目录标记不代表正文或阅读权益已验证，来源权限保持原范围。见 [快速开始](docs/QUICK_START.md) 与 [本轮验证](docs/verification/2026-10-28-availability.md)。

独立实现的多平台小说本地下载与管理项目，采用 Apache-2.0。当前为 **v0.3.1 本机候选版**：官网公开内容、本地书库、批量任务、备份恢复及修订历史已形成可验证流程；账户窗口处于实验阶段。

**未完成的门禁**：起点/书旗真实登录、权限失效和已购正文验收；独立干净 Windows 及缺失 WebView2 的安装验证。当前不称 v1.0 全部阶段完成，也不把官网窗口打开称为已购下载支持。2026-10-25 起点当前页实验已在原生窗口保存真实 984 章目录及公开首章，逐章预览和导出可用；真实已购/未购引擎验收仍待配合，书旗正文仍禁用下载。

开始使用见 [使用指南](docs/USER_GUIDE.md)，逐平台边界见 [来源状态](docs/sources.md)，阶段状态见 [执行清单](docs/REMAINING_PHASES.md) 和 [全阶段核对](docs/ACCEPTANCE_STATUS.md)。历史 v0.1/v0.2 安装包保留，新版不覆盖它们。

2026-10-26 作品工作台版：书架点击“打开作品工作台”，在同一本书内查看概览、保存、阅读和导出。新任务或起点确认目录后直接进入对应作品；状态提示区分选章完成与全书收齐，导出会记住格式。完整原生交互与真实账户门禁仍按 [本轮验证](docs/verification/2026-10-26-book-workbench.md) 区分，不因界面整合扩大平台承诺。

2026-10-26 操作引导版：粘贴起点链接自动进入四步引导，首次免填账户别名，已有作品可直接选择；默认保存全部，选章及管理操作折叠。导出统一为“选格式 → 导出文件 → 打开文件夹”，应用内新增“使用指引”。第一次使用可读 [五分钟操作说明](docs/QUICK_START.md)。本轮调整不改变平台支持或已购权益的验收状态。

2026-10-25 新增起点账户内“自动保存全书”实验：从已确认目录逐章补齐，支持范围、跳过已存、暂停及重启后选择账户继续，遇到访问限制或异常即停止。核心回归、真实浏览器 DOM 契约和原生构建通过；本轮原生窗口控制授权未完成，官网自动连续保存与真实已购全书仍待验收。见 [操作步骤](docs/USER_GUIDE.md#起点自动保存全书实验性) 和 [证据边界](docs/verification/2026-10-25-automatic-session-download.md)。

## 当前可用范围

- 用户提供的 JSON 书籍导入，每章内容和任务完成状态同时提交到 SQLite。
- 持久任务、独立暂停与取消、在途请求中止、中断后继续、重复作品保护。
- 共用 Rust 核心的 CLI 与 Vue/Tauri 桌面端。
- 从本地内容生成 UTF-8 TXT 或 EPUB 3；部分内容明确标注，已有导出文件不覆盖。
- 同一书库只有一个写入进程；正文保留原有章节顺序和字符串 ID。
- 番茄官网详情页链接/ID、目录、范围下载、字体还原、受限计数及缺失章节恢复；只保存网页实际开放的正文。
- 七猫官网详情、完整目录、明确免订阅的网页正文；检查章节身份、标题与声明字数，排除推荐作品正文。
- 书旗官网详情与完整目录；正文实测限流，当前禁用下载，不能把目录接入理解为正文可用。
- 核心统一提供来源能力；链接自动识别，任务恢复按已保存的来源选择适配器，跨平台同名 ID 相互隔离。
- 晋江官网完整目录、公开免费正文与最多 20 项的首屏搜索；VIP/锁定章节保留受限状态。
- 小批量持久队列（最多 10 个输入、5 个待处理批次）、跨来源去重、逐本执行及重启继续。
- 标签、归档、一致性备份、新目录恢复和脱敏诊断预览。
- 目录变动预览、显式确认、去重正文历史、旧文预览与导出；旧任务保持原范围和旧版本。
- 起点/书旗独立临时官网会话、关闭清理和远程 IPC 边界；起点新增“完整目录 → 当前章预览 → 确认保存 → 离线导出”实验，真实登录及已购正文未验收。
- 逐章错误、失败定向重试及末尾增章补齐。自动补齐仍拒绝删章/重排，此类变化使用显式确认入口。
- 连续范围或 `1,3,5-8` 离散选章；任务导出仅包含所选集合，书库导出包含该作品的全部已保存章节。
- 本地详情、目录检索与分页、正文预览、导出记录及打开文件夹。

首版范围经用户确认是“官网实际开放内容，缺章明确标注；App 正文后续扩展”。番茄前轮短篇 5/5、长篇保存 10/47；2026-10-21 七猫样本目录 4100 章，仅 14 章明确免订阅，本轮实际保存所选第 1/3/5 章并拒绝第 15 章。书旗样本完整目录 11 章，正文返回 429。晋江样本 252 章，所选 1/3 保存、21 VIP 和 252 锁定受限。上述均不是全站可用证明；起点当前页实验另有公开首章小样本记录；各平台账户/已购正文未通过验收。

核心功能、真实原生运行及本机安装/卸载已验证；**独立干净 Windows 环境的 AC-20 仍未通过完整验收**，因此此处标为候选版。最新证据见 [v0.2 多来源验证](docs/verification/2026-10-21-multi-source.md)，历史见 [v0.1 验收记录](docs/verification/2026-10-20-v01.md)。

## 安装候选版

新候选安装器为 `target/release/bundle/nsis/OpenNovel Downloader_0.3.0_x64-setup.exe`，交付副本、CLI、SHA-256 和 manifest 放在 `.artifacts/v03-release/`。候选包实测结果以 [v0.3 交付记录](docs/verification/2026-10-21-v03.md) 为准；不能仅凭该路径推断构建或安装成功。

双击安装器，按向导安装到当前用户。运行应用不需要 Node、Rust 或 Java；需要 WebView2。安装器配置了缺失时下载 WebView2 引导程序，因此首次补装运行时需要网络。包未进行代码签名；只在本机 Windows 11 x64 实测，其他系统尚未验收。

默认卸载保留书库和导出；勾选卸载器的“删除应用数据”会删除这些内容，请先备份。数据库升级会在迁移前备份旧版本；旧程序不能直接打开较新 schema，应恢复副本到独立目录验证。

## 环境

- Windows x64；Node.js >= 22.12；npm 11；当前开发验证使用 Rust 1.96 stable。
- Visual Studio C++ 工具链、Windows SDK、WebView2，见 [Tauri 前置要求](https://v2.tauri.app/start/prerequisites/)。
- Java 21 可用于开发阶段 EPUBCheck；应用本身不依赖 Java。

首次安装：`npm ci`。首次 Rust 构建会下载 crates 并编译内置 SQLite。

## 运行桌面应用

在仓库根目录执行 `npm run desktop:dev`。在“添加小说”粘贴番茄、七猫、晋江或书旗官网作品详情链接，读取目录。番茄、七猫与晋江可选章下载；书旗当前仅浏览目录。纯数字 ID 保持番茄兼容；其他来源使用 `qimao:作品ID` 、`shuqi:作品ID` 或 `jinjiang:作品ID`。本地验证可导入 `examples/local-book.json`。

书库位于系统应用数据目录的 `org.opennovel.downloader/library.db`，导出文件位于同目录的 `exports`。旧 schema 1～7 在升级前自动生成 SQLite 备份；当前 schema 8 增加批次、修订历史和非敏感账户记录。程序返回完整路径。打开文件夹仅接受数据库中的导出记录 ID，不开放任意路径或 shell 命令。

`npm run dev` 仅打开浏览器预览，无法访问本地核心，不会模拟书库成功结果。

## 运行 CLI

以下命令在仓库根目录执行：

```powershell
cargo run -p opennovel-cli -- sources
cargo run -p opennovel-cli -- inspect qimao:作品ID
cargo run -p opennovel-cli -- download qimao:作品ID --chapters "1,3,5"
cargo run -p opennovel-cli -- inspect shuqi:作品ID
cargo run -p opennovel-cli -- inspect https://fanqienovel.com/page/作品ID
cargo run -p opennovel-cli -- download https://fanqienovel.com/page/作品ID --from 1 --to 3
cargo run -p opennovel-cli -- download https://fanqienovel.com/page/作品ID --chapters "1,3,5-8"
cargo run -p opennovel-cli -- import examples/local-book.json
cargo run -p opennovel-cli -- list
cargo run -p opennovel-cli -- resume <job_id>
cargo run -p opennovel-cli -- details <job_id>
cargo run -p opennovel-cli -- book <book_id>
cargo run -p opennovel-cli -- read <book_id> <chapter_id>
cargo run -p opennovel-cli -- retry <job_id> --chapter <chapter_id>
cargo run -p opennovel-cli -- update <book_id>
cargo run -p opennovel-cli -- export <book_id> --format txt
cargo run -p opennovel-cli -- export <book_id> --format epub
cargo run -p opennovel-cli -- export-job <job_id> --format epub
```

作品 ID 替换为官网实际 ID，`<job_id>` 和 `<book_id>` 使用命令返回的实际 ID。CLI 默认书库是 `.artifacts/library.db`，可在子命令前通过 `--database <path>` 指定。不要将它指向一个已经被桌面应用打开的书库。任务完整完成退出码为 0，部分/受限/取消为 2，命令错误为 1；`download` 不传范围时选择全目录。

“所选范围完成”不表示整本完整。书库另外展示已保存章节与全目录的比例；部分导出在文件名及内容中标明缺失。网页受限章节不会保存截断预览。运行任务的在途请求可中止，暂停/取消后的迟到正文不会提交；单次网络超时为 20 秒。429 遵守 Retry-After，本轮停止，等待后可手动重试。活动任务的暂停/取消以桌面端为主，CLI 不提供跨进程控制另一实例的通道。

“查看章节明细”可按未保存、失败、受限或全部筛选，每页 100 章。`retry` 支持重复传入 `--chapter` 指定多个失败项，只执行本次指定范围；`resume` 会继续原任务所有待处理项。定向重试中取消或强退后，继续原任务仍按所有待处理项执行。受限章节不能直接重试，应在书库点击“检查更新并补齐”重新读取官网开放状态；这不会绕过访问限制。

检查更新会创建新的全目录补齐任务，复用已经保存的正文，并识别末尾新增章节；旧任务保留当时的执行记录。新目录不能删改旧章节身份或顺序。网络书籍完整导出的文件名加入 `complete-章节数`，以便增章后重新导出并保留旧文件；相同快照仍拒绝覆盖。

## 导入格式

根字段是 `id`、`title`、`author` 和 `chapters`；每章包含字符串 `id`、`title` 与字符串数组 `paragraphs`。具体示例见 [local-book.json](examples/local-book.json)。例文为本项目原创的测试内容。

文件最大 16 MiB，最多 10000 章。空正文、重复章节 ID、无效字段和不适合 XML 的控制字符会在创建任务前拒绝。作品 ID 在本地来源内唯一；相同作品重复导入不会覆盖已有内容。网络来源支持连续范围和离散选章；公开单章修订已实现，整书删除未开放。

## 开发检查

```powershell
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
npm run lint
npm run typecheck
npm run test
npm run build
npm run desktop:build
npm run desktop:package
```

`desktop:build` 生成原生应用，不生成安装器。`desktop:package` 先从锁定依赖生成许可证通知，再生成 Windows x64 NSIS 安装包；首次构建会下载 NSIS 工具。CI 配置了同样检查与安装包 artifact，尚未在远端运行。签名、自动更新及其他操作系统留待后续版本。

## 工程结构

| 目录 | 职责 |
| --- | --- |
| `crates/core` | 来源适配、网络预算、字体还原、SQLite、持久任务、导出 |
| `crates/cli` | 共用核心的命令行下载与书库入口 |
| `apps/desktop/src` | Vue 交互与类型化 IPC 调用 |
| `apps/desktop/src-tauri` | 系统数据目录、后台执行和窄命令接口 |
| `examples` | 本项目原创导入样例 |
| `docs` | 架构决策、阶段验证记录及来源状态 |

完整产品路线见 [项目计划书](PROJECT_PLAN.md)，本轮进度见 [task.md](task.md)。正式开发已在业务代码创建前初始化 Git 并提交规划基线。

2026-10-20 的验证范围和原生运行记录见 [工程基础验证记录](docs/verification/2026-10-20-foundation.md)。

平台范围见 [来源状态](docs/sources.md)，实际获取路径与限制见 [番茄研究](docs/research/fanqie-web.md)、[七猫与书旗研究](docs/research/qimao-shuqi-web.md)。本轮实施路线见 [多来源方案](docs/MULTI_SOURCE_PLAN.md)。晋江公开内容已接入；下一步是起点/书旗真实账户与已购场景及独立干净 Windows 验收。

历史章节明细与更新证据见 [前轮验证记录](docs/verification/2026-10-20-chapter-details.md) 和 [v0.1 验收记录](docs/verification/2026-10-20-v01.md)。

最新候选增加书架/任务分页、状态筛选、主书库快照的空闲及原生最小化调度、重复导出明确提示；本轮完整回归、真实来源复验及安装数据保护证据见 [全量检验与优化](docs/verification/2026-10-21-full-check.md)。

## 可选的 Windows 代码签名

开发和自用不需要购买签名证书。正式对外分发时可使用已接入的签名/验签入口；当前候选包仍未签名。办理条件和命令见 [Windows 代码签名](docs/WINDOWS_SIGNING.md)，本轮实际检查见 [签名验证记录](docs/verification/2026-10-21-signing.md)。

## 开源与贡献

项目采用 [Apache-2.0](LICENSE)。依赖保留各自许可证，安装目录附带自动生成的 `THIRD_PARTY_NOTICES.txt`，补充文本及来源见 [third_party](third_party/README.md)。不复制参考下载器代码，不在仓库分发采集正文。贡献与问题报告方式见 [CONTRIBUTING.md](CONTRIBUTING.md)。

# OpenNovel Downloader

独立实现的多平台小说本地下载与管理项目。当前版本 **0.0.1 开发预览**：本地书库基础与实验性番茄官网网页来源。第一版先完成免费平台，再逐步研究起点、晋江等付费平台。

## 当前可用范围

- 用户提供的 JSON 书籍导入，每章内容和任务完成状态同时提交到 SQLite。
- 持久任务、后台取消、中断后继续、重复作品保护。
- 共用 Rust 核心的 CLI 与 Vue/Tauri 桌面端。
- 从本地内容生成 UTF-8 TXT 或 EPUB 3；部分内容明确标注，已有导出文件不覆盖。
- 同一书库只有一个写入进程；正文保留原有章节顺序和字符串 ID。
- 番茄官网详情页链接/ID、目录、范围下载、字体还原、受限计数及缺失章节恢复；只保存网页实际开放的正文。
- 逐章状态和最近一次错误原因、失败章节定向重试、检查更新并补齐；支持目录末尾增章，删章/插章/重排时停止更新。

**第一版尚未完成。** 番茄网页来源仍为实验性，长篇样本 47 章中只能保存网页开放的 10 章，其他 37 章明确记录为受限；另一短篇样本当前目录 5/5 章完成。不能据此声称全站下载可用。七猫、书旗、起点、晋江尚未接入；账户及 VIP 能力未实现。

## 环境

- Windows x64；Node.js >= 22.12；npm 11；当前开发验证使用 Rust 1.96 stable。
- Visual Studio C++ 工具链、Windows SDK、WebView2，见 [Tauri 前置要求](https://v2.tauri.app/start/prerequisites/)。
- Java 21 可用于开发阶段 EPUBCheck；应用本身不依赖 Java。

首次安装：`npm ci`。首次 Rust 构建会下载 crates 并编译内置 SQLite。

## 运行桌面应用

在仓库根目录执行 `npm run desktop:dev`。在“添加小说”粘贴番茄作品详情页链接或 ID，读取目录、选择序号范围并开始下载。本地验证可点击“导入本地书籍”，选择 `examples/local-book.json`。

书库位于系统应用数据目录的 `org.opennovel.downloader/library.db`，导出文件位于同目录的 `exports`。旧 schema 1/2 书库在升级前自动生成 SQLite 备份；当前 schema 3 保存逐章最近一次错误。程序返回完整路径。桌面端不向界面开放任意文件写入或 shell 执行权限。

`npm run dev` 仅打开浏览器预览，无法访问本地核心，不会模拟书库成功结果。

## 运行 CLI

以下命令在仓库根目录执行：

```powershell
cargo run -p opennovel-cli -- inspect https://fanqienovel.com/page/作品ID
cargo run -p opennovel-cli -- download https://fanqienovel.com/page/作品ID --from 1 --to 3
cargo run -p opennovel-cli -- import examples/local-book.json
cargo run -p opennovel-cli -- list
cargo run -p opennovel-cli -- resume <job_id>
cargo run -p opennovel-cli -- details <job_id>
cargo run -p opennovel-cli -- retry <job_id> --chapter <chapter_id>
cargo run -p opennovel-cli -- update <book_id>
cargo run -p opennovel-cli -- export <book_id> --format txt
cargo run -p opennovel-cli -- export <book_id> --format epub
```

作品 ID 替换为官网实际 ID，`<job_id>` 和 `<book_id>` 使用命令返回的实际 ID。CLI 默认书库是 `.artifacts/library.db`，可在子命令前通过 `--database <path>` 指定。不要将它指向一个已经被桌面应用打开的书库。任务完整完成退出码为 0，部分/受限/取消为 2，命令错误为 1；`download` 不传范围时选择全目录。

“所选范围完成”不表示整本完整。书库另外展示已保存章节与全目录的比例；部分导出在文件名及内容中标明缺失。网页受限章节不会保存截断预览。请求进行中的取消最多等待单次 20 秒超时；429 或页面变化停止本轮，需手动重试。

“查看章节明细”可按未保存、失败、受限或全部筛选，每页 100 章。`retry` 支持重复传入 `--chapter` 指定多个失败项，只执行本次指定范围；`resume` 会继续原任务所有待处理项。定向重试中取消或强退后，继续原任务仍按所有待处理项执行。受限章节不能直接重试，应在书库点击“检查更新并补齐”重新读取官网开放状态；这不会绕过访问限制。

检查更新会创建新的全目录补齐任务，复用已经保存的正文，并识别末尾新增章节；旧任务保留当时的执行记录。新目录不能删改旧章节身份或顺序。网络书籍完整导出的文件名加入 `complete-章节数`，以便增章后重新导出并保留旧文件；相同快照仍拒绝覆盖。

## 导入格式

根字段是 `id`、`title`、`author` 和 `chapters`；每章包含字符串 `id`、`title` 与字符串数组 `paragraphs`。具体示例见 [local-book.json](examples/local-book.json)。例文为本项目原创的测试内容。

文件最大 16 MiB，最多 10000 章。空正文、重复章节 ID、无效字段和不适合 XML 的控制字符会在创建任务前拒绝。作品 ID 在本地来源内唯一；相同作品重复导入不会覆盖已有内容。网络来源支持连续范围选章，尚无离散选章、修订和删除功能。

## 开发检查

```powershell
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
npm run lint
npm run typecheck
npm run test
npm run build
npm run desktop:build
```

`desktop:build` 生成原生应用，不生成安装器。打包安装、签名、跨平台发布和自动更新尚未验收。

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

平台范围见 [来源状态](docs/sources.md)，实际路径、字体映射复现和限制见 [番茄研究](docs/research/fanqie-web.md)。工程基础、实验性接入与 v0.1 验收分别记录，后续重点是免费平台覆盖、错误明细、恢复和安装验收。

章节明细、指定重试、目录更新和原生检查的最新证据见 [本轮验证记录](docs/verification/2026-10-20-chapter-details.md)。

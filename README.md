# OpenNovel Downloader

独立实现的多平台小说本地下载与管理项目。当前版本 **0.0.1 工程预览**，从本地导入建立技术基础。

## 当前可用范围

- 用户提供的 JSON 书籍导入，每章内容和任务完成状态同时提交到 SQLite。
- 持久任务、后台取消、中断后继续、重复作品保护。
- 共用 Rust 核心的 CLI 与 Vue/Tauri 桌面端。
- 从本地内容生成 UTF-8 TXT 或 EPUB 3；部分内容明确标注，已有导出文件不覆盖。
- 同一书库只有一个写入进程；正文保留原有章节顺序和字符串 ID。

**番茄、七猫、书旗、起点和晋江均尚未接入。** 本地导入不代表商业平台下载能力。账户与 VIP 场景按计划分阶段研究，未购买内容的获取能力尚未验证。

## 环境

- Windows x64；Node.js >= 22.12；npm 11；当前开发验证使用 Rust 1.96 stable。
- Visual Studio C++ 工具链、Windows SDK、WebView2，见 [Tauri 前置要求](https://v2.tauri.app/start/prerequisites/)。
- Java 21 可用于开发阶段 EPUBCheck；应用本身不依赖 Java。

首次安装：`npm ci`。首次 Rust 构建会下载 crates 并编译内置 SQLite。

## 运行桌面应用

在仓库根目录执行 `npm run desktop:dev`。点击“导入本地书籍”，选择 `examples/local-book.json`。

书库位于系统应用数据目录的 `org.opennovel.downloader/library.db`，导出文件位于同目录的 `exports`。程序返回完整路径。桌面端不向界面开放任意文件写入或 shell 执行权限。

`npm run dev` 仅打开浏览器预览，无法访问本地核心，不会模拟书库成功结果。

## 运行 CLI

以下命令在仓库根目录执行：

```powershell
cargo run -p opennovel-cli -- import examples/local-book.json
cargo run -p opennovel-cli -- list
cargo run -p opennovel-cli -- resume <job_id>
cargo run -p opennovel-cli -- export <book_id> --format txt
cargo run -p opennovel-cli -- export <book_id> --format epub
```

`<job_id>` 和 `<book_id>` 使用命令返回的实际 ID。CLI 默认书库是 `.artifacts/library.db`，可在子命令前通过 `--database <path>` 指定。不要将它指向一个已经被桌面应用打开的书库。

## 导入格式

根字段是 `id`、`title`、`author` 和 `chapters`；每章包含字符串 `id`、`title` 与字符串数组 `paragraphs`。具体示例见 [local-book.json](examples/local-book.json)。例文为本项目原创的测试内容。

文件最大 16 MiB，最多 10000 章。空正文、重复章节 ID、无效字段和不适合 XML 的控制字符会在创建任务前拒绝。作品 ID 在本地来源内唯一；相同作品重复导入不会覆盖已有内容。当前尚无修订、选章和删除功能。

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
| `crates/core` | 输入模型、SQLite、持久导入任务、导出 |
| `crates/cli` | 无界面的本地核心入口 |
| `apps/desktop/src` | Vue 交互与类型化 IPC 调用 |
| `apps/desktop/src-tauri` | 系统数据目录、后台执行和窄命令接口 |
| `examples` | 本项目原创导入样例 |
| `docs` | 架构决策、阶段验证记录及来源状态 |

完整产品路线见 [项目计划书](PROJECT_PLAN.md)，本轮进度见 [task.md](task.md)。正式开发已在业务代码创建前初始化 Git 并提交规划基线。

2026-10-20 的验证范围和原生运行记录见 [工程基础验证记录](docs/verification/2026-10-20-foundation.md)。

本轮只建立工程基础，不代表 v0.1 的真实来源验收完成。下一步以番茄为首候选完成目录、正文、错误与限流验证，再接入通用网络适配边界。

# 工程基础验证记录

日期：2026-10-20。阶段：W01/W02，工程预览 0.0.1。

## 验证对象与版本

- 仓库规划基线：`dea0770`，main 分支；基线提交先于业务代码创建。
- 本轮对象：随本记录一起提交的 Rust 核心、CLI、Vue/Tauri 桌面端及锁文件。后续代码修改需重新验证。
- Windows x64，Node.js 24.16.0，Cargo/Rust 1.96.0，Tauri Rust 2.12.3。
- 实际原生 WebView：Microsoft Edge WebView2 155.0.4283.62。
- 测试内容：本项目原创示例及由原创文字构造的 10000 章本地文档，无商业平台内容或账户。

## 命令检查

| 检查 | 结果 | 证据范围 |
| --- | --- | --- |
| `cargo fmt --all -- --check` | 通过 | Rust 源码与测试 |
| `cargo clippy --workspace --all-targets -- -D warnings` | 通过 | 核心、CLI、桌面端和测试；0 警告 |
| `npm run test`（`cargo test --workspace`） | 11 项通过，0 失败 | 10 项核心集成测试及 1 项 CLI 进程恢复测试 |
| `npm run lint` | 通过 | Vue、TypeScript 和 Vite 配置 |
| `npm run typecheck` | 通过 | 前端类型检查 |
| `npm run build` | 通过 | 15 个模块；JS 约 74.56 kB，CSS 约 7.59 kB，未压缩 |
| `npm run tauri --workspace @opennovel/desktop -- build --no-bundle --debug` | 通过 | Windows 原生调试可执行文件 |
| `npm run desktop:build` | 通过 | Windows Release 可执行文件，未创建安装器 |
| `npm run desktop:dev` | 通过 | Vite + 原生 WebView + Rust IPC；图标引用修正后 0 个控制台错误/警告 |

Rust 测试覆盖：字符串章节 ID 与顺序、重复导入、取消及继续、取消前零写入、事务失败回滚、受控中断恢复、XML 转义与 EPUB 结构、单写入实例锁，以及拒绝较新数据库版本。

补查空作者输入时，EPUBCheck 复现 `RSC-005`：空的 `dc:creator` 不符合格式规则。回归测试先在修复前失败，再将空作者改为省略可选 creator 元数据；修复后的回归测试及完整测试均通过。有作者的作品继续保留转义后的作者元数据。

CLI 进程恢复测试会启动真实子进程，在观察到部分章节已提交后终止该进程；重开书库检查 paused 状态、已保存数量和正文一致，继续至 10000/10000，再核对章节 ID 无重复。这不是仅修改状态或使用异常代替终止进程的测试。

## 原生桌面验证

使用 Playwright CLI 连接本轮启动的真实 Tauri WebView，页面地址为 `http://tauri.localhost/`。没有用普通浏览器预览或模拟 IPC 替代原生验证。

| 场景 | 观察结果 |
| --- | --- |
| 启动及读取空书库 | 界面正常，Rust 返回 0 本作品 |
| 原生文件选择及导入 | 选择 `examples/local-book.json`，任务完成，书库显示 2/2 章 |
| TXT/EPUB 导出 | 两种文件实际生成，界面返回系统应用数据目录中的完整路径 |
| 10000 章后台导入 | 界面可继续显示和操作，章节计数持续更新 |
| 原生取消及继续 | 保存到 56/10000 时取消，显示“已取消”；继续后最终 10000/10000 |
| 重复导入 | 返回作品已存在的提示，保留原作品与任务 |
| 重复导出 | Windows 返回文件已存在错误，现有文件未被覆盖 |
| Release 重启读取 | 关闭调试实例，启动 Release 应用，仍显示 3 本验证作品、20002 章、0 个运行任务 |
| 书库搜索 | 输入“纸页”，列表正确筛选到原始样例 |
| 来源说明页 | 本地 JSON 标为已接入，商业平台均为未接入 |
| WebView 控制台 | 读取到 0 个错误及 0 个警告 |
| 数据库只读复核 | `PRAGMA integrity_check` 返回 `ok`；两本长篇各 10000 章，样例 2 章；待处理项为 0 |
| 空作者与特殊字符修复复核 | 最终 Release 导入 1 章边界样例，显示“未标注作者”，按原文显示 XML 符号，导出成功且控制台无错误 |

原生验证截图与页面快照保存在本机忽略目录 `output/playwright/` 和 `.playwright-cli/`。验证进程已关闭，本轮设置的调试端口不由应用代码默认启用。

本轮原生测试在默认系统应用数据目录创建了原创验证书籍，保留这些数据；未删除用户书库。CLI 书库和导出保存在 `.artifacts/smoke-20261020/`，与桌面书库分开。

在前述 3 本/20002 章验证之后，追加了 1 本/1 章空作者边界样例。最终本机测试书库为 4 本原创验证作品、20003 章。

开发启动复核发现默认 `/favicon.ico` 请求为 404。入口页已显式引用项目原创 SVG 图标；复核 `/app-icon.svg` 返回 200，最终 Release 已重新构建并再次启动，确认书库 IPC 和控制台正常。

## EPUB 合规验证

ZIP/XML 结构和转义测试通过后，另使用官方 EPUBCheck 5.4.0 对 CLI 与桌面应用实际生成的 EPUB 执行校验。普通作品与修复后的空作者/特殊字符样例均按 EPUB 3.3 规则通过：0 fatals、0 errors、0 warnings、0 infos。

校验器通过官方公布的 Maven Central 坐标 `org.w3c:epubcheck:5.4.0` 获取，依赖保存在本机 `.tools/epubcheck-maven/lib/`；应用不需要 Java/Maven。官方分发依据见 [EPUBCheck 发布说明](https://github.com/w3c/epubcheck/releases/tag/v5.4.0)。GitHub ZIP 分发连接超时后改用同版本 Maven 工具，没有降低格式校验要求。

实际运行入口为 `java -cp ".tools/epubcheck-maven/lib/*" com.adobe.epubcheck.tool.Checker <实际生成的文件.epub>`。校验通过不替代目标阅读器兼容性验收。

## 原生构建产物

- 文件：`target/release/opennovel-desktop.exe`。
- 长度：10720256 字节。
- SHA-256：`C4F2132AAADA7955B22C801AB246D18A800A1987617C0441B2747C9357F477FD`。
- 构建文件、书库、导出内容和工具缓存均不提交 Git。

## 限制与下一阶段

- 未验证番茄、七猫、书旗、起点或晋江的真实下载；没有账户与 VIP 实现。
- 当前只验证本机 Windows x64；macOS/Linux、干净系统安装、签名与更新仍待后续阶段。
- 目标电子书阅读器的兼容性验收待 v0.1；原生应用运行和 EPUB 结构测试不能替代这项验收。
- GitHub Actions 配置已提供，本轮没有推送远程或声称远程 CI 已运行。
- 本轮形成可执行的工程基础，不代表 v0.1 或真实来源准入 G1 已完成。下一阶段先执行 W03/W04 来源预研，再实现网络来源适配。

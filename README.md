# OpenNovel Downloader

**下载：[Windows x64 安装包](https://github.com/damiaoloia5-ui/OpenNovel-Downloader-Public/releases/download/v0.3.19/OpenNovel-0.3.19-Windows-x64-setup.exe) · [便携 ZIP](https://github.com/damiaoloia5-ui/OpenNovel-Downloader-Public/releases/download/v0.3.19/OpenNovel-0.3.19-Windows.zip) · [全部历史版本及校验值](https://github.com/damiaoloia5-ui/OpenNovel-Downloader-Public/releases)**

公开源码位于 [OpenNovel-Downloader-Public](https://github.com/damiaoloia5-ui/OpenNovel-Downloader-Public)。v0.3.19 已发布，安装包尚未正式代码签名；支持范围及未完成验收见下文。每个版本独立保存安装包、便携包、完整对应源码与校验值，并启用 Release 不可变保护。主程序和引擎分别保留许可证。

2026-11-03 · **v0.3.19 Windows x64 一体化版**：更新主程序及独立引擎的安全依赖，收紧主窗口 CSP，加入可重复依赖审计及 CI 检查，修复书旗官方免费链接 `aut=0` 被误拒绝。全量受控检查、原生权限隔离通过；新引擎实测免费样本 117/117 章并核对自动 TXT。本轮书旗正文复测遇限流，真实已购全书、独立干净系统和正式签名仍未完成，**不标为正式稳定版或全平台完整下载**。见[发布前功能与安全验收](docs/verification/2026-11-03-release-security.md)及[安全说明](SECURITY.md)。

2026-11-03 · v0.3.18：修复九平台账户被旧数据库约束阻止、首次官网窗口加载时误停、飞卢普通文字章节缺少声明字数的问题；新增咪咕官网公开试读导出，保留作者与简介；当前页 TXT 支持自选文件夹、简洁命名和同名保护。飞卢两本公开首章已在真实浏览器验证，一本前两章原生保存及自动 TXT、咪咕原生界面试读导出通过。真实已购全书、七猫/咪咕 App 全文及图片正文仍未验收，不能宣称全部平台完整可用。见 [本轮验证与剩余条件](docs/verification/2026-11-03-reader-completion.md)。

2026-11-03 · v0.3.16：新增书旗官网明确免费正文保存，用户两本样本各 20 章通过真实下载与 TXT 核对；其余章节未开放，不代表全书。其他平台账号/完整正文缺口仍保留，见 [本轮验证](docs/verification/2026-11-03-platform-expansion.md)。以下为既有版本记录和使用说明。

独立实现的多来源小说本地保存与管理软件。输入支持的作品链接，保存实际可获取的正文，自动生成 UTF-8 TXT；提供 EPUB、断点恢复、离线阅读、缺章说明、书库备份和修订历史。

此前 **v0.3.15 Windows x64 一体化版**。已对用户指定的 12 部作品先测量再统一修复，番茄原生增强通过 HTTPS 复用与最多三章预取，在新书库完成 2033/2033 章约 153 秒、250/250 章约 23 秒；TXT 逐段核对通过，插图有明确遗漏提示。其余 10 本仍因网页范围、验证或未接入能力未完成全书，不能称全平台支持或任意书三分钟。见[逐书耗时与问题清单](docs/verification/2026-11-02-twelve-books.md)。账号能力以[来源状态](docs/sources.md)为准；原书库和既有正文保留。

## 使用

0.3.20 源码新增“软件更新”：连接公开 GitHub Release，验签后备份各用户书库再安装。
旧版需要先手动安装带更新功能的版本一次；当前公开下载链接仍指向已发布的 0.3.19。
维护者发布时需要同时提供安装包、签名和更新清单，见[自动更新指南](docs/automatic-updates.md)。

1. 运行维护者提供的 Windows x64 安装包，安装后打开软件。
2. 在“添加小说”粘贴官网作品链接，点“开始保存为 TXT”。
3. 在“自动 TXT 文件”下的“本书保存位置”点“选择文件夹”，可指定本书保存目录；不选则每本书使用独立默认目录。
4. 等任务结束，点“打开 TXT 所在文件夹”。文件含作品信息与简介，再次打开同一本书可继续原进度。

**给普通用户使用 Windows x64 一体化安装包。** 已内置 Python、独立 Windows 引擎、对应源码和 WebView2 离线安装器，无需另外安装 Docker、Python、Rust 或 Node。番茄本地增强首次默认关闭，主动启用后记住选择；其他平台能力仍以[来源说明](docs/sources.md)为准。基础包只提供网页与本地功能。不要只复制 EXE，须保留随包组件目录。

保存中先暂停再退出。TXT 缺失时可从本地书库重新生成，已有编辑版不会被覆盖；重新生成不能补出没有保存的章节。操作步骤见[第一次使用](docs/QUICK_START.md)和[完整指南](docs/USER_GUIDE.md)。

## 来源范围

| 来源 | 当前范围 |
| --- | --- |
| 番茄、七猫、晋江、纵横、17K | 实验性官网目录及公开正文；以逐章实际结果为准 |
| 番茄本地增强 | 可选独立组件；Windows 当前免费样本通过 116/116，不代表全站 |
| 书旗 | 详情、目录与官网明确免费正文；两本样本各 20 章通过，其余章节未开放 |
| 起点 | 本人官网会话内保存实验；真实已购全书仍待验收 |
| 刺猬猫、飞卢 | 目录、账户会话内连续保存普通可见文字实验及当前页 TXT；飞卢前两章原生保存通过，真实已购全书未验收 |
| 咪咕 | 官网手机公开试读、作品信息和独立 TXT；不计入全书完成，不是 App 全文 |
| 本地 JSON | 原创样例导入、持久任务和导出；不是在线平台接入证明 |

免费、已登录、目录可见不等于正文完整。缺章、受限和失败会明确记录；不自动购买或处理验证码。最新日期和样本见[来源状态](docs/sources.md)，账户与安装门禁见[验收状态](docs/ACCEPTANCE_STATUS.md)。

## 开发与检查

Windows x64 开发需要 Rust >=1.96、Node >=22.12、Python >=3.11、Visual Studio C++ 工具链、Windows SDK 和 WebView2。Java 仅用于额外的 EPUBCheck 验证，应用无需 Java。

```powershell
npm ci
npm run desktop:dev
```

浏览器预览 `npm run dev` 无法调用本地核心。完整受控检查、桌面构建及原创样本原生自检：

```powershell
python scripts/verify_project.py --package basic --native
```

检查生成全新的 `.artifacts/checks/<时间与随机标识>/report.json` 及逐项日志，失败返回非零，不请求商业小说平台。已有 Cargo 缓存可加 `--offline`。原生自检只创建独立原创样本，不能替代窗口交互、真实账号或另一台系统验收。

基础包：`npm run desktop:package`。一体化包需要固定离线组件材料，由维护者显式准备，不从陌生解析服务下载。完整构建和从新源码副本复验步骤见[构建与交付](docs/BUILD_AND_RELEASE.md)。

## 工程与数据

| 目录 | 职责 |
| --- | --- |
| `crates/core` | 来源适配、校验、SQLite、持久队列、恢复和导出 |
| `crates/cli` | 复用核心的命令行入口 |
| `apps/desktop/src` | Vue 界面与类型化 IPC，不抓取或存储正文 |
| `apps/desktop/src-tauri` | 原生系统接口、官网会话及后台 TXT |
| `scripts` | 检查、构建、组件准备与原创受控测试 |

默认书库位于当前系统用户的应用数据目录 `org.opennovel.downloader/profiles/<用户ID>/library.db`，导出位于该用户的 `exports/平台－书名－作者`。侧栏可以新建空白用户、切换独立书库；原有书库登记后保留原位置，不复制给新用户。界面偏好、官网会话和任务也按用户隔离。同一书库只允许一个写入实例。软件内用户不是密码保护，需要互相保密时使用独立 Windows 账号。升级前备份，卸载默认保留数据。详见 [本地用户与干净分发](docs/local-users.md)。

v0.3.18 首次打开旧库会自动备份并升级到 schema 10。回退程序时应使用升级前备份的副本，旧程序不能直接打开已升级的书库。

CLI 示例：

```powershell
cargo run -p opennovel-cli -- sources
cargo run -p opennovel-cli -- import examples/local-book.json
cargo run -p opennovel-cli -- list
```

其他命令见[使用指南](docs/USER_GUIDE.md)。源码、书库、导出和组件不是同一类材料：书库、正文、凭据、缓存与构建产物均不提交 Git。

## 维护与许可

主程序采用 [Apache-2.0](LICENSE)。可选引擎独立提供，保留 GPL-3.0 和完整对应源码；第三方许可见[通知说明](third_party/README.md)。不复制参考项目代码，不将第三方组件改称主项目许可。

参与开发见[贡献指南](CONTRIBUTING.md)，范围依据为[项目计划](PROJECT_PLAN.md)。历史阶段记录保留在[进展归档](docs/PROGRESS_HISTORY.md)，当前状态以收尾清单和最新验收为准。没有用户明确授权，不自动推送、创建 tag、公开发布或购买签名服务。

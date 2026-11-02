# OpenNovel Downloader

独立实现的多来源小说本地保存与管理软件。输入支持的作品链接，保存实际可获取的正文，自动生成 UTF-8 TXT；提供 EPUB、断点恢复、离线阅读、缺章说明、书库备份和修订历史。

当前为 **v0.3.11 Windows x64 一体化候选版**。修复七猫作品页“最新章节”缓存滞后造成的目录导入失败，并区分网页渠道受限与 App 免费阅读，见[七猫验证](docs/verification/2026-11-02-qimao-catalog.md)。新增作品信息和简介导出、每本书独立保存目录及原生文件夹选择，文件名简化为“平台－书名－作者”，见[导出验证](docs/verification/2026-11-02-export-layout.md)。修复番茄本地引擎因间隔号等字符的字数统计差异而提前停止；Windows 原生链路已实测一个免费作品 116/116 章入库并自动生成 TXT。已实现功能与已验收能力分开记录，尚未完成真实账号权益和独立干净 Windows 等门禁，不能称所有平台或正式稳定版验收完成。见[本轮验证](docs/verification/2026-11-02-fanqie-word-count.md)、[项目收尾清单](docs/PROJECT_CLOSEOUT.md)和[界面设计规范](DESIGN.md)。

## 使用

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
| 书旗 | 详情与目录，正文下载禁用 |
| 起点 | 本人官网会话内保存实验；真实已购全书仍待验收 |
| 刺猬猫、飞卢 | 本人登录后当前页普通可见文字独立 TXT 实验，不是自动全书 |
| 咪咕 | 官网会话和作品页识别，正文未验收 |
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

默认桌面书库位于系统应用数据目录 `org.opennovel.downloader/library.db`，导出默认在同目录 `exports/平台－书名－作者`，可在工作台按书更改；自定义启动参数可能指定其他书库。同一书库只允许一个写入实例。升级前备份，卸载默认保留数据；删除应用数据前先导出一致性备份。

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

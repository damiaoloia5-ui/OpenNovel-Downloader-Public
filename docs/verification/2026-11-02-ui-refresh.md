# v0.3.7 界面升级验证

2026-11-02。修改前 `git status --short --branch` 干净，基线 `e8400a3a30e2aa3b480f1c986378e4d1a3edaac4`。沿用 PRODUCT.md 的中文、绿色、简明操作原则。图查询未索引后读取实际 Vue 与样式；未使用 Computer Use。

## 修改范围

- `App.vue`、新 `AppIcon.vue`、`style.css`：深绿导航、浅色工作区、统一图标/控件/状态色、稳定五组排版封面、章节进度及多宽度适配。
- `AutomaticTxtPanel.vue`、`BookWorkspace.vue`：突出的 TXT 文件夹按钮、按钮间距、工作台页签和 16px 离线正文；窄屏页签换行。
- Account、AutoSave、Availability、Batch、Capture、ExportActions、HelpGuide、LibraryTools、LocalEngine、OfficialReader、Revision、SourceSearch、TxtSupplement 面板统一主题变量，来源搜索长文本允许换行。
- npm/Cargo/Tauri 与锁文件同步为 0.3.7，资源配置使用本版本的固定组件目录；未增加 npm 依赖。
- 新 DESIGN.md；更新 README、CHANGELOG、快速指南、验收状态、收尾清单及 task.md。

除图标导入和确定性封面配色函数外，原 Vue 业务脚本与基线逐字比较相同。未修改 Rust 业务代码、数据库 schema、网络来源、账户权限或正文获取边界。封面为本地文字排版，并非官网封面。

## 实际渲染

以真实 App.vue、子组件及 CSS 启动隔离 Vite 页面，只替换 API 和原生窗口接口。使用六部原创示例作品，覆盖长书名、完整/部分/未保存、运行/完成/受限任务、自动 TXT 和原创阅读正文；未读取真实书库或浏览器账户。

通过终端 Playwright CLI 驱动独立无头 Edge，截图后人工查看：

| 检查 | 结果 |
| --- | --- |
| 添加、书库、任务、帮助、工作台 | 1280/1120/760/390px，页面无横向溢出 |
| 阅读页 | 1280/760/390px，无横向溢出，正文 16px、2.1 行高 |
| 来源、账户、备份诊断 | 1280/760/390px，无横向溢出 |
| 九页最终文字对比度抽查 | 已渲染、非禁用文字与实色背景最低 5.0:1；不是所有动态状态的完整 WCAG 审计 |
| 键盘焦点 | 链接输入框通过 Tab/Shift+Tab 获得 3px solid 可见轮廓 |
| 减少动态效果 | 实际计算的 transition-duration 为 0s |
| 最终浏览器控制台 | 0 errors、0 warnings |

原始截图在本机 `output/playwright/`，最终九页三尺寸共 27 张；另有五页 1120px 布局测量。测量回执 `.artifacts/ui-extra-report-20261102.json`。这些仅证明受控页面的布局与交互展示，不代替原生窗口、真实账户或平台正文验收。

## 完整检查

统一入口新回执 `.artifacts/ui-checks-20261102/report.json`：14 步全部通过，版本 0.3.7，构建时工作树含本轮变更。Python 3.12.14、Node 24.16.0、Rust 1.96.0。

| 检查 | 结果 |
| --- | --- |
| cargo fmt / clippy（警告视为错误） | 通过 |
| cargo test --workspace --locked --offline | 152 通过、0 失败，原有 5 项显式性能/专项项 ignored |
| npm lint / typecheck | 通过 |
| npm test:frontend | 65 通过、0 失败、1 项原有专项跳过 |
| Python 回归 | 54 通过；日志中的预期失败夹具由测试核对，不是主检查失败 |
| 签名边界测试 | 11 通过；项目本身仍未正式签名 |
| 一体化资源 | 固定清单、1045 文件、离线组件准备通过 |
| CLI release / 原创样本 | 0.3.7 原创导入、TXT/EPUB 验证通过 |
| 前端与 NSIS release | 通过 |
| 新 GUI 原生自检 | 原创两章、自动 TXT、删除后恢复、WebView2 与必要检查通过 |

前端 JS 208.48kB、CSS 34.71kB（gzip 69.82/7.28kB），相较 0.3.6 的 JS/CSS 合计增加 9.76kB，gzip 合计增加约 2.53kB。未加入远程字体、图片请求或动画库。

最初无头浏览器在沙箱系统 Temp 写入失败，改用独立浏览器与项目临时目录后通过；隔离预览最初重复导入 Vue runtime，统一为同一个 Vite 依赖后警告消除。版本助手只修改三个 OpenNovel crate，保留同为 0.3.6 的无关依赖。快捷方式更新最初因 Windows PowerShell 默认编码无法解析中文 JSON，在写入前停止；显式 UTF-8 后成功。未以放宽断言替代修复。

## 本机交付与保护

独立目录 `.artifacts/ui-delivery-20261102`，GUI 与 CLI 分处根目录和 `cli/`，避免 Windows 文件名大小写不敏感导致相互覆盖。最终复制的 GUI 再次在不同工作目录、仅 System32 PATH、无组件覆盖环境变量条件下完成原创安装自检。

| 文件 | 字节 | SHA-256 |
| --- | ---: | --- |
| `OpenNovel.exe` | 17,757,184 | `de8d9d8ec2428c6b47de6d8b518ad882784f2fde57a3d62e2dec177d66f0a87e` |
| `cli/opennovel.exe` | 8,034,304 | `b17b0c8a634c2e8bb2f88b3f67497582fdcec0ed6d446579e8253e6f98da9a78` |
| `OpenNovel Downloader_0.3.7_x64-setup.exe` | 96,713,419 | `7a679958ed5eafcce80cd05c088bb1ddf7cd86a7ee120362762d029a6184bcf6` |

原书库只读一致性备份，integrity_check=ok，3 本、1459 章、5 个任务；原数据库前后 SHA-256 均为 `da0e840834ce09463481e72872825e9ce541de491cccd7b4d559089c9e42cf10`。未打开用户原书库执行测试。

桌面唯一匹配的 OpenNovel 快捷方式从 0.3.6 目录更新至本次 GUI，原 `--database` 参数保留；备份、前后回执和受摘要保护的 `restore-desktop-shortcut.ps1` 在新交付目录。旧程序和原始书库保留。只是核对快捷方式字段，未模拟用户双击或原生点击。

本机复制版自检显示组件 installed=true、enabled=false、ready=false，原因 `docker_unavailable`；软件提供启动 Docker Desktop 后重新准备的指引。本轮只做界面升级，不主动启动或更改 Docker。不能把随包组件存在写成服务已就绪。

## 验收边界

本轮完成界面实现、受控视觉检查、回归、打包、自检与可恢复入口更新。未进行真实官网/已购内容测试、原生窗口点击、独立干净 Windows 安装、正式签名或远端 CI；既有外部门禁保持未验收。不自动创建 tag、推送或公开发布。

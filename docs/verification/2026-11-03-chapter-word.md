# 保存前选章与 Word 导出验证

实施日期：2026-11-03；中断后于 2026-11-04 完成交付检查。

## 本次范围

- 添加小说、公开网页和账户目录确认流程，在开始保存前选择全部章节或起止章节、TXT 或 Word、保存位置。
- 起止按目录序号计算，从 1 开始，包含两端；不合法或超过目录总数的范围拒绝创建任务。
- 指定范围复用已有正文，但生成文件只包含所选任务章节。续存、重启和重新生成保留原范围与格式。
- 已保存作品可直接按范围导出 TXT、EPUB 或 DOCX，不需要重新下载。
- DOCX：正文宋体／Times New Roman 小四 12 pt，首行缩进两字，1.5 倍行距；章标题黑体小三 15 pt，居中另起页；作品信息与简介楷体；A4、2.54 cm 页边距、页码。
- 内核独立生成 WordprocessingML，不新增 Office、Python 或在线服务运行依赖。字体未嵌入，其他机器缺少字体时由阅读软件替代。

## 代码与数据

主要改动：`crates/core/src/docx.rs`、`export.rs`、`export_settings.rs`、`storage.rs`；桌面 `SaveOptions.vue`、`saveOptions.ts` 和各保存／导出组件；桌面自动导出服务、IPC、安装自检；CLI 格式选项与回归测试。

书库 schema 10 升至 11，新增自动导出计划。范围、格式、目录与任务一起事务提交；迁移在恢复运行中任务之前执行，迁移失败不会改变原任务状态。升级前保留原库备份，旧程序不能直接打开升级后的新库。安装检查只使用独立原创样本库。

## 自动化结果

| 检查 | 结果 | 本地证据（不提交生成物） |
| --- | --- | --- |
| Rust fmt、clippy | 通过 | `.artifacts/chapter-docx-20261103/final-core/report.json` |
| Rust 全量测试 | 189 通过，0 失败，9 忽略 | `final-core/rust-tests.log` |
| 前端 lint、typecheck、build | 通过 | `final-assets/build-report.json`；最终打包再次执行前端构建 |
| 前端交互与回归 | 90 通过，0 失败，1 跳过 | `final-package/frontend-tests.log` |
| Python 工具测试 | 95 通过 | `checks-final/python-tests.log` |
| CLI 原创 TXT／EPUB／DOCX 导出 | 通过，DOCX ZIP、XML 与正文校验通过 | `final-core/cli-original.log` |
| 签名流程契约与集成资源检查 | 通过；不代表已经正式签名 | `checks-final/report.json` |

本次新增测试覆盖精确范围导出、不改变书库章节数、重启保持选择、删除文件后重建、保护已编辑文件、XML 转义、格式校验、事务失败回滚、schema 10 备份和失败迁移保留运行状态。账户流程测试使用受控 IPC，不请求商业平台或读取账户凭据。

## 界面与排版

- 实际 Vue 组件在无头浏览器中检查 1440 px 和 560 px 宽度，范围／格式在保存按钮之前；窄屏文档宽度 560 px，无横向溢出。截图与测量在 `ui/`，IPC 使用原创数据。
- Microsoft Word 以只读方式打开原创 DOCX，核对宋体／Times New Roman、12 pt、两字缩进、1.5 倍行距、黑体 15 pt；证据为 `word-render/typography.json`。
- Microsoft Word 的 PDF 导出两次超时，已停止专属验证进程，未把它记作成功。改用 WPS 导出 PDF，再用 Poppler 渲染，三页全部逐页检查，无裁切、重叠或异常空页。证据为 `word-render/render-report.json` 和 `page-1.png` 至 `page-3.png`。
- 未使用 Computer Use；无头预览不作为原生窗口交互证明。

## 交付检查

- 最终前端测试与 Windows NSIS 一体化打包通过，安装包约 257.50 MiB，证据为 `final-package/report.json`。普通运行无需 Node、Rust、Python 或 Docker；包含现有独立本地引擎组件及对应源码。
- 交付目录：`.artifacts/chapter-docx-20261103/delivery-final`。从该目录运行 `OpenNovel.exe --check-installation`，环境 PATH 为空，原创 TXT、Word 自动生成、Word 指定章节范围、TXT 删除恢复和保存目录检查全部通过。可选本地引擎检查就绪但保持关闭，未打开用户书库、未请求真实平台；证据在 `native-final/check-*/report.json`。
- 交付文件校验共 1152 个文件，附 `SHA256SUMS.txt`、`DELIVERY.json`；最终提交号与重新计算的清单记录在 `.artifacts/chapter-docx-20261103/final-verification.json`。
- 桌面 `OpenNovel Downloader.lnk` 已指向 `delivery-final/OpenNovel.exe`，保留原书库参数和旧程序。更新前后的用户书库 SHA-256 相同；原快捷方式及恢复脚本保存在不入库的 `private-backup/`。未替用户启动并迁移个人书库。

## 验收边界

本次完善本地保存和排版，不扩大任何平台获取能力。没有执行真实已购全书、普通干净 Windows 安装或原生窗口手工交互验收。平台范围仍以 `docs/sources.md` 为准，不能据此宣称所有平台完整下载、三分钟下载或正式签名已完成。

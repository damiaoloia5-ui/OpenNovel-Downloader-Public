# 0.3.21 本地交付与剩余条件

2026-11-05，基线 79bd830。将已验证的选章导出、无变化刷新和恢复提示优化交付为独立版本 0.3.21。

## 本轮结果

- `.artifacts/delivery-0321-checks/report.json` 的 14 步检查全部通过，包含核心与界面回归、签名契约、一体化资源、构建和原生原创自检。
- 清洁便携目录及对应 ZIP 位于 `.artifacts/delivery-0321/`。复制后的应用在空 PATH 下完成自检，独立引擎就绪，使用原创临时数据。
- 安装程序及供分发的 ZIP 位于 `dist/installers/` 的 0.3.21 版本目录，包含运行依赖、许可与对应源码。安装器 SHA-256 为 `c2aa606e558c6aea821dd61093dc456bcadf95ecd031cabe29aaeb55ae9a5364`。
- 实际解包核对 1059 个文件，其中引擎资源 1048 个，主程序和 WebView2 离线安装器摘要匹配。记录在 `.artifacts/installer-audit-0321/payload-verification.json`。
- 更新签名和实际 EXE 验签通过，待发布资产在 `.artifacts/delivery-0321/signed/`，验签回执为 `verification.json`。
- 桌面原先未发现指向本项目的入口，已创建 `OpenNovel Downloader.lnk` 指向 0.3.21 清洁便携目录。5 个现有用户数据文件前后摘要一致。记录在 `.artifacts/delivery-0321/shortcut-check.json`。完成的是快捷方式读回及目标核验，原生窗口视觉验收仍待执行。
- 正常网络环境下 `.artifacts/security-0321-host/report.json` 审计通过。npm、宿主 Rust 和独立引擎已知漏洞数量均为 0。RustSec 数据库返回 1403 条公告，提交 965bf9ad083b58e6340d65829d85217b18be9290。Windows 依赖链中的 fxhash 维护状态告警继续跟踪。

## 历史待办复核

上一轮对目录修订的描述过宽。`accept_catalog_update` 已在事务中保留旧快照、移出删除章节、按稳定 ID 更新位置及标题。既有 `reviewed_reorder_removal_and_insertion_keep_old_body_and_job_exports` 回归本轮通过。用户可从作品详情进入“目录变更与历史版本”，预览后确认应用。自动补章入口遇到结构变化仍要求先完成这次确认。

无变化刷新优化已交付。完整事件推送、全格式流式导出属于后续优化路线，当前实现采用版本检查及轮询恢复。

## 仍未完成的条件

| 项目 | 当前结果与完成条件 |
| --- | --- |
| 独立 Windows 跨版本更新 | 当前机器缺少 System32 下可直接启动的 WindowsSandbox 入口。需要可用的一次性 Windows 环境完成安装、升级、重启及故障注入验收。 |
| 实际已购全书 | 需要本人有效会话和对应作品权益，逐平台实测完整目录、正文和导出。已有账户队列与受控回归结果继续保留。 |
| 七猫 App、咪咕全文、真正图片正文 | 完整通道仍需研究与实现，当前版本能力以 docs/sources.md 为准。 |
| Windows Authenticode | 需要开发者身份核验或开源签名服务审核通过。现有更新签名已经可用。 |
| 远端交付 | 当前完成本地安装包与更新资产。仓库 push、标签及公开 Release 尚待发布操作。 |

此记录保留尚未完成项，便于后续继续实施。当前进度尚不满足“所有平台全书、全部环境验收完成”的目标。

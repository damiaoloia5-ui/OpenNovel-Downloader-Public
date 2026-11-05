# 0.3.22 咪咕免费正文通道交付

2026-11-05，源码基线 b5e794c。

## 实现与真实验收

独立实现咪咕官方手机详情、完整分页目录与明确免费正文服务。普通作品链接进入 GUI/CLI 共用队列，已存正文支持继续保存，自动 TXT/Word 沿用已有格式与位置。之前完成的当前页 Word 导出也随本版交付。

样本作品 384034435514，完整目录 462 项、明确免费 91 项。首次正文任务因正常小字号误判为隐藏文字停止，修正 CSS 零值判断后继续原任务，91/91 项保存成功，正文阶段 47.86 秒。TXT 与 Word 各核对 9112 段、259809 个正文字符的顺序与内容，最终导出范围为 91/462。

来源与完整性校验包括作品/章节 ID、连续目录顺序、分页总数、费用状态、隐藏标记、付费跳转、可识别 HTML 标签、段落外文本和未知字形。所有非零费用状态保留受限。HTTP 请求继续使用共用节流、超时、冷却及域名限制。

详细契约及原始证据索引见 [研究记录](../research/migu-public-chapters.md)。真实数据仅保留在忽略的隔离验收目录。

## 工程与交付

- `.artifacts/delivery-0322-checks/report.json` 的 14 步检查通过。包含 fmt/clippy、Rust、lint/typecheck、前端、Python、签名契约、CLI、桌面一体化构建和原创原生自检。
- 前端 101 项通过、1 项既有跳过。最后增加未知字形保护后，咪咕 3 项回归及 workspace clippy 再次通过。实际样本另确认未知字形数为 0，最终 CLI 同步重建。
- `.artifacts/delivery-0322/delivery-check.json` 通过，复制后应用在空 PATH 下原生自检、组件就绪和更新签名验证通过。
- `.artifacts/installer-audit-0322/payload-verification.json` 通过，实际解包 1059 文件核验，无额外负载。
- `.artifacts/delivery-0322/shortcut-check.json` 记录桌面入口由 0.3.21 更新为 0.3.22，原快捷方式已备份，现有用户数据摘要核验通过。
- 本地安装包 `dist/installers/OpenNovel-0.3.22-Windows-x64/OpenNovel-0.3.22-Windows-x64-setup.exe`，SHA-256 `e2c728b3b4b6eb19e189b42615fddca35236c8b9f520a3046585db3a96ff2c83`。

## 安全审计与未完成范围

npm 和宿主 Rust 联网审计通过。引擎阶段拉取公告发生 GitHub 网络错误，原失败记录在 `.artifacts/security-0322-host/`。改用本轮已核对的干净官方 RustSec 提交 965bf9ad083b58e6340d65829d85217b18be9290 后，宿主和独立引擎均报告 0 条已知漏洞，记录在 `.artifacts/security-0322-pinned/report.json`。保留 fxhash 维护状态告警。

本轮成功范围为咪咕明确免费的普通文字。咪咕已购/App 专属、图片正文和其他作品布局仍需验证。网页页数/标记与导出对照证明当前契约下的完整读取，逐字校勘及原生窗口视觉验收仍待执行。

七猫移动链接本轮重定向回官网网页，未带授权的 App 详情请求返回 401。该通道尚未接入，响应统计为 `.artifacts/channel-research-20261105/qimao-public-probe.json`。本轮没有使用第三方解析服务、账号 Cookie、会员声明或候选项目代码。

Windows Authenticode、独立系统跨版本升级、真实已购全书和公开 GitHub 发布仍按现有条件单独验收。当前完成本地交付，未执行 push 或创建 tag。

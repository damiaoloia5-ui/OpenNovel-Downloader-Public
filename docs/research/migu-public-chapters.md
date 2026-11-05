# 咪咕官网免费章节读取契约

2026-11-05。独立实现于 `crates/core/src/sources/migu.rs`，以官方页面部署代码和实际匿名响应为证据。

## 发现

官方手机详情 `/nap/p/wap_detail_new.jsp?bid=…` 的内嵌 JSON `pageData` 包含作品、作者、简介、完整章节总数及首段目录。`wap_reading_page` 组件使用 `feeStatus == 0` 选择免费章，再向官方 `wechatForChapterContentAction.json` 请求正文。应用独立解析 JSON，不执行远端脚本。

分页目录使用 `/nap/p/catalogForWap.jsp`，读取 `bkdetail_catalog` 的 `chapter`。仅重建 bid/start/count/order/vt 参数，逐页核对总数、连续位置、作品路径、章节 ID，完整读取后才创建任务。费用字段必须存在，所有非零费用状态、隐藏或提前阅读标记均保留为受限。

正文服务位于 `n.cmread.com/nap/f/ms.content.wechatService/wechatForChapterContentAction.json`。请求参数为 bid/cid/pageNo，使用匿名 HTTPS。正文响应必须成功、可见、无付费跳转、无隐藏标志，pageActionUrl 必须绑定原作品和章节。逐页核对 currPage/totalPage，整章完成后交给既有事务保存。图片、未知结构、段落外遗漏文本及权限提示会停止读取。

官网返回的小字号 `font-size:0.75em` 在首轮被过宽的零值前缀检测误判，已改为 CSS 数值检查，并用原创用例覆盖小字号、半透明、真正零字号和隐藏段落。

## 实际证据

作品 384034435514 的完整目录读取到 462 项，其中 91 项明确免费。初次任务在版权页因字号判断失败，修复后继续同一任务成功保存 91/91 项，正文阶段 47.86 秒，零失败。全目录首次完整检查约 32 秒。

已保存正文共 259809 个字符。TXT/Word 各核对 91 项、9112 个段落的内容与顺序，Word ZIP 和 XML 可读。导出结果明确为部分 91/462。未请求其余受限项，无账号、Cookie、会员声明或第三方正文服务。

证据保存在忽略目录 `.artifacts/channel-research-20261105/`，包括原失败、续存回执和 `acceptance.json`。商业正文与网页样本不提交仓库，普通测试只使用原创数据。

当前证明范围为上述作品在本次请求时明确免费的章节。已购章节、图片正文、其他作品布局和完整 App 通道仍需独立验证。

官方入口 [作品手机详情](https://n.cmread.com/nap/p/wap_detail_new.jsp?bid=384034435514)。

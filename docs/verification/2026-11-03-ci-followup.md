# CI 修复与公开样本补验

日期：2026-11-03。开发基线 `84ca516d5a0f652fa5212913901adfeb242c602a`，应用版本仍为 0.3.19。本轮修改验证工具和 CI，不改变小说下载实现、用户书库、桌面快捷方式或既有一体化交付。

## 远端失败与修复

[GitHub 运行 41588843143](https://github.com/damiaoloia5-ui/OpenNovel-Downloader/actions/runs/41588843143) 的 Windows 检查停在签名契约。测试依赖 `.artifacts/v03-release/OpenNovel-Downloader-0.3.0-windows-x64-setup.exe`，该历史文件不会出现在全新检出中。远端只上传报告、未上传逐项日志；本轮补充日志上传。

将四个签名脚本复制到不含历史产物的独立目录，修复前实际报“requires the preserved v0.3 unsigned candidate”，退出 1。修复后测试用 Rust 编译原创未签名 PE，再验证拒绝签名、拒绝错误发布者/篡改、预检失败不改动文件等契约：11 项通过，退出 0。不安装证书、不使用私钥、不放宽正式签名校验。

本地证据保存在 `.artifacts/ci-followup-20261103/signing-red/result.log` 和 `signing-green/result.log`。日志及二进制均不提交 Git。

## 本地回归

`python scripts/verify_project.py --package basic --native --offline --output .artifacts/ci-followup-20261103/basic-regression` 的 13 项检查全部通过：

- Rust fmt、Clippy 通过；Rust 180 通过、9 个明确忽略的外部/手动测试。
- 前端 lint、typecheck、build 通过；测试 78 通过、1 跳过。
- 初次统一执行 Python 80 项通过；加入安装验证工具后，完整 Python 套件另跑 86 项通过，其中新增 6 项检查本机/自托管拒绝、已有安装拒绝、路径边界、NSIS 参数与数据修改/丢失检测。
- 签名契约、release CLI、原创两章 TXT/EPUB、桌面基础安装包构建、原生原创内容自检全部通过。原生自检包含删除自己生成的 TXT 后恢复，不操作用户书库。

基础安装器为 4,983,907 字节，SHA-256 `eee9429546b30c6971bcc1645aeddfc643cb030db179954f561195974a78a15b`，位于 `target/release/bundle/nsis/OpenNovel Downloader_0.3.19_x64-setup.exe`。这是本次基础包，未签名、不含可选引擎；没有替换 `.artifacts/security-delivery-20261103` 中既有一体化交付。

统一报告记录基线提交及 `source_dirty=true`，不能把本地修复通过称为远端 CI 已通过。本轮未修改依赖，未重新宣称安全审计覆盖日期或安全认证；此前审计边界见[发布前安全验收](2026-11-03-release-security.md)。

## 安装生命周期检查

新增 `scripts/verify_installer.py` 并接入 CI：只允许一次性 GitHub 托管 Windows runner，在没有既有安装登记、应用数据时，创建原创书库与 TXT，安装到受限新目录，核对 EXE/许可及原生自检，同版本重装，再卸载并核对原始数据摘要。它不递归删除目录、不提供本机绕过参数、不请求小说平台。

保护与数据丢失检测测试最初为 6 项，随后补到 8 项；**安装/重装/卸载链路已在托管 runner 实际通过**，最终证据见本文末尾。该结果仅是托管 Windows Server 上的基础包验收，不等同于普通用户干净 Windows、WebView2 缺失离线安装、一体化组件或真实窗口交互验收。

## 书旗公开来源

同一 CLI，独立书库，不使用账户。保留第一次限流及冷却后失败的记录。

| 作品 ID | 本轮结果 |
| --- | --- |
| 9147568 | 目录 11 项；保存第 1、6、11 章，3 保存、0 受限、0 失败；TXT 全部段落与 EPUB ZIP 通过，2.42 秒 |
| 9127028 | 第一次目录返回限流、要求等待至少 15 秒；冷却后独立重试仍限流、要求至少 59 秒，停止继续请求，未验收正文 |

证据：`.artifacts/ci-followup-20261103/shuqi-public/` 和 `shuqi-after-cooldown/`。第一本是抽样而非全书证明；不能将第二本限流解释为需要 VIP，也不将本轮报告改写为全来源通过。其余五个平台已通过的前轮公开样本范围不因此扩大。

## 剩余条件

远端修复复验和托管安装链路已完成；普通独立 Windows/缺失 WebView2、一体化安装、真实已购账户仍分别保留。本轮不能标记“全部平台与正式发布已完结”。

## 授权后的首次远端结果与校验修正

用户明确授权 push 并继续远端验收，已推送 `e6236e47bc1a9252991865512b9392edcd3991e0` 并核对远端 main 摘要。[运行 41614544170](https://github.com/damiaoloia5-ui/OpenNovel-Downloader/actions/runs/41614544170) 的 13 项基础检查通过，依赖审计通过：npm 0 漏洞，Rust 0 漏洞，RustSec 1403 条公告、提交 `6c5fedf8ff03c485ed674d8f941e0180f3f72d70`。fxhash 维护状态警告仍在 Windows 依赖树内；proc-macro-error/glib 警告不在 Windows 树内。此 CI 未扫描独立引擎，不替代此前一体化组件审计。

安装验收在首次安装后的 EXE 摘要核对处停止，没有完成重装/卸载。查明是测试预期错误：[Tauri bundler 2.10.1 源码](https://github.com/tauri-apps/tauri/blob/tauri-bundler-v2.10.1/crates/tauri-bundler/src/bundle.rs) 在打包前将唯一 `__TAURI_BUNDLE_TYPE_VAR_UNK` 改为 `__TAURI_BUNDLE_TYPE_VAR_NSS`，打包后恢复构建目录中的原始 EXE，所以两者不能直接要求原始 SHA-256 相等。

修正为在内存构造唯一规定标记变化后的预期文件，再与已安装文件逐字节比较，记录两者摘要和偏移；不修改安装 EXE，不忽略未知字节变化。新增测试拒绝额外修改、截断、追加数据、未打包标记和缺失/多重标记。安装测试移到昂贵的审计工具编译之前，仍要求所有门禁成功才输出安装包 artifact；开启失败时缓存以减少重复构建。Python 全套本地复验为 88 项，随后执行下面的真实远端复验。

已下载的首次远端报告和全部日志保存在 `.artifacts/ci-followup-20261103/remote-first/`。普通 `rg --files` 会按项目忽略规则隐藏 `.log`，但文件实际已经包含在远端上传的 30 个文件中。现有一体化交付三文件、原用户书库与桌面快捷方式摘要全部未变，记录在 `preserved-delivery.json`。未创建 tag 或发布 Release。

## 最终远端验收与产物核对

代码提交 `1b9737c6e9012d3587bfd486f599672ecc23defc`，工作区干净。[运行 41619957718](https://github.com/damiaoloia5-ui/OpenNovel-Downloader/actions/runs/41619957718) 的基础检查、安装生命周期、依赖审计、报告上传与基础包上传全部通过，最终作业结论 success，耗时 31 分 20 秒。远端环境为 Rust 1.99.1、Node 24.22.2、Python 3.14.10。

- 13 项统一检查全部通过；Rust 180 通过、9 忽略，前端 78 通过、1 跳过，Python 88 通过。外部/手动测试的忽略状态不改变。
- 真实安装与同版本重装都通过文件比对、许可材料和原生原创自检。EXE 只在偏移 14635488 的规定包类型标记处变化；两次已安装 EXE 的 SHA-256 均为 `ad188c80c0469d0d0cee600b8299ef6483bde6863d0a0250cf01acbfb35c02bd`。
- 真实默认卸载后主程序及卸载登记已移除，原创书库和 TXT 摘要仍与安装前一致。未打开用户书库、未请求小说平台、未验证账户。
- npm 与宿主 Rust 审计仍为 0 漏洞；公告快照与上述 1403 条版本相同，维护状态警告保持可见。
- 下载的 CI 基础安装器为 4,938,815 字节，SHA-256 `cd6afd4febe0c5d77f60a48b5f85a1d1ae8506690150f388179c359c8ba15578`，与安装验收报告中的安装器摘要完全一致。该包不含可选番茄组件，不替换既有一体化候选包。

原始报告与全部步骤日志：`.artifacts/ci-followup-20261103/remote-final/`；下载的基础包：`remote-basic-package/target/release/bundle/nsis/`；汇总核对：`final-verification.json`。下载和核对不等于已发布 GitHub Release。

## 补充的一体化引擎审计

本地再次直连 RustSec 失败，错误保留在 `integrated-audit/`，该次报告为失败。随后通过 Git 获取与远端相同的官方提交 `6c5fedf8ff03c485ed674d8f941e0180f3f72d70`，核对缓存工作区干净，并显式使用固定快照模式重新扫描：宿主 0 漏洞、独立引擎 0 漏洞且 0 警告、npm 0 漏洞。报告明确为 `explicit-pinned-snapshot-not-latest`，不把失败联网改写为成功。

实际分发源码 SHA-256 为 `c3a5588fb2b0c6d402de076de10cfd6114fc2690eda72f3b3957e74bf505e813`，内部锁文件摘要为 `575e4b5e4632f89bf7c1deba9741dd3beddf6cf0d9836efecf1ec70ef3d954bd`。证据在 `integrated-audit-snapshot/report.json`。这是依赖公告匹配和权限静态检查，不是安全认证，也不能代替普通干净 Windows 和真实账户验收。

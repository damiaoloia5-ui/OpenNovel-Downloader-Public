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

本地已验证的是 6 项保护与数据丢失检测测试；**安装/重装/卸载链路尚未在托管 runner 实际执行**。新的远端运行须推送修复提交后验证。即使通过，也仅是托管 Windows Server 上的基础包验收，不等同于普通用户干净 Windows、WebView2 缺失离线安装、一体化组件或真实窗口交互验收。

## 书旗公开来源

同一 CLI，独立书库，不使用账户。保留第一次限流及冷却后失败的记录。

| 作品 ID | 本轮结果 |
| --- | --- |
| 9147568 | 目录 11 项；保存第 1、6、11 章，3 保存、0 受限、0 失败；TXT 全部段落与 EPUB ZIP 通过，2.42 秒 |
| 9127028 | 第一次目录返回限流、要求等待至少 15 秒；冷却后独立重试仍限流、要求至少 59 秒，停止继续请求，未验收正文 |

证据：`.artifacts/ci-followup-20261103/shuqi-public/` 和 `shuqi-after-cooldown/`。第一本是抽样而非全书证明；不能将第二本限流解释为需要 VIP，也不将本轮报告改写为全来源通过。其余五个平台已通过的前轮公开样本范围不因此扩大。

## 剩余条件

远端修复复验、托管安装链路、普通独立 Windows/缺失 WebView2、真实已购账户仍分别保留。没有创建 tag、推送新提交、发布 Release 或办理签名证书。本轮不能标记“全部平台与正式发布已完结”。

# GitHub 下载与历史版本

公开下载入口：[全部版本](https://github.com/damiaoloia5-ui/OpenNovel-Downloader-Public/releases)，
首个发布版本：[v0.3.19](https://github.com/damiaoloia5-ui/OpenNovel-Downloader-Public/releases/tag/v0.3.19)。
原仓库保持私有（本地 `origin`）；公开仓库使用独立 `public` 远端。仅推送已审查且明确获准公开的提交，
不自动镜像后续私有开发。两仓库的 `v0.3.19` 均指向 `5dcbc93`，未来版本使用新标签。

源码用 Git 保存，安装包放 GitHub Releases，不把 EXE、ZIP、书库或构建目录提交进 Git。
Actions artifact 是检查产物，可能到期，不能用作长期下载入口。

## 生成发布目录

0.3.20 起增加签名自动更新，先按 [自动更新发布指南](automatic-updates.md) 准备安装器、签名和 `latest.json`。
归档命令必须带 `--updater <已签名并由实际程序验签的目录>`；旧版本归档命令仍保留用于历史记录。

先完成构建与交付检查，取得同版本的 `release-manifest.json`、`delivery-check.json`、
一体化安装器和便携 ZIP。归档工具校验两份回执、源码提交版本及文件摘要，并从便携包的
固定路径取出许可和引擎对应源码。它不读取书库、不联网、不推送，也不创建 tag。

```powershell
python scripts/prepare_github_release.py --delivery .artifacts/release-hardening-20261104 --output .artifacts/github-releases/v0.3.19
```

输出路径必须不存在；失败保留现场，不覆盖旧目录。仅有完整 `SHA256SUMS.txt` 的归档才能
进入发布审核。输出包含安装器、便携包、固定提交的应用源码 ZIP、完整引擎源码、
许可通知、release.json、发布说明和校验清单。不上传上级 `.artifacts`、原始检查日志或用户数据。
公开安装器统一使用无空格的 `OpenNovel-<版本>-Windows-x64-setup.exe`，避免托管服务改名后
与校验清单不一致；原交付目录及二进制内容不变。
现有工具明确准备未签名、尚未完成稳定验收的一体化版本，不代替 `release_readiness.py`。
正式签名版需使用签名流程和实际验收证据，不得直接改布尔字段来冒充通过。

## 发布与保存

1. 扫描完整源码历史及发布资产，复核公开仓库地址、提交作者信息和许可证；公开会暴露提交历史。
2. 获得项目要求的 tag 授权后，在归档记录的 `source_commit` 上创建对应版本 tag。
   tag 必须精确指向构建对应提交，不能因为 main 前进就改指新提交。
3. 推送已获授权的 tag，使用 `gh release create <tag> --repo <owner/repo> --verify-tag --draft --notes-file <RELEASE_NOTES.md>` 建立草稿。
4. 仅上传归档目录列出的文件，不使用 `--clobber`，不重复利用已有版本号。
   上传后检查资产名称、大小及 GitHub SHA-256；必要时下载重新计算。
5. 复核说明中的签名、来源支持和验收边界后发布草稿。完整稳定发布仍需收尾门禁通过。
6. 新版本创建新 tag 和 Release。保留所有旧 Release、附件和对应源码，不设置自动删除历史包任务。
   应在首次公开发布前考虑启用仓库 Release immutability；它只保护启用后发布的版本。
   启用后先在草稿阶段上传齐全再发布，避免锁定后无法补附件。
7. `.artifacts/github-releases/<版本>/` 为本地副本，建议再复制到独立硬盘或备份服务。
   GitHub 托管和同一块本地磁盘都不构成永久保存保证。

公开仓库的 `/releases` 页面提供所有历史安装包，`/releases/latest` 指向最新发布版。
发布后才能把具体下载链接加入 README；未发布的草稿不能当作用户可下载地址。

官方说明：[Releases](https://docs.github.com/en/repositories/releasing-projects-on-github/about-releases)、
[Release 不可变设置](https://docs.github.com/en/code-security/how-tos/secure-your-supply-chain/establish-provenance-and-integrity/prevent-release-changes)。

# GitHub 自动更新（Windows x64，0.3.20 起）

## 用户操作

打开主界面上方的“软件更新”。默认在启动后检查公开仓库的新版本，也可以取消“启动时检查新版”。
发现新版本后，点击“下载并安装更新”：下载、核对签名、等待任务结束、备份各用户书库、启动安装器。
检查版本不会自动关闭软件或安装；下载可以取消。网络失败时原版本继续使用。

0.3.19 及以前没有这个入口，需要先从公开仓库手动安装带更新功能的新版本一次。
便携版点击更新时需勾选“转为安装版”；原便携目录不删除，不会在原 ZIP 旁直接覆盖文件。
安装版沿用 `org.opennovel.downloader` 标识、当前 Windows 用户安装范围和原有用户数据目录。

安装前暂停下载/保存任务、等待导出完成，并关闭软件内的官网窗口。
若其他 OpenNovel 进程正在使用某个书库，更新会停止，关闭对应进程后再试。
已验签的安装包保留在本次进程的内存中；退出软件后需重新下载，不在磁盘留下未受控安装包缓存。

## 数据保护与恢复

- 安装前通过 SQLite 备份 API 为所有已存在的本地用户书库生成一致快照，并校验数据库和 SHA-256。
- 备份用户列表 `profiles.json`，每个书库独立子目录；全部成功后才写 `complete.json`。
- 备份默认在 `%APPDATA%\org.opennovel.downloader\update-backups\before-update-<唯一标识>`；界面显示实际路径。
- 备份失败、书库占用、任务尚未完成会阻止安装。已下载包可以重试，用户原文件不覆盖。
- 原导出目录、正文、设置和账号记录保留；安装器不删除 WebView2 用户数据。软件原有的官网临时会话仍在关闭时清理，更新后可能需要本人重新登录，不承诺保留登录状态。
- 备份不复制 Cookie、令牌、浏览器缓存或外部导出文件。备份也包含私人书籍信息，不要上传公开仓库。
- 不自动删除历史备份。磁盘容量不足时停止更新，应先将旧备份转存后再重试。

需要恢复时，在“备份与恢复”导入某个用户备份子目录，先恢复到新目录核对，再切换书库；不要直接覆盖现有数据库。
应用已有 schema 升级前备份和拒绝旧版打开新版数据库的机制。降级不能视为无损回滚，优先保留当前数据并恢复独立副本。
这些措施用于降低更新风险，不构成断电、磁盘损坏或未来错误迁移下绝对零丢失的保证。

## 信任边界

清单固定为：
`https://github.com/damiaoloia5-ui/OpenNovel-Downloader-Public/releases/latest/download/latest.json`。
仅接受同一公开仓库下与稳定版本号完全匹配的 Windows x64 EXE。
HTTP、其他仓库、带用户信息的地址、非默认端口和非允许 GitHub CDN 跳转均被拒绝。
清单最大 128 KiB，安装器最大 768 MiB；连接、读取、整个下载均有超时。

安装器使用 Tauri/minisign 更新签名，公钥随程序内置。签名同时绑定文件和版本，拒绝篡改、无签名和降级。
只有在 Rust 后端完成备份及操作互斥后才能调用安装；前端和官网页面没有 updater 插件权限。
清单中的更新说明只作为文字显示。发布站能看到正常连接的 IP 等网络信息，不发送书库、书名、账户、Cookie 或凭据。

更新签名免费，不等于 Windows Authenticode。后者仍未办理，Windows 可能继续显示“未知发布者”。
发布账户被盗但私钥未泄露时，伪造安装包仍无法通过验签；发布者私钥泄露则必须另行制定换钥和人工升级方案。

## 维护者：以后如何发布

1. 使用新版本号，同步 `package.json`、桌面 package、Cargo workspace/lock、Tauri 配置、集成资源目录；完成工程与安装器检查。
2. 用 `npm run desktop:package:integrated` 构建。普通构建和 CI 不接触更新私钥。
3. 在创建签名身份的 Windows 用户下执行下方命令，生成安装包、`.sig`、`latest.json` 及本地验签回执。
4. 组装交付回执，再用 `prepare_github_release.py --updater <签名目录>` 生成公开归档。
5. 完成项目要求的 push/tag 授权，在**新版本草稿 Release** 上传归档列出的全部文件。
6. 核对下载与校验值后发布，设为最新稳定 Release。公开仓库已开启不可变 Release，不修改旧版本或在发布后补附件。

```powershell
python scripts/updater_release.py prepare --installer "target/release/bundle/nsis/OpenNovel Downloader_0.3.20_x64-setup.exe" --privacy-report .artifacts/release/privacy-check.json --verifier target/release/opennovel-desktop.exe --output .artifacts/signed-updates/v0.3.20
python scripts/prepare_github_release.py --delivery <已核验交付目录> --updater .artifacts/signed-updates/v0.3.20 --output .artifacts/github-releases/v0.3.20
```

只上传一个新版 EXE **不足以自动更新**。每个新 Release 必须带有准确的 `latest.json` 和签名，安装包文件名必须为 `OpenNovel-<版本>-Windows-x64-setup.exe`。
没有清单时客户端显示“尚未提供自动更新清单”，不会猜测或安装未签名的附件。

私钥保存在 `.artifacts/private-updater/signing-key.dpapi`，由创建它的 Windows 用户 DPAPI 保护，不提交、不打包、不上传。
公钥 `apps/desktop/src-tauri/updater-public-key.txt` 可以公开，必须和配置一致。
**不要重新执行 init-key 或换公钥来修复普通发布失败**：旧客户端只信任旧公钥。
DPAPI 文件单独复制到另一电脑通常无法解密；应保存原 Windows 用户的可恢复系统备份，并在迁移发布环境前完成安全的密钥迁移演练。
当前工具不提供自动跨机器迁移私钥或云端代签；无法恢复私钥时需要用户手动安装换钥版本。

原生只读验签入口：`opennovel-desktop.exe --verify-update <latest.json> <安装包>`。
退出码 0 表示嵌入公钥验签通过，1 表示拒绝，2 表示参数错误。它不启动安装器，不打开用户书库，不联网。
此入口可以验证当前版本；正常软件更新入口仍只接受较新版本。

本机受控测试不能代替普通用户 Windows 上的真实更新与故障恢复验收。
官方机制参考：[Tauri updater](https://v2.tauri.app/plugin/updater/)。

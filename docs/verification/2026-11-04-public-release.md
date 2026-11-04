# v0.3.19 公开下载发布记录

## 仓库与源码

用户明确授权保留原私有仓库、另建 `OpenNovel-Downloader-Public` 公开仓库，
并创建、推送 `v0.3.19` 标签。此次没有修改原仓库可见性或改写提交历史。

- 公开源码：https://github.com/damiaoloia5-ui/OpenNovel-Downloader-Public
- 公开下载：https://github.com/damiaoloia5-ui/OpenNovel-Downloader-Public/releases/tag/v0.3.19
- 私有备份仍是本地 `origin`；公开仓库为独立 `public` 远端。
- 两个仓库的 `v0.3.19` 均为 `5dcbc93ee415279fe94d97ae7b682047e2ea13fb`。
- 发布前 75 个可达提交 Gitleaks 扫描通过；提交作者邮箱随已授权的完整历史公开。

## 下载资产核验

Release ID `433454769`，共 9 项资产、369620869 字节。先创建草稿、逐项核对
服务器 SHA-256、大小、名称和说明，全部相符后才发布。
发布后使用不带认证头的 urllib 重新下载全部 9 项文件，并与本地逐项核对摘要及字节数，全部通过。
这证明当前匿名下载链路可用，不是只检查草稿、上传成功状态或已登录维护者的权限。

| 下载文件 | SHA-256 |
| --- | --- |
| OpenNovel-0.3.19-Windows-x64-setup.exe | `2215eacaaf9174ccf3027fa96823642b4abd89a94ab21eb0a2ea1d45c7bba5ab` |
| OpenNovel-0.3.19-Windows.zip | `13c92e88af819318db52c4aa1c90fe383eb332004a344bcd810918c3179f89e9` |
| OpenNovel-0.3.19-source.zip | `f56933a63f849cf6fdda349f6979c05f0efc92d79c88e99327221bb1cfc93ffa` |
| OpenNovel-0.3.19-engine-source.tar.gz | `c3a5588fb2b0c6d402de076de10cfd6114fc2690eda72f3b3957e74bf505e813` |

其他五项为 LICENSE.txt、THIRD_PARTY_NOTICES.txt、release.json、RELEASE_NOTES.md、SHA256SUMS.txt。
主程序 Apache-2.0；独立引擎保持 GPL-3.0 与完整对应源码。安装器仅改为公开下载的无空格文件名，
字节与先前验收的一体化交付完全一致；便携包与对应源码也未重建。
本地公开归档位于 `.artifacts/github-publish/v0.3.19`；匿名回下载副本位于
`.artifacts/public-download-v0.3.19`，报告位于 `.artifacts/publication-anonymous-download-report.json`。
这些产物和原始日志均不提交进 Git。

## 历史保留与公开维护

- 仓库 Release immutability 已开启，发布后匿名 API 确认本版 `immutable=true`。
  新版本使用独立 tag / Release，不改写本版标签或附件，也不设置历史自动清理。
- 公开仓库已开启 secret scanning、secret scanning push protection 及 private vulnerability reporting。
  原仓库的可见性与配置不受影响。
- README 提供安装包、便携包与历史版本入口；独立本地归档仍应另备份到其他介质，
  同一块磁盘上的多份副本不是独立灾备，GitHub 托管也不代表永久保存保证。

## 测试范围

下载文件名调整后 6 项发布保护回归通过。原软件提交 `5dcbc93` 与发布工具基线 `bbf05b7`
均已通过原私有仓库远端 CI；包含基础安装、重装、卸载和依赖审计。
公开仓库首次 CI（41762940565）记录时仍在运行；不将私有仓库结果冒充公开仓库重新构建结果。
本轮未修改应用界面或 Rust 运行代码，未重复本地应用 lint、typecheck、build 或真实账号测试。

本包仍未正式代码签名；真实已购全书、独立普通 Windows 和最终对外许可审核仍未完成。
公开下载不等于全平台完整支持或正式稳定验收完成，发布说明及 `release.json` 保留此边界。

# GitHub 推送与发布归档准备

## 已完成

- 用户授权本次推送；现有私有仓库 main 已从 `3e3ce8d` 推进到
  `5dcbc93ee415279fe94d97ae7b682047e2ea13fb`，GitHub API 回读一致。
- Gitleaks 8.30.1 对全部可达历史扫描 73 个提交、约 3.19 MB，未发现规则命中的凭据。
  历史路径检查未发现数据库、日志、密钥或二进制安装包提交；凭据扫描不能证明没有任何隐私。
  提交历史含作者邮箱，公开完整历史会连同公开，未改写原历史。
- 复核当前一体化安装器与便携 ZIP 的长度和 SHA-256；便携 ZIP 的 1055 项 CRC 通过，
  未发现书库、导出目录或账户 Cookie 文件。安装器内容范围仍依据原构建清单与已记录校验，
  本次没有重新解包安装器或执行真人账号验收。
- 增加 `scripts/prepare_github_release.py`：固定文件归档、与源码提交版本核对、引擎对应源码
  摘要核验、固定提交源码 ZIP、许可、发布说明及 SHA256SUMS；拒绝覆盖已有输出。
  工具不联网、不改仓库可见性、不创建 tag、不发布，不收集同目录额外的书库与日志。
- `docs/github-releases.md` 记录每版本独立 Release、草稿审核、附件校验、不可变设置及独立备份建议。

## 本地交付

新归档位于 `.artifacts/github-releases/v0.3.19/`，源码固定 `5dcbc93`，
不是后续发布文档提交的重新构建版本。文件没有改变原交付二进制：

| 文件 | SHA-256 |
| --- | --- |
| OpenNovel Downloader_0.3.19_x64-setup.exe | `2215eacaaf9174ccf3027fa96823642b4abd89a94ab21eb0a2ea1d45c7bba5ab` |
| OpenNovel-0.3.19-Windows.zip | `13c92e88af819318db52c4aa1c90fe383eb332004a344bcd810918c3179f89e9` |
| OpenNovel-0.3.19-source.zip | `f56933a63f849cf6fdda349f6979c05f0efc92d79c88e99327221bb1cfc93ffa` |
| OpenNovel-0.3.19-engine-source.tar.gz | `c3a5588fb2b0c6d402de076de10cfd6114fc2690eda72f3b3957e74bf505e813` |

## 验证与边界

归档保护 6 项测试通过：正常归档与逐文件校验、额外私有文件排除、旧目录保护、
被修改安装器拒绝、额外回执路径拒绝、版本及引擎源码不匹配拒绝（部分在同一测试中核验）。
首次测试因受限执行环境临时目录权限失败，正常主机环境复验通过。
全量 `python -W error -m unittest discover -s scripts/research -p test_*.py`：108 项通过。
应用业务代码未改动，不重新构建或把旧运行结果写成本次新运行结果。

`5dcbc93` 远端 CI：https://github.com/damiaoloia5-ui/OpenNovel-Downloader/actions/runs/41755176674 。
现已成功，下载报告复核 `source_commit=5dcbc93` 且工作区干净；基础包安装、重装、
空 PATH 自检、卸载后多用户书库/导出/注册表及节流数据保留均通过。
npm 和主程序 Rust 审计为 0 已知漏洞，仍有 fxhash 不再维护告警；此 CI 没有扫描独立引擎，
不能替代一体化组件审计或独立普通用户系统验收。
公开仓库目的地和 tag 授权尚待用户选择，本记录不代表已经公开源码或存在可下载 Release。
未签名、独立普通 Windows、真实已购全书及最终对外许可审核边界仍保留，
归档 `ready_for_stable_release=false`，没有绕过发布就绪门禁。

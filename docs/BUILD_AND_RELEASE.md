# 构建、检查与交付

基础包和一体化包使用同一主程序。基础包不含番茄可选组件；一体化包含固定 Python、独立 GPL Windows 原生引擎及对应源码和 WebView2 离线安装器。普通 CI 构建基础包，不读取账号、不请求商业小说平台，也不下载候选引擎。

## 前置环境

Windows x64、Rust >=1.96、Node >=22.12、Python >=3.11、Visual Studio C++ 工具链、Windows SDK、WebView2。首次构建需联网取得 npm/Cargo/NSIS 等正常构建依赖。运行应用不需要这些开发工具。

```powershell
npm ci
python scripts/verify_project.py --package basic --native
```

完整检查按顺序运行 fmt、Clippy、Rust、lint、类型、前端测试、Python、前端构建、Windows 签名契约、release CLI 构建与原创 TXT/EPUB、桌面打包及原生原创样本检查。任一步失败即停止并返回非零；报告和逐项日志保存在全新的 `.artifacts/checks/<时间和随机标识>/`。可用 `--output` 指定一个不存在的目录，拒绝覆盖既有证据。

Windows 签名契约使用当前 Rust 工具链生成原创未签名 EXE，检查无证书拒绝、文件不被修改、错误发布者与篡改签名拒绝，不需要历史安装包，也不执行真实项目签名。Microsoft SignTool 和 Windows SDK 仍是显式前置条件。

GitHub 托管 Windows runner 在基础检查和安全审计通过后，运行 `python scripts/verify_installer.py`。它只允许一次性托管 runner，拒绝本机、自托管 runner、已有 OpenNovel 安装登记或应用数据；在新目录执行静默安装、同版本重装、默认卸载，并核对安装 EXE/许可摘要、原生原创自检、卸载登记移除，以及原创书库/TXT 保留。此过程会写安装登记和一次性测试数据，所以不提供绕过本机限制的参数。失败保留报告与日志，不强制清理；CI 上传检查日志，不上传书库。未在远端实际通过前只算工具已实现。托管 runner 带有开发工具与运行时，不能替代普通干净 Windows、WebView2 缺失分支或交互窗口验收。

Git 检出会记录实际提交和脏状态；不含 `.git` 的源码 ZIP 会明确记录来源元数据未知，不推断干净、不自动初始化仓库。发布时应保留源码 ZIP 与交付回执中的提交和摘要对应关系。

`--offline` 仅限制 Cargo 使用已有缓存，不能把整个构建称为离线。检查不自动安装缺失开发依赖；缺项以失败明确报告。原生自检会在独立样本目录生成并删除一次自己创建的原创 TXT，验证恢复，不打开用户书库。

## 来源兼容巡检（维护者手动启用）

普通 CI 不请求小说平台。维护者需要检查当前官网变化时，先构建 CLI，再显式运行：

```powershell
python scripts/verify_sources.py --live --executable "D:\OpenNovel\target\release\opennovel.exe" --output "D:\OpenNovel\.artifacts\source-check-new"
```

请替换为实际绝对路径；输出目录必须尚不存在。固定样本覆盖番茄、七猫、晋江、纵横、17K、书旗，每来源两部作品。保存最多三个开放章及一个限制项，核对实际 TXT 全部已存段落和 EPUB ZIP；书旗同样只读取明确免费的章节。同来源出现异常后不继续请求下一本。另检查起点、书旗、刺猬猫、飞卢、咪咕的官方入口 HTTP 状态。报告不保存会话 Cookie、响应头或登录信息，不执行登录、购买或验证码操作。

`public_checks_passed` 只说明本次公开样本通过；`entry_checks_completed` 只说明入口探测完成（可能返回验证），二者均不证明全站、账户权益或全书能力。番茄本地引擎另用显式原生验收。样本变化时由维护者核对官网再调整固定列表，不能把失效样本直接当平台全部失效。报告和正文仅留本地，不提交 Git。普通用户无需安装 Python 或运行此脚本。

## 一体化包的显式材料

需要与 `scripts/research/native_engine_lock.json` 匹配的三个本地文件：

- `manifest.json`
- `engine.exe`
- `corresponding-source.tar.gz`

它们可来自维护者提供的固定离线分发，或已验证一体化包的 `opennovel-engine/native-engine` 目录。脚本只接收本地目录，不接受解析接口或任意下载 URL。没有匹配材料时明确失败，不回退开发卷，也不把不同版本材料拼在一起。

另需 `third_party/python-embed-lock.json` 固定的官方 Python ZIP。维护者可显式取得并检查：

```powershell
python scripts/prepare_embedded_python.py --download
```

然后运行（路径替换为实际材料目录）：

```powershell
python scripts/verify_project.py --package integrated --native --distribution "D:\release-inputs\native-engine"
```

若 Python ZIP 不在默认缓存，可附加 `--python-archive "D:\release-inputs\python-embed.zip"`。这不改变固定摘要和版本约束，不允许任意 Python 替代。单独准备组件：

```powershell
python scripts/prepare_integrated_component.py --distribution "D:\release-inputs\native-engine" --python-archive "D:\release-inputs\python-embed.zip"
npm run desktop:package:integrated
```

准备脚本从摘要匹配的源码归档恢复已审查的上游文件，并按 `third_party/local-engine-source-lock.json` 的 993 项身份、大小和摘要复核；不运行上游源码。宿主脚本使用本仓库版本，组件 metadata、许可、清单和所有输出文件一并校验。无需 `.artifacts/local-engine-review-20261101`、旧 Cargo 卷或本机开发镜像。输出为 `.artifacts/integrated-resources-<package.json版本>/opennovel-engine`，Tauri 配置必须指向相同版本。

已经存在的完全相同结果可复核复用，改写、缺失、多余文件或 metadata 不匹配时拒绝覆盖；保留目录用于排查。新版本使用新目录。应用日常启动不执行这些构建或下载流程。

## 引擎与源码重建

对应源码归档包含原始上游、原创 stdio 包装器、228 个 vendored 依赖及许可证和 `REBUILD-NATIVE.py`。解压后按 `REBUILD-NATIVE.md` 使用 Rust 1.96 MSVC 工具链执行离线构建，CRT 静态链接。维护者入口为 `scripts/build_native_engine.py --source <已核验完整源包目录> --output <新目录>`，详见 [原生组件](windows-native-component.md)。

新二进制及归档不自动获得应用信任，必须审查并更新固定清单。历史 `REBUILD.py` / `build_offline_engine.py` 对应旧 Docker 方案，不是 Windows 原生版前置条件。

主程序 Apache-2.0 与引擎 GPL-3.0 分别保留，公开分发前复核完整许可材料。清单、单元测试和成功构建不是法律或安全认证。

## 交付检查

依赖和权限检查按 [SECURITY.md](../SECURITY.md) 执行；CI 基础包审计不能替代一体化内置引擎审计。安全刷新工具 `scripts/update_native_engine_dependencies.py --source-archive <当前锁定源码归档> --output <新目录>` 保留上游文件，更新独立 workspace 锁定依赖、完整 vendor 和许可材料，离线重建。新 manifest 仍需审查、固定并通过原生沙箱测试，不能直接分发未核验构建。

1. 核对源码提交、版本一致、工作区和 staged diff；正文、凭据、缓存、构建产物不提交。
2. 运行统一检查并保留新回执。`passed=true` 仅覆盖实际列出的命令与原创样本。
3. 将安装器、完整组件、许可、使用指南、SHA-256 和检查报告放入新的交付目录，保留旧版。
4. 备份用户库；新版在独立夹具/副本验证后再让用户使用。不让两个实例写同一书库。
5. 实际安装、窗口操作、账号权益及独立系统验收按[收尾清单](PROJECT_CLOSEOUT.md)执行；未做的分支保持未验收。

CI 配置使用相同基础检查入口；在远端成功运行前不能标为远端 CI 已通过。当前安装器未正式签名。不要自动购买证书、创建 tag、推送或公开发布。

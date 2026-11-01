# 构建、检查与交付

基础包和一体化包使用同一主程序。基础包不含番茄可选组件；一体化包含固定 Python、独立 GPL 引擎镜像及对应源码。普通 CI 构建基础包，不读取账号、不请求商业小说平台，也不下载候选引擎。

## 前置环境

Windows x64、Rust >=1.96、Node >=22.12、Python >=3.11、Visual Studio C++ 工具链、Windows SDK、WebView2。首次构建需联网取得 npm/Cargo/NSIS 等正常构建依赖。运行应用不需要这些开发工具。

```powershell
npm ci
python scripts/verify_project.py --package basic --native
```

完整检查按顺序运行 fmt、Clippy、Rust、lint、类型、前端测试、Python、前端构建、Windows 签名契约、release CLI 构建与原创 TXT/EPUB、桌面打包及原生原创样本检查。任一步失败即停止并返回非零；报告和逐项日志保存在全新的 `.artifacts/checks/<时间和随机标识>/`。可用 `--output` 指定一个不存在的目录，拒绝覆盖既有证据。

Git 检出会记录实际提交和脏状态；不含 `.git` 的源码 ZIP 会明确记录来源元数据未知，不推断干净、不自动初始化仓库。发布时应保留源码 ZIP 与交付回执中的提交和摘要对应关系。

`--offline` 仅限制 Cargo 使用已有缓存，不能把整个构建称为离线。检查不自动安装缺失开发依赖；缺项以失败明确报告。原生自检会在独立样本目录生成并删除一次自己创建的原创 TXT，验证恢复，不打开用户书库。

## 一体化包的显式材料

需要与 `scripts/research/engine_bundle_lock.json` 匹配的三个本地文件：

- `manifest.json`
- `runtime-image.tar.gz`
- `corresponding-source.tar.gz`

它们可来自维护者提供的固定离线分发，或已验证一体化包的 `opennovel-engine/offline-engine` 目录。脚本只接收本地目录，不接受解析接口或任意下载 URL。没有匹配材料时明确失败，不回退开发卷，也不把不同版本材料拼在一起。

另需 `third_party/python-embed-lock.json` 固定的官方 Python ZIP。维护者可显式取得并检查：

```powershell
python scripts/prepare_embedded_python.py --download
```

然后运行（路径替换为实际材料目录）：

```powershell
python scripts/verify_project.py --package integrated --native --distribution "D:\release-inputs\offline-engine"
```

若 Python ZIP 不在默认缓存，可附加 `--python-archive "D:\release-inputs\python-embed.zip"`。这不改变固定摘要和版本约束，不允许任意 Python 替代。单独准备组件：

```powershell
python scripts/prepare_integrated_component.py --distribution "D:\release-inputs\offline-engine" --python-archive "D:\release-inputs\python-embed.zip"
npm run desktop:package:integrated
```

准备脚本从摘要匹配的源码归档恢复已审查的上游文件，并按 `third_party/local-engine-source-lock.json` 的 993 项身份、大小和摘要复核；不运行上游源码。宿主脚本使用本仓库版本，组件 metadata、许可、清单和所有输出文件一并校验。无需 `.artifacts/local-engine-review-20261101`、旧 Cargo 卷或本机开发镜像。输出为 `.artifacts/integrated-resources-<package.json版本>/opennovel-engine`，Tauri 配置必须指向相同版本。

已经存在的完全相同结果可复核复用，改写、缺失、多余文件或 metadata 不匹配时拒绝覆盖；保留目录用于排查。新版本使用新目录。应用日常启动不执行这些构建或下载流程。

## 引擎与源码重建

对应源码归档保留独立 `REBUILD.py`、固定上游、原创封装、228 个 vendored 依赖及许可证、锁文件和无网 Cargo 配置。在独立目录提取并阅读 `REBUILD.md`，准备其中固定的官方构建/基础镜像，再运行 `python REBUILD.py`。重建使用独立卷、只读源码、非 root 和无网构建，保留源码。

重建得到的新镜像和归档不自动获得应用信任。必须另行审查、更新固定清单和相应验收，才能进入下一组件版本。`scripts/build_offline_engine.py` 及其他 research runner 保留历史审查上下文，不是贡献者日常应用构建前置步骤。

主程序 Apache-2.0 与引擎 GPL-3.0 分别保留，公开分发前复核完整许可材料。清单、单元测试和成功构建不是法律或安全认证。

## 交付检查

1. 核对源码提交、版本一致、工作区和 staged diff；正文、凭据、缓存、构建产物不提交。
2. 运行统一检查并保留新回执。`passed=true` 仅覆盖实际列出的命令与原创样本。
3. 将安装器、完整组件、许可、使用指南、SHA-256 和检查报告放入新的交付目录，保留旧版。
4. 备份用户库；新版在独立夹具/副本验证后再让用户使用。不让两个实例写同一书库。
5. 实际安装、窗口操作、账号权益及独立系统验收按[收尾清单](PROJECT_CLOSEOUT.md)执行；未做的分支保持未验收。

CI 配置使用相同基础检查入口；在远端成功运行前不能标为远端 CI 已通过。当前安装器未正式签名。不要自动购买证书、创建 tag、推送或公开发布。

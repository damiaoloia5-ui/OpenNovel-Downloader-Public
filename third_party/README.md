# Dependency license supplements

`license-overrides.json` records original license text omitted from some published
Rust crate archives. Every supplement includes its retrieval URL and SHA-256;
the release generator checks these hashes before packaging. Upstream GitHub files
are stored without Git newline conversion so hashes also match on Windows. Files
are pinned to the crate's `.cargo_vcs_info.json` commit. For `fxhash` and `mac`,
whose manifests declare dual Apache/MIT licensing, the Apache-2.0 option is used.
`selectors` uses the canonical Mozilla MPL-2.0 text. Dependencies keep their
licenses; the project's Apache-2.0 choice does not relicense them.

Run `npm run release:notices` after installing the locked dependencies. Generated
notices list exact crate versions and source download locations, including MPL
sources. Ordinary builds do not fetch license files from upstream. Dependency
updates that omit license text fail this step until a reviewed supplement exists.

The generated inventory intentionally includes build/test crates and may include
dependencies not linked into the final Windows executable. It is a notice
inventory, not a legal opinion or a claim that every listed crate ships as code.
# 一体化版的 Python 运行时

Windows x64 一体化候选版附带 PSF 官方 Python 3.14.10 嵌入包，固定下载地址和 SHA-256 见 [python-embed-lock.json](python-embed-lock.json)。发布页为 https://www.python.org/downloads/release/python-31410/ 。保留官方包的全部 37 个文件及 `python/LICENSE.txt`；只修改 `python314._pth` 为固定的压缩标准库、运行时目录和相邻已校验脚本目录，不加载 site、用户包或环境变量导入路径，不附带 pip。

维护者显式下载并校验，应用启动不会下载或安装 Python。应用编译时固定逐文件哈希，发现改写、多余文件或子目录则拒绝运行。此举验证版本字节一致性，不构成整个系统的全面安全审计。历史 v0.3.2 未包含候选 Linux 引擎二进制；v0.3.3 的独立组件交付见下节。不能把 Apache-2.0 主项目许可证用于重新许可第三方组件。

## 离线引擎组件

v0.3.3 一体化版随附独立 GPL-3.0 引擎镜像及完整对应源码归档。组件与主程序通过受限私有协议通信，不链接进桌面主程序；各自保留许可证，不将第三方代码改称 Apache-2.0。固定输入、文件摘要及镜像身份见 `scripts/research/engine_bundle_lock.json`，对应源码位于交付目录 `opennovel-engine/offline-engine/corresponding-source.tar.gz`。

源码包包含固定上游提交、原创封装、全部 228 个锁定注册表依赖及各自许可、Cargo 锁文件、离线构建配置、重建工具与说明。缺少元数据的 allo-isolate 已核对随附 Apache-2.0 文件。构建实际使用只读源码、全能力丢弃、非 root、无网容器；主程序与组件的来源和能力验证不能替代正式对外发布时的完整许可及系统验收。

运行镜像基于固定 Python Linux 镜像，保留其原有文件与通知；额外只加入独立引擎及许可清单。维护者工具不从用户书库、会话目录或 Cargo 凭据收集任何分发材料。应用只导入程序固定摘要匹配的本地镜像，不下载替代解析服务，不安装 Docker，也不清理用户镜像或卷。

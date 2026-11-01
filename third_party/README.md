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

维护者显式下载并校验，应用启动不会下载或安装 Python。应用编译时固定逐文件哈希，发现改写、多余文件或子目录则拒绝运行。此举验证版本字节一致性，不构成整个系统或 Docker 构建卷的全面安全审计。候选 Linux 引擎不包含在安装包；其独立源码及 GPL 许可证仍随组件保留，不能把 Apache-2.0 主项目许可证用于重新许可第三方组件。

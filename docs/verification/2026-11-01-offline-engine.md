# v0.3.3 离线引擎集成与交付验证

日期：2026-11-01。修改前基线 `59ca5f98dca53fd0031dc5a0619da7daf852057b`，工作区干净，HEAD 为回退点。图工具返回项目未索引后读取源码定位。不使用 Computer Use，不推送、不创建 tag。本记录只覆盖下列实际结果，不代表全部平台或发布门禁完成。

## 本轮变化

- 随安装包交付固定 Python、9 项宿主运行文件、独立引擎 Docker 镜像及完整对应源码。主程序仍为 Apache-2.0，引擎以独立 GPL-3.0 组件运行；不链接进主程序。
- 首次主动启用，或用户此前已主动启用后明确继续保存时，验证本版本内置清单、文件大小和 SHA-256，再从本地归档导入固定镜像。只读状态检查不导入镜像；失败明确提示并保留已存正文，不静默退回开发卷。
- 原组件仍默认关闭，按书库记住用户选择。准备过程中较晚返回的结果不能覆盖用户后来关闭/更改的选择；任务在准备前注册，可取消，不重复启动同一作品。
- 日常运行不再要求开发 Rust/Python 镜像标签或开发 target 卷。依然需要已安装并可用的 Docker Desktop Linux x64 后端；不会安装 Docker、改动 BIOS/虚拟化或停止用户 Docker 服务。明确操作可尝试启动已安装 Docker Desktop。
- 导入被取消后，Docker 后台可能完成镜像缓存，但不开始正文任务；原书库与正文保持。下载能力仍限定明确免费章节及既有来源验收边界，不增加未购买正文或验证码处理能力。

## 构建和材料核对

候选源码固定为 `07cfb2249d4fd57d19bf4420af0e97621ec92df9`，原始源码 993 项完整性保持。`Cargo.lock` 的 228 个 registry 依赖均按锁定摘要验证并 vendor；其中 59 个缺失归档由维护者显式联网下载，运行时不下载这些依赖。未发现 Git/私有依赖；依赖许可证字段缺失数为 0，其中一个通过 license-file 声明的 Apache-2.0 文本单独检查。

仅导出请求的公共 crate 归档，不导出开发缓存配置或凭据。归档提取拒绝路径穿越、绝对路径、链接和设备条目，并限制体积/数量。对应源码包包括原始源码、宿主自编封装、锁文件、完整 vendor、许可证清单、构建配置及独立 `REBUILD.py`。这些是本轮工程检查，不是正式法律或安全认证。

新建本项目专属构建卷 `opennovel-offline-engine-20261101-target`。构建使用非 root、只读源码、无网络、去除 capabilities、受限资源；仅空 target 初始化以 root 调整其所有权，未挂载候选源码。全新离线 release 构建通过，第二次构建二进制摘要相同。运行镜像无需挂载该构建卷。旧镜像和旧开发卷保留。

| 项目 | 固定值 |
| --- | --- |
| 引擎镜像 | `sha256:476605c1339d5abd9ceab90c681aea4246243a030593bd00c213b26e728c7f8c` |
| 引擎二进制 SHA-256 | `c5d55a281113b028dd2f0666b6f39f9c99d441597c96eac498016516bcab6494` |
| 镜像归档字节 | 44,547,647 |
| 镜像归档 SHA-256 | `fb3f95a9747bdf21f8ca92ad8544b265a7613af154a74d6edf251784977a72d8` |
| 对应源码归档字节 | 33,737,004 |
| 对应源码归档 SHA-256 | `f04eaa823a359eeff3fe9a910965025fd4bf1574c696158ee4a3056f60b9fca0` |

归档内 OCI blob 摘要、配置/层/索引引用闭包均核对，无外部 descriptor URL。Docker 从本地 tar/gzip 导入的接口依据为 [Docker image load 官方文档](https://docs.docker.com/reference/cli/docker/image/load/)。不把本地导入称为整个下载过程离线，实际免费正文仍需访问平台。

## 验证结果

| 检查 | 本轮实际结果 |
| --- | --- |
| Rust 格式 | `cargo fmt --all --check` 通过 |
| Rust lint | workspace/all-targets Clippy，`-D warnings` 通过 |
| Rust 测试 | 148 通过，0 失败，5 忽略；忽略项需显式外部/原生条件 |
| 前端 lint | 0 错误，1 个既有 `OfficialReaderPanel.vue` 格式警告 |
| 前端类型/构建 | typecheck、build 通过 |
| 前端测试 | 56 通过，0 失败，1 个性能用例跳过 |
| Python | `-W error`，42 通过；包含源码包和镜像验证/导入失败门禁 |
| 隔离持久会话 | 6 项通过，包括取消、挑战后停止/重建、失效和清理；核对无开发卷挂载 |
| 原生首次准备 | 移除并重导入本项目新镜像，原创两章→核心入库→完整 TXT 通过 |
| 最终 EXE | `--check-installation` 原创完整 TXT 与所有必要项通过 |
| Windows 打包 | EXE、NSIS 生成成功；NSIS 96,707,137 字节，约 92.23 MiB |

原生首次准备前核对新镜像属于本项目且无使用它的容器，仅非强制移除这一镜像；未删除其他镜像/容器/卷。测试去掉引擎和 Python 环境变量，PATH 只含 System32，从不同工作目录运行，模拟曾启用后重启恢复。首次镜像导入、会话及原创 TXT 通过，未请求真实平台或打开原书库。此项不是另一台干净 Windows 验收，也没有实测停止后的 Docker Desktop 冷启动分支；服务启动分支仅受控测试覆盖。

6 项受控容器的证据均显示使用上述新镜像，`development_volume_mounted=false`，清理成功。最终分发的资源总计 1045 项，核对清单及各自摘要，无额外文件。

### 真实免费作品复验

用户授权范围内手动显式运行，不由普通 CI 请求商业平台。从空书库访问 [番茄测试作品](https://fanqienovel.com/page/7672011528131857470)，不使用账号、不导入参考正文：

- 保存前 0 章，本次 102/102 完成，约 **152.65 秒**，自动生成完整 TXT。
- 导出 102 章正文全部与数据库一致；102 章均与上一轮验证结果一致。
- TXT SHA-256：`a7aa56db1dd0fbbd20ff1fe1d9afe6b2e5ffe4a44d5a83959da6bbfea26de23d`。
- 这只证明当时该样本完整通过，不代表番茄全站、所有免费平台或任何付费权益通过。

## 交付和原库保护

本机交付目录 `.artifacts/offline-delivery-20261101`，保留旧版目录。直接运行 EXE 必须与同目录 `opennovel-engine` 资源保持在一起；安装器附带资源。

| 文件 | 字节 | SHA-256 |
| --- | --- | --- |
| `OpenNovel-Offline.exe` | 17,744,896 | `6234a27181d02ad1ebb8c4c0fbba6cf9cfc9c5913707934fe72da28d2265ddf9` |
| `OpenNovel Downloader_0.3.3_x64-setup.exe` | 96,707,137 | `d3f0230633a0ef2a9a63a0524f262411bcebc0cb8667e42593894f8312b8585a` |

原库只读一致性备份 `original-library-backup.db`，integrity_check 为 ok，3 作品/1459 章节/5 任务。原文件校验前后 SHA-256 一致：`da0e840834ce09463481e72872825e9ce541de491cccd7b4d559089c9e42cf10`。未在原库运行真实下载或自动迁移。

桌面唯一匹配的旧 OpenNovel 快捷方式已备份并改为直接指向新 EXE，数据库参数不变；重新读取目标、参数和工作目录核对成功。备份在交付目录 `previous-desktop-shortcut.lnk`，状态见 `shortcut-before.json`、`shortcut-after.json`，防覆盖恢复脚本位于 `.artifacts/restore-offline-shortcut.ps1`。这证明快捷方式属性更新，不证明原生窗口点击/渲染已验收；按用户要求未使用 Computer Use。

关键本机证据（忽略提交，含导出正文/二进制/测试数据，不进 Git）：

- `.artifacts/offline-engine-20261101/distribution-v2/`：镜像、源码及清单。
- `.artifacts/offline-native-20261101/result.json`：首次导入及原生原创 TXT。
- `.artifacts/offline-controlled-20261101/controlled-session-checks.json`：6 项会话隔离及清理。
- `.artifacts/offline-live-20261101/acceptance.json`、`verification.json`：102 章复验。
- `.artifacts/offline-rust-v3.log`、`offline-clippy-v3.log`、`offline-python-final.log`、`offline-frontend-final.log`：回归记录。
- `.artifacts/offline-delivery-20261101/DELIVERY.json` 及 `installation-checks/`：产物摘要和最终 EXE 原创样本检查。

## 未完成门禁

未进行原生窗口/IPC 交互、独立干净 Windows、WebView2 缺失分支和真实账号/已购权益验收；未停止用户 Docker 服务验证冷启动。当前仅本机候选版，不保证所有免费小说完整下载，不宣称起点/其他平台已购全书验收完成。无正式代码签名，未公开发布；公开分发前仍需复核依赖/容器基础层许可材料、更新及安全维护流程，不能将本轮许可证清单视为正式发布认证。

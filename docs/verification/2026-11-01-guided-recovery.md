# v0.3.4 首次使用诊断与恢复验证

日期：2026-11-01。基线 `e596ae08f80e2ccaffe345b5a9881c2086ffff8f`，修改前工作区干净，未另建备份 commit，HEAD 为回退点。优先代码图查询未返回，终止等待后回退源码；未使用 Computer Use。

## 问题与实现

原组件检查将命令程序缺失、Docker 服务不可访问和 Windows 容器模式都表示为 `docker_unavailable`。原界面依据当前 `enabled` 值反转启用：一旦已记住选择的组件不再就绪，`enabled=false`，只能再次启用；如果连组件发现也失败，则关闭按钮也不显示。用户无法从该入口忘记原来的启用选择。

本轮修改：

- 宿主 worker 区分未找到 Docker 命令程序、服务不可访问和 Windows 容器模式。后两者不作同一处理；Windows 模式/缺失程序不会尝试启动 Desktop，更不会安装或自动改系统配置。
- 原生状态增加固定 `issue` 枚举和 `next_step`，从已知 worker 原因映射，不解析平台错误文字来执行修复。未知结果不视为就绪，不直接暴露远端细节。
- 检查区显示具体下一步。当前未就绪但曾主动启用时，显示“重新准备本地增强”；即使组件缺失或损坏，也显示“关闭并忘记选择”。设置明确传入 true/false，不再反转当前就绪状态。
- 只读检查、准备和关闭各有等待提示；忙碌时禁止重复操作，离开组件后不提交迟到的界面结果。原生此前的选择代次保护继续生效。
- 安装自检 JSON 和文本摘要也输出下一步。版本化资源目录分别保留，不覆盖此前候选组件；Python 缓存加入忽略规则。

未调整数据库 schema、导出正文算法、平台权限、独立引擎二进制或镜像摘要。主程序/宿主脚本仍与独立引擎分离；默认关闭、用户主动启用、只读不导入、本地固定摘要导入和免费范围保持。

## 本轮验证

| 检查 | 结果 |
| --- | --- |
| Rust 格式 | fmt/check 通过 |
| Rust lint | workspace/all-targets Clippy，`-D warnings` 通过 |
| Rust 回归 | 150 通过，0 失败，5 忽略 |
| 前端 lint | 0 错误，1 个既有 OfficialReaderPanel.vue 格式警告 |
| 前端类型/构建 | typecheck、build 通过 |
| 前端回归 | 60 通过，0 失败，1 个性能用例跳过 |
| Python | `-W error`，43 通过 |
| 原生组件 | 原创两章读取→核心事务保存→自动完整 TXT，通过 |
| 最终 EXE | 必要安装检查及可选组件只读检查通过，`issue=null` 且 `next_step` 非空 |
| Windows 构建 | v0.3.4 EXE 与 NSIS 成功；未在本轮执行安装器安装/卸载 |

新增 Rust 用例验证故障类型、序列化、明确下一步、故障后保留选择、未知诊断不授权；新增 Python 用例验证缺失程序和 Windows 模式差异，以及这些原因不触发 Desktop 启动。前端使用实际编译的 Vue 组件和内存宿主，新增 4 个用例覆盖失效时关闭、准备失败后重试、重复点击/迟到结果、只读检查不启用。这些测试不作为真实窗口交互证明。

首轮 Clippy 指出旧 `check` 方法在生产路径中已不再引用，检查 diff 后将仅供显式测试使用的方法标记 `cfg(test)`，完整复验通过。未放宽 lint，也未跳过失败用例。

原生组件测试从独立目录运行本轮 Rust 原生测试程序，清除引擎/Python 环境变量，PATH 仅 System32，不依赖工作目录；发现新版同目录组件，恢复已记录的主动选择，再完成原创两章完整 TXT。本轮不删除 Docker 镜像/卷，不停止用户服务，不请求小说平台，不打开原书库。最终分发 EXE 的 `--check-installation` 同样从不同工作目录、纯系统 PATH 验证，并使用新建原创样本库。

上一轮真实免费作品 102/102 及冷镜像首次导入证据见[v0.3.3 记录](2026-11-01-offline-engine.md)；本轮没有重复这些平台请求，也不把旧证据称为本轮实测。

## 交付与数据保护

目录 `.artifacts/guided-delivery-20261101`，EXE 旁保留完整 `opennovel-engine` 资源。安装器资源路径已检查。旧 v0.3.3 目录保留。

| 文件 | 字节 | SHA-256 |
| --- | --- | --- |
| `OpenNovel-Guided.exe` | 17,749,504 | `ac5b79095a2257809f6cc53191eeb43dcefb2b07626eb8a760a3b6c8335bb25a` |
| `OpenNovel Downloader_0.3.4_x64-setup.exe` | 96,700,587 | `bc556ed9f6c4c8ba429154f6d3e91cabe9cbf743e4451062356c37bf0f574dbc` |

原库通过只读连接做一致性备份，`original-library-backup.db` integrity_check 为 ok，3 作品/1459 章节/5 任务。原文件前后 SHA-256 相同：`da0e840834ce09463481e72872825e9ce541de491cccd7b4d559089c9e42cf10`。未在原库运行下载、迁移或导出。

仅修改唯一匹配 v0.3.3 目标及原书库参数的桌面 OpenNovel 快捷方式，先备份到交付目录 `previous-desktop-shortcut.lnk`，再更新为新版 EXE。重新读取目标、参数和工作目录核对成功；不会关闭用户旧窗口，用户需自行退出后再打开。防覆盖恢复脚本 `.artifacts/restore-guided-shortcut.ps1`；状态在交付目录 `shortcut-before.json`、`shortcut-after.json`。

证据留在忽略的本机目录，不提交正文、数据库、镜像、缓存或安装包：

- `.artifacts/guided-rust.log`、`guided-frontend.log`、`guided-python.log`：完整回归。
- `.artifacts/guided-resources.log`、`guided-package.log`：组件准备与打包。
- `.artifacts/guided-native-20261101/result.json`、`native.log`：原生原创 TXT。
- `.artifacts/guided-delivery-20261101/DELIVERY.json` 及 `installation-checks/`：最终 EXE、产物摘要与原库备份。

## 验收边界

Docker 程序缺失、Windows 容器模式及导入失败分支由受控测试验证，未卸载本机 Docker 或切换用户容器模式。未进行原生窗口/IPC 点击、独立干净 Windows、WebView2 缺失、真实账号或 VIP 验收。仍需 Docker Desktop Linux 后端，无正式代码签名。本机候选交付不代表全平台、全书或正式公开发布通过；本轮不创建 tag，不推送远程。

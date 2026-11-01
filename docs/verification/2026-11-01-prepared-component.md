# 独立本地引擎组件准备验证

2026-11-01。开始基线 `86839f3`，工作区干净。沿用用户对本地引擎审查、隔离及后续开发的授权，不使用 Computer Use，不读取真实账户或修改原书库/快捷方式。

## 实现

- Windows HTTP 测试接收流显式转为阻塞模式，确定性延迟请求红绿验证和默认并行全量通过，详见[根因记录](2026-11-01-http-fixtures.md)。没有调整生产 HTTP 策略。
- `scripts/prepare_local_engine.py` 核对固定候选源后，向一个新的绝对目录复制运行脚本、校验清单、完整源码及许可证，可选复制 EXE 并生成独立测试库启动脚本。不会下载或启动候选，不覆盖或清空已有目录。
- 将只读运行配置和源码校验抽到 `engine_runtime_config.py`；运行组件不再导入整个研究构建程序。原研究 runner 继续导出原有路径/校验接口。
- 桌面支持显式 `OPENNOVEL_ENGINE_ROOT`；运行目录必须恰好包含 7 个普通脚本文件，字节须与本次 EXE 构建内嵌内容一致。额外的 json.py、目录和改写脚本拒绝，组件清单不能扩展允许代码。未配置时保留原开发布局兼容。
- 候选仍在独立 Docker 构建卷内，不分发候选二进制。复制完整源码是独立组件校验与许可材料，不移植候选算法进主程序；Python、Docker、镜像及构建卷仍需另行准备，不称通用安装包或全面安全审计。

## 当前实测

| 检查 | 结果 |
| --- | --- |
| `cargo fmt --all -- --check` | 通过 |
| workspace Clippy 全 targets，`-D warnings` | 通过 |
| `cargo test --workspace`，默认并行 | 140 通过、0 失败、4 个显式忽略（性能及需 Docker 的原生用例） |
| 桌面运行目录测试 | 原生 Rust 测试进程接受正确目录，拒绝改写脚本、多余脚本及相对路径 |
| Python `-W error` 门禁/协议/准备测试 | 22 项通过，未请求平台或 Docker |
| 准备工具测试 | 精确复制清单/许可证/哈希、保留既有输出、缺输入不创建目录、独立库启动脚本通过 |
| 实际组件准备 | 993 个候选源文件复制并匹配；在另一个临时 cwd 导入复制的运行配置校验成功，未使用原研究目录脚本/源码 |
| 独立组件原生验收 | 显式配置新 ROOT 后，就绪检查通过；Windows 原生后端经复制脚本及 Docker 无网引擎保存原创两章，自动 TXT 生成并核对成功 |
| 前端 lint | 0 errors，保留 OfficialReaderPanel.vue 既有 warning 1 条 |
| typecheck/Vite/Windows release | Tauri `build --no-bundle` 通过；本轮未改界面或 IPC 名称，不重复前端交互测试 |

`prepared-preview/OpenNovel-Preview.exe`：17,646,080 字节，SHA256 `1c6146456b439a54e557c9897e7d654825a39010f9bb1494e5d0a4a00050129a`。

本机组件位于忽略目录 `.artifacts/engine-component-20261101/prepared-preview/`，入口 `Start-Isolated-Preview.cmd`，元数据 `component.json`，核对报告 `../preparation-check.json`。运行脚本自带本机 Python 路径，仅适用于当前机器；未更换原快捷方式。源码、EXE、日志、测试正文与数据库均不应加入 Git。

证据：`.artifacts/engine-component-workspace-20261101.log`、`engine-component-build-20261101.log`，以及 HTTP 定位/失败日志。没有把未执行的窗口交互、真实多作品或全书试验计入通过项。

## 审批中断与恢复

本轮最初两次显式 `git add` 的自动审批未能在期限内完成，当时未执行暂存；该超时不是代码不安全判定。保留改动并完成不受影响的准备/验证工作，没有绕过 .git 写保护。

默认受限进程连接 Docker named pipe 返回 permission denied。后续该动作的正常提权审批恢复，已使用新 OPENNOVEL_ENGINE_ROOT 显式运行 `native_worker_reuses_session_and_commits_validated_txt`：本次新增的正常就绪检查和原创两章自动 TXT 贯通均通过，日志 `.artifacts/prepared-component-native-20261101.log`。这是本轮证据，不借用前轮结果。Git 提交仅包含审查后的本次源码、原创测试和说明，不自动 push/tag。

真实平台全书、VIP 已购、独立干净 Windows 和原生窗口操作仍未完成；本轮不扩大来源能力结论。

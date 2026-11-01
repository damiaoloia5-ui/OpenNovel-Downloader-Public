# 本地引擎桌面开发预览验证

日期 2026-11-01。修改前 `main` / `bc32d5c`，工作区干净。依据用户授权继续开发默认关闭的隔离本地组件，不再询问常规步骤；图工具未索引后读取源码。没有使用 Computer Use，没有访问真实账号或修改原书库/桌面快捷方式。

## 本轮交付

- 添加小说页增加组件状态、主动启用、重查和操作说明，每次应用启动默认关闭。仅启用后，单本番茄链接选择独立引擎来源；普通 SourceHub/CLI/批次不因此获得引擎能力。
- 官网完整目录仍由 core 解析；明确 `needPay=0`、非付费作品的 App-only 章节可以尝试，付费标记和未知权限继续拒绝。网页来源与原任务不改写、不自动合并。
- 桌面直接复用已打开的 Library，Python stdio producer 仅生成章节信封，避免另开进程争抢同一数据库锁。身份、标题、权限、字数等仍由核心独立验证后事务提交。
- 任务内复用隔离进程，暂停、取消、失败后保留进度；恢复新建会话，只请求未存章节。复用原有自动 TXT 服务，无需用户另选导出格式。
- 固定脚本路径和显式 Python 路径；状态检查不请求小说平台，不自动安装或拉取镜像。候选不挂载书库/浏览器/凭据/Docker socket，只由固定 HTTPS 门禁正常读取官方免费内容。
- Docker 容器启用 AutoRemove；启动等待 Running，逐章开始检查实际容器，防止附着的 Docker CLI 尚存而实际引擎已退出。清理关闭 stdin，仅处理本次精确 ID；取消/父进程 EOF 丢弃迟到正文。限流、访问挑战和权限变化返回脱敏分类，不暴露正文、签名或凭据。

本轮不修改数据库 schema、账户登录、支付权限、验证码处理或既有免费网页正文算法。

## 验证结果

| 检查 | 实际结果 |
| --- | --- |
| Rustfmt | 通过 |
| workspace 全 targets Clippy，`-D warnings` | 通过 |
| Rust 全量回归，原生执行、`--test-threads=1` | 138 通过、0 失败、4 忽略；其中隔离原生用例另行显式执行通过，其余为既有性能用例 |
| 新核心目录测试 | 2 项通过：区分 App-only 免费与付费，保留付费作品/未知字段保护 |
| 新本地 TCP 夹具回归 | 先复现失败，再通过：未发送 HTTP 即关闭的 TCP 连接不能消耗预设响应 |
| Python 门禁及桌面协议 | 18 项通过，包括顺序复用、拒绝任意配置、旧编号、在途取消、EOF、脱敏挑战 |
| 隔离持久会话 | 6 项通过：复用、挑战停止、新会话恢复、取消、旧响应和死容器；逐例核对清理 |
| 三章跨进程恢复 | 通过：保存 1 章停止，同一任务恢复到 3 章，只请求缺失 2 章，部分/完整 TXT 检查通过 |
| Windows 原生桌面后端贯通 | 显式运行 ignored 用例通过；真实 Python 和 Docker 无网候选连续读取原创 2 章，核心入库，原有后台自动 TXT 状态 ready、文件正文核对通过 |
| 前端 lint | 0 errors，保留 OfficialReaderPanel.vue 既有换行 warning 1 条；未做无关格式修改 |
| typecheck / Vite build | 通过；JS 201.37 kB、gzip 67.08 kB |
| 前端测试 | 50 通过、0 失败、1 既有性能项跳过 |
| Windows release EXE | Tauri `build --no-bundle` 通过，17,605,632 字节 |

### 没有隐去的失败

默认并行全量测试在本机多次出现回环 HTTP 的 IncompleteMessage、连接拒绝、未观察到请求等间歇异常；正常原生权限对照仍会出现，不能归结为沙箱。排查确认旧测试服务器误处理零字节预连接，已增加确定性失败/修复回归，并保留完整请求和重试断言。**该修复没有消除全部并行异常**；单线程完整回归通过，不宣称默认并行测试已稳定。生产网络策略、超时、重试次数和挑战判断未因此放宽。

初次 Clippy 的多余 mut 已修复；诊断期间的临时日志已移除。验证环境使用项目内 CARGO_HOME、TMP/TEMP，避免默认临时目录权限问题，未放宽系统权限。

## 预览产物

忽略目录 `.artifacts/desktop-engine-20261101/delivery/`：

- `OpenNovel-LocalEnginePreview-492adb3bda94.exe`
- SHA256 `492adb3bda9465b878c65239b07f59802c31b3d9a5742a3162ba1e57b77a9fd2`
- `Start-Isolated-Preview.cmd`：只为当前机器设置进程级 Python 路径，使用 delivery 下独立 preview-library，不更改系统设置或现有快捷方式。
- `delivery.json`：尺寸、哈希和交付边界。

这是本机源码树配套的开发预览，未捆绑 Python、Docker 或候选 Linux 二进制，不能复制一个 EXE 就在其他机器获得引擎。固定候选仍 GPL-3.0 独立组件，未移入主程序或安装包；通用分发前仍需完成许可、版本与安装管理。

日志位于 `.artifacts/desktop-engine-{workspace-serial-tests,native-acceptance,frontend-tests,final-build}.log`；并行失败日志另存 workspace/native-tests。持久链路证据在 `.artifacts/persistent-engine-20261101/`。受控正文、数据库和构建产物不进入 Git。

## 剩余门禁

没有执行原生窗口点击/布局验收，前端构建及后端测试不替代它；遵守用户禁止 Computer Use 的要求。未新增真实商业作品请求，因此既往真实单章成功仍是单章证据；真实多作品/全书、千章速度、付费已购账号、可移植组件安装和干净 Windows 门禁未完成。宿主/daemon 崩溃下极小的 created 容器窗口仍需维护者检查；取消不保证已发 HTTP 的物理即时中止。

操作与维护步骤见 [本地引擎指引](../local-engine.md)。本轮不创建 tag、不推送；Git 提交只包含本次源码、原创测试及说明。

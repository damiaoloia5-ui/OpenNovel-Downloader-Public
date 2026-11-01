# 长期本地引擎与持久队列验证

日期：2026-11-01。修改前基线 4a21fe8，工作区干净。沿用用户对本地引擎评估、常规步骤自主执行的授权。本轮目标是消除逐章重启容器的原型结构，接上已有核心事务、恢复和 TXT 导出；未进行新商业平台请求、账号测试或桌面打包。

## 完成内容

### 长期会话

- 原创 `opennovel_engine_service.rs` 通过有界 JSON 行协议连续处理请求，复用同一个候选上下文、客户端与进程；不加载通用路由服务器，不调用设备选择/注册流程。
- 每次请求使用单调递增 sequence；输入上限 4096 字节，输出读取上限 2 MiB。响应必须对应当前序号，不能把旧正文交给下一章。
- `EngineSession` 明确绑定作品和最多 10000 个批准章节；拒绝未知章节，一次仅允许一个在途请求。普通模式必须提供受控 fixture，真实 HTTPS 仍需显式 live=True，默认不启用。
- 持久 relay 与引擎共用无网网络空间，只通过容器 tmpfs 邮箱交换一次性、按编号匹配的请求/响应。宿主继续复用前轮的精确 HTTPS 请求校验，候选不能任意联网。
- 容器非 root、只读根目录、无 capabilities、无对外端口，日志驱动 none；执行时仅挂载只读产物和研究脚本，不挂载书库、浏览器、账号或 Docker socket。
- 上游失败、身份/权限改变、响应超时/越序、进程退出或取消时停止会话，下一次使用必须新建上下文。没有自动换身份、注册设备、验证码处理或接口回退。
- 取消检查覆盖请求前、传输等待、响应交付和返回前；迟到结果不能注入新会话。已经发出的宿主 HTTPS 请求可能继续到既有 I/O 预算结束，其结果丢弃，本轮不声称物理网络请求总能瞬时中止。
- 正常关闭只停止/删除当前对象创建的精确容器 ID，核对清理结果；relay 空闲 90 秒退出，头部读取有 3 秒超时，以限制宿主异常后的遗留生命周期。此空闲策略不是对 daemon/宿主故障下清理必定成功的保证。

### 核心持久队列

- 新增 `Library.create_engine_download`，仅接受独立来源 `fanqie-app-local`，复用现有目录校验、逐章事务、取消/恢复与导出。不新增数据库 schema。
- 将内部授权布尔值改为 Public / QidianSession / LocalEngine，保持起点的 automatic_session 标记只用于原起点路径。局部引擎入口不能创建其他来源任务。
- 普通 SourceHub、HTTP 和批次能力仍未启用该来源：`download_supported("fanqie-app-local")` 保持 false，普通 create_download 不能绕过实验入口。
- `engine_queue` 是独立 stdio 队列伴随程序，向控制方索取当前章信封，再交给既有 `parse_engine_chapter`。身份、标题、免付费标记、字数等校验保持；目录受限项不请求。
- 队列伴随程序在中断后仍保留原书库/任务，可通过原 job ID 恢复；已有章节不重抓。保存过正文时生成 TXT，缺章时保持 partial 标记。

## 实际验证

所有正文为原创受控内容，没有请求真实平台或用户凭据。候选来源固定提交 `07cfb2249d4fd57d19bf4420af0e97621ec92df9`；原始源码仍在忽略目录，未复制到业务代码，未打包/分发其二进制。

### 容器链路

`run_persistent_engine_checks.py` 的 6 个用例全部通过，每例核对本轮容器已删除：

| 用例 | 观察结果 |
| --- | --- |
| 连续复用与未知身份 | 3 次读取使用同一 engine 容器；未知章节拒绝，不增加请求 |
| 访问挑战 | 第一次挑战即关闭，后续调用不再请求 |
| 新会话恢复 | 新建容器/能力后正常读取，未复用挑战进程 |
| 取消在途任务 | 取消后返回停止，没有将迟到正文交给调用者 |
| 旧编号响应 | 人工注入错误 sequence，响应被拒绝并关闭 |
| 死进程 | 进程退出后立即识别，不等待章节超时、不发送新上游请求 |

`run_engine_queue_checks.py` 实际连接 Python 长期会话与 Rust 队列程序：

1. 新建独立原创三章目录及书库，保存第 1 章后停止，任务 cancelled，导出 TXT 为部分内容。
2. 退出旧服务和队列进程，以同一 job ID 重新启动。
3. 恢复只请求第 2、3 章，书籍和任务 ID 不变。
4. 最终 saved=3、state=completed，导出 TXT 含三份对应原创段落，partial=false。

这证明受控断点续存和事务链路，不证明真实免费作品全书或已购正文连续下载。

### 核心及回归

- 新增 3 项 Rust 集成测试：实验入口隔离、跨书库重开只补缺章、目录受限项不请求。
- `cargo fmt --all -- --check`、workspace 全 targets Clippy（warnings 为错误）通过。
- `cargo test --workspace --locked`：134 项通过、0 失败、3 项显式性能测试忽略。
- 核心 `engine_queue` 构建、隔离候选服务构建与 Rustfmt 检查通过。
- 既有 Python 门禁测试 12 项通过；新长期链路 6 项及跨进程队列验收通过。
- 前端 lint/typecheck/build、原生界面及安装包未重复执行：本轮没有修改界面、IPC 或打包入口。没有使用 Computer Use。

回归中出现 Windows 默认临时目录权限错误，切换到项目内既有受控 TMP/TEMP 后通过，未放宽系统权限。全量回归失败时检查到旧 HTTP 测试夹具存在请求未完整读取、提前关闭响应及客户端初始化等待期限过短的问题；只修正测试服务的协议/生命周期，保留断言、生产重试次数与访问限制。最终完整回归通过，未通过跳过用例或降低断言隐藏失败。90 秒空闲退出由实现检查确认，本轮未等待完整空闲期限单独实测。

## 运行与证据

入口为研究工具，依赖前轮固定源码、缓存、Docker 镜像及相应 Rust 示例，不自动下载/安装候选：

```powershell
python -B scripts/research/run_local_engine_gate.py
cargo build -p opennovel-core --example engine_queue --locked
python -B scripts/research/run_persistent_engine_checks.py
python -B scripts/research/run_engine_queue_checks.py
```

证据保存在忽略目录 `.artifacts/persistent-engine-20261101/`：`controlled-session-checks.json`、`queue-*/queue-checks.json`、独立样本库与 TXT，以及 clippy/workspace 日志。正文、数据库、容器产物和能力令牌不提交 Git。

## 仍未完成

长期服务减少了容器启动次数，但没有进行真实千章测速，宿主 HTTPS 连接仍按既有单请求实现处理，不能据此给出真实速度倍数。本轮完成核心队列和会话连接，尚未交付 GUI 自动目录入口、组件安装/版本管理、发布包、真实多作品/全书与账户验收；普通用户仍通过现有桌面功能操作，不能将研究 API 视为已启用的新来源。

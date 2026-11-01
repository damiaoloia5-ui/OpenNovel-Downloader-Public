# 番茄本地引擎开发预览

2026-11-01：桌面已接入默认关闭的隔离组件入口。真实内容当前仅有既往单章验收；本轮原生后端使用原创两章验证。尚未承诺真实全书、全站或 VIP 支持，也不是已随安装包交付的通用组件。

## 普通操作

1. 启动配置好组件的新版桌面程序，进入“添加小说”。
2. 展开“番茄网页只保存了前几章？本地引擎实验”，查看状态，点击“启用本地引擎实验”。每次重启默认关闭。
3. 粘贴番茄官网作品详情链接，点击“开始保存为 TXT”。默认选择目录中明确免费的章节；网页锁定、App 正常接口可能开放的免费章可以尝试。
4. 在作品工作台查看进度；已存正文保留，暂停后继续原任务只补缺章。停止或完成后，由现有自动 TXT 服务生成文件。
5. 看到付费、验证或完整性错误时查看原因，处理后再继续。软件不自动购买、注册设备、轮换身份或处理滑块。

新引擎使用独立来源 `fanqie-app-local`，与 `fanqie-web` 分别入库，不自动合并已有网页版本。关闭组件前先暂停任务并等进程清理完成。关闭后仍可阅读、导出已有正文；再次续存需重新启用。

## 维护者准备环境

这是本机开发组件，支持独立准备目录和原有源码树两种布局；不从当前目录或网页下载并执行脚本。Python、Docker、固定镜像及构建卷仍为外部依赖，单独复制 EXE 到另一台机器不能获得此能力。

- Windows、项目现有 Rust/Node 构建环境、Python 3.12+、运行中的 Docker Desktop Linux 后端。
- 先按[隔离评估](research/local-engine-admission-20261101.md)固定候选源码、inventory、镜像和依赖缓存；不要运行来源不明的 Windows 成品，也不要使用候选设备补齐/权益入口。
- 候选固定 `ch6vip/fqapp` 提交 `07cfb2249d4fd57d19bf4420af0e97621ec92df9`，GPL-3.0。候选仍独立构建，未复制进 Apache-2.0 主程序。本预览没有打包分发候选二进制；未来分发前需落实其对应源码、许可和通知义务。
- 运行 `python -B scripts/research/run_local_engine_gate.py` 完成离线构建与门禁，确认固定 target volume 中存在 `opennovel_engine_service`。
- 在启动桌面程序的同一个 PowerShell 中配置 Python 可执行文件的绝对路径（请换成本机安装路径）：

```powershell
$env:OPENNOVEL_ENGINE_PYTHON = 'C:\Python312\python.exe'
& $env:OPENNOVEL_ENGINE_PYTHON -B scripts/research/desktop_engine_worker.py --check
npm run desktop:dev
```

状态检查只验证固定源码哈希、本地镜像、只读构建及 Docker 可用性，不向小说平台请求正文，不自动安装依赖或拉取镜像。不需要填写 API Key、Cookie 或账户凭据。依赖缺失时只禁用引擎，普通网页功能保持原流程。

### 独立准备目录（推荐用于本机预览）

先完成固定构建和桌面 `npm run desktop:build`，再使用当前 Python 准备一个**尚不存在的绝对目录**：

```powershell
python -B scripts/prepare_local_engine.py --output 'D:\OpenNovel-Engine-Preview' --executable target/release/opennovel-desktop.exe
```

工具先核对已有候选源码，再复制 7 个运行脚本、完整候选源码及其许可证、校验清单和可选 EXE。不会拉取镜像、请求章节、执行候选或覆盖已有目录。复制失败时保留现场，不自动递归删除输出。候选 Linux 二进制不在复制目录中，仍使用本机预先构建的只读 Docker 卷。

双击新目录中的 `Start-Isolated-Preview.cmd`，启动脚本仅设置进程级 `OPENNOVEL_ENGINE_PYTHON` 和 `OPENNOVEL_ENGINE_ROOT`，并使用该目录的独立 `preview-library`。程序启动后仍需主动启用引擎，不更改现有快捷方式、系统设置或原书库。

程序按本次 EXE 构建嵌入的字节核对全部运行脚本，并拒绝额外脚本、子目录或被改写的脚本；`component.json` 只是校验记录，不能授权任意代码。升级脚本或程序后请重新准备一个新目录，不能拼接不同版本。清单及源码校验不构成对 Docker 卷内二进制或整套依赖的全面安全证明。

不需要继续读取原开发目录内的运行脚本或候选源码，但仍依赖本机 Python 安装和 Docker 环境，因此该目录不是通用离线安装包。最新准备验证见 [本轮记录](verification/2026-11-01-prepared-component.md)。

## 实现与限制

桌面后端在进程内复用 `Library`，避免伴随程序再次打开同一数据库锁。Python 仅负责生产章节信封；核心独立验证身份、标题、免付费状态和字数后事务提交。前端不接收原始引擎 HTML，也不负责抓取和导出。

每个任务懒启动一组无网容器，连续章节复用进程。只有宿主固定 HTTPS 门禁可以向已审查的官方目的地发送限定 GET；不接受任意 URL、重定向、凭据或写操作。候选不挂载书库、浏览器数据或 Docker socket。

JSON 行大小、编号、身份和读取等待均有上限。取消/EOF 通知 worker，丢弃迟到结果并清理本次精确容器；容器自动删除，relay 空闲退出。Docker daemon 故障、宿主强杀或 created 状态的极小崩溃窗口仍可能需要维护者检查，不能保证所有系统故障下零遗留。已经发出的 HTTP 请求可能继续到既有 I/O 期限，结果不会入库。

单次限流、访问挑战或异常即停止，不并发轰炸、不自动切换其他服务。真实全书覆盖、速度、原生交互与独立安装交付仍需后续实测。

# Windows 一体化组件

适用：v0.3.8 Windows x64 一体化版。基础包和历史 Docker 开发组件不等同于本方案。

## 用户操作

运行一体化安装器，打开软件，粘贴作品链接，点“开始保存为 TXT”。番茄增强仅首次需要主动启用并记住选择。软件自行检查内置组件；用户无需安装 Python、Docker、Rust、Node 或配置环境变量。Windows WebView2 缺失时由安装器附带的离线安装程序处理。章节获取本身仍需要联网；“离线安装”不表示能离线下载新小说。

所有功能入口在同一个软件内。账户平台仍需本人登录，真人验证仍需本人处理，未验收的平台能力不会因打包而变成已支持。TXT、EPUB 和书库管理保持原有流程，部分保存继续明确标注缺章。只能复制整个交付目录，不能单独复制 EXE。

## 边界和数据流

主程序继续使用 Rust core 管理书库和导出。内置 Python 负责受限请求和进程生命周期；独立 GPL 引擎只做签名请求构造和响应处理，通过有长度上限的 JSON 行通信。未把 GPL 候选代码复制或链接到 Apache-2.0 主程序中。

1. 主程序核验 Python、运行脚本、引擎和完整源码压缩包的固定摘要。包内 manifest 不能自行授权代码。
2. 任务创建独立 AppContainer，能力列表为空，未配置网络豁免。只向本次临时复制的引擎授予读取/执行权限，不改变书库或浏览器目录权限。
3. 只继承两条输入输出管道；进程创建时原子绑定 Job Object，限制一个进程及 256 MiB 内存，关闭宿主 Job 句柄会终止引擎。禁止子进程，不启动 HTTP 监听或 Docker。
4. 引擎提出的请求必须经过宿主同一 HTTPS 门禁：固定官方目的地、GET、限定路径/参数/请求头和本任务章节 ID，不转发 Cookie、账户数据或任意 URL，不接受重定向。
5. 返回内容必须符合序号、作品/章节身份、明确免费状态；core 再核对章节标题、字数和正文并同事务保存。TXT 自动交付不变。
6. 验证、限流、权限变化、超时、进程退出均停止并保留已保存进度。取消后不提交迟到响应，继续时新建隔离进程。

没有普通进程回退：AppContainer、完整性或自检失败只禁用增强，不能通过安装 Docker 或放宽权限绕过失败。

微软说明 AppContainer 使用独立 SID 与能力限制资源，未授予网络能力时不能联网；实现使用其文档规定的进程属性与句柄列表，并额外检查实际令牌。这些机制不等于经过第三方安全审计。[AppContainer 启动说明](https://learn.microsoft.com/en-us/windows/win32/secauthz/implementing-an-appcontainer)、[进程属性说明](https://learn.microsoft.com/en-us/windows/win32/api/processthreadsapi/nf-processthreadsapi-updateprocthreadattribute)。

## 构建和再分发

`scripts/build_native_engine.py` 从已核验完整源包构建独立 Windows x64 引擎，保留原始候选文件，使用单独 workspace、原创 stdio 适配层及 228 个锁定 vendor 依赖。Cargo 使用 `--locked --offline`；CRT 静态链接。`scripts/research/native_engine_lock.json` 固定交付字节，换二进制必须重新审查并更新清单。

完整源码位于组件的 `native-engine/corresponding-source.tar.gz`，含上游 GPL 许可证、依赖源码/许可证、原创包装器及 `REBUILD-NATIVE.py`、`REBUILD-NATIVE.md`。独立组合引擎按 GPL-3.0 分发，主程序保持 Apache-2.0。重建前置工具仅面向维护者，用户端不需要。

组件准备沿用 `scripts/prepare_integrated_component.py --distribution <固定原生分发目录>`。`scripts/verify_project.py --offline --package integrated --native --distribution <目录>` 执行全量检查；`scripts/research/run_native_engine_checks.py` 额外执行原创原生压力、停止恢复和系统隔离验证，普通 CI 不请求商业平台。

一体化配置使用 Tauri `offlineInstaller`，把微软 WebView2 离线安装器带入安装包，因此包体积明显增大。维护者首次构建仍可能需要下载微软运行时。[Tauri Windows 安装说明](https://v2.tauri.app/distribute/windows-installer/)。

## 已知限制

正常退出会清理本次临时 EXE 和 AppContainer 配置；宿主被强杀或断电时 Job 可终止子进程，但 Windows 临时目录/空配置可能遗留，不含书库或正文。系统或安全软件阻止执行时应显示失败，不能保证任意 Windows 配置都兼容。正式代码签名尚未办理。

原创 1000 次本地处理用来测量本地开销，不代表真实 1000 章下载速度。清空 PATH 验证不代替干净系统；现机 WebView2 存在也不能证明缺失分支已运行。真实账号、平台覆盖和独立干净 Windows 验收继续按收尾矩阵记录。

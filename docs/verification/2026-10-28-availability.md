# 可获取范围与缺章说明验证

日期：2026-10-28。干净基线 main / 897c75f。没有使用 Computer Use，也没有以其他工具操作窗口或浏览器。

## 已实现

- 添加页以当前来源能力和目录 restricted 标记呈现开放、受限和待验证数量。未开放来源不因 restricted=false 显示免费；起点账户目录不推断登录或阅读权益。
- “仅保存目录标记开放章”生成完整开放序号集合（含不连续集合），复用既有下载命令；与高级自定义范围分开。零开放章时禁用，链接修改后需重新读取；高级选章仍可记录受限项。
- core 的 book_details 为每章提供可选 availability（state、reason、job_id）。SQL 一次查询每章最近任务，按 rowid 确定同秒先后；后建窄范围任务不会覆盖范围外章节旧原因，其他作品任务不会串入。
- 已存正文优先；缺章依次结合未支持来源、最近确认目录限制和每章任务状态。保留任务原始错误，不从关键词断定付费或登录。不新建任务，不刷新官网，不修改正文或任务状态。
- 工作台概览/保存正文显示分类数量、下一步、原始原因、类型和标题/序号筛选，每页 50 项，并进入对应任务明细。历史快照和旧数据无新字段时允许空值，前端降级待核对。
- 新增 npm run test:frontend 并接入现有 Windows CI 配置；远端 CI 尚未运行。

架构见 [ADR 0012](../adr/0012-chapter-availability-evidence.md)，操作见 [快速开始](../QUICK_START.md)。

## 已运行验证

| 检查 | 结果 |
| --- | --- |
| npm run lint | 通过，无警告 |
| npm run typecheck | 通过 |
| npm run test:frontend | 19 项通过，含新增 6 项范围/分类模型及 1 项真实 Vue 编译与服务端渲染 |
| cargo fmt --all -- --check | 通过 |
| cargo clippy --workspace --all-targets --locked -- -D warnings | 通过 |
| cargo test --workspace --locked | 84 项通过、0 失败，含新增 6 项原创核心回归 |
| npm run build（也由 Tauri beforeBuildCommand 执行） | 通过，最终 JS 178.39 kB / gzip 59.69 kB，CSS 24.46 kB / gzip 5.42 kB |
| 当前源码 CLI | 重新构建后新库原创导入 2/2；book JSON 含逐章 saved 依据；真实 TXT/EPUB 导出 2/2、非部分内容；标题、TXT 首末章顺序、EPUB ZIP 与 mimetype 通过 |
| npm run desktop:build | 最终 Tauri release --no-bundle 通过；只证明编译，不证明原生交互 |
| 生产资源 | 存在新范围、缺章及自定义按钮文案，不含受控测试替身标记 |

测试没有使用真实平台正文或账户凭据。日志与原创导出保存在忽略目录 `.artifacts/availability-20261028`，不提交。

## 验证环境修复

首次全量 Rust 测试的旧导出回归在系统临时目录报 Windows PermissionDenied（代码 5），独立复现；同一测试仅将 TMP/TEMP 改为项目 `.cache/availability-tests-tmp` 后通过，随后全量 84 项通过。没有修改导出实现或弱化覆盖保护，也未对系统临时目录做权限修改。该结果不能证明默认临时目录权限问题在系统层已解决。

组件测试最初使用 Vite 开发服务器 SSR，在样式模块 transport 加载处超时。测试工具改为编译实际组件的 SSR 生产模块再直接渲染，真实组件断言保留：整书计数、已收齐、缺章分类、未知/未支持降级、50 项上限、分页、原始原因转义。它不启动浏览器，也不证明实际界面交互。

## 证据边界

本轮不操作原生程序窗口，未完成原生添加/下载/缺章筛选及任务跳转的人工交互验收；原生构建和 CLI 不能替代这些证据。真实登录、已购/未购差异、限免、广告阅读及官网连续自动保存均未验证。没有实现破解付费、跨平台版本查找、第三方正文拼接或自动购买。

目录开放数只是当前适配器解析结果；保存失败时仍显示实际缺章。报告依据持久的最近确认目录与任务，可能落后于官网现状，用户需明确检查目录或恢复原任务后才有新依据。

本轮使用独立本地候选程序交付，保留旧程序与原书库，不做数据库迁移，不创建 tag、push、签名或公开发布。最终构建与交付哈希见 `.artifacts/availability-20261028/delivery.json`。

交付程序：`.artifacts/availability-20261028/OpenNovel-Availability.exe`；携带原书库参数的入口：`启动可获取范围版.cmd`。通过快捷方式文件操作更新之前创建且目标/参数匹配的两个桌面入口，没有启动或控制窗口；原书库更新前后 SHA-256 一致。

最终程序 SHA-256：`2848AD003A9724DA4FE4A8D29837B5F972E218180C0AC07AB0FFE1A75C92F811`。不生成新安装器，用户日常使用桌面快捷方式或启动脚本，裸 EXE 使用默认书库位置。

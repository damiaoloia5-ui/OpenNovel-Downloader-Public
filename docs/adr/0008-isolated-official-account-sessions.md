# ADR 0008：独立的临时官网会话

日期：2026-10-21。账户实验基础设施，schema 8；不等于已购正文支持。

先实现可验证的官网窗口隔离，不导入 Cookie，不让用户提交密码。核心仅保存 UUID、平台和本地别名，最多 10 个记录；名称不是远端账户身份。浏览器会话使用独立临时 data_directory 和 InPrivate，关闭后下次使用全新目录。没有持久凭据，因此当前无需系统凭据库；后续若持久保存会话，必须先设计系统安全存储、失效和迁移策略，不把明文 Cookie 放入 SQLite。

起点和书旗分别限定 HTTPS 平台域及其子域；入口只接受不带 query/fragment 的官网作品链接或首页。跨站 SSO 暂不放开，遇到不支持导航显示提示。同域新窗口在原会话导航，拒绝无限 popup 和网页文件下载。此策略可能使部分登录方式不能使用，需要用户实测后逐项适配。

主窗口固定使用当前书库目录下的 UI WebView 数据目录，只允许应用本地地址导航。所有自定义 IPC 在统一分发前检查真实调用 WebView 标签和当前本地 origin，远程账户窗口无权访问书库、导出、文件路径或其他账户操作；capabilities 仍仅授权 main，不加入 remote。

WebView2 的全局 WEBVIEW2_USER_DATA_FOLDER 会覆盖调用侧目录设置。账户窗口发现该变量时拒绝创建并说明原因，避免表面上使用不同路径却共享浏览器数据。测试专用远程调试实例只使用自造标记；用户登录实例不启用调试端口。

关闭并清理通过浏览器 clear_all_browsing_data，再销毁窗口；移除记录保留小说书库。窗口 X 结束临时 InPrivate 会话，重新打开全新浏览器环境。临时目录清理失败时可能保留浏览器非会话缓存，但从不再次复用。退出仅作用于本机窗口，不声称撤销平台其他设备会话。

账户状态仅为 closed / open_unverified；无已验证的登录检测、权限查询和已购正文适配器时，can_download 固定 false。备份只包含非敏感记录，恢复后无活动会话；诊断继续不含账户记录。AC-15/16 必须以真实平台、用户本人登录和已购/未购场景验收，不能由窗口基础设施或模拟标记替代。

依据：[Tauri WebviewWindow API](https://v2.tauri.app/reference/javascript/api/namespacewebviewwindow/)、已锁定 Tauri 2.12.3 / Wry 源码及 [WebView2 多配置说明](https://learn.microsoft.com/en-us/microsoft-edge/webview2/concepts/multi-profile-support)。

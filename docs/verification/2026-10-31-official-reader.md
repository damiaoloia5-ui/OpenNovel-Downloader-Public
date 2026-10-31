# 三家官网会话及当前页 TXT 实验

日期：2026-10-31，干净基线 cb0928c。图工具无本项目索引后读取当前源码与约束。使用工程纪律及完成前验证技能；全程无 Computer Use、浏览器自动化、真实凭据或真实书库操作。用户明确暂不方便登录，继续开发；真人登录、VIP、原生交互不能标记完成。

## 产品范围与架构

三家可从作品链接进入对应面板，或从账户页展开入口。本人在独立官网窗口登录和打开章节，软件只识别当前页；原有起点向导和自动保存逻辑保留。不是三家自动全书接入，也没有完整目录导入。没有把站点登录、买 VIP 和获取全书混为一项能力。

采用固定的三种临时会话，复用原官网窗口创建边界，新增平台严格作品 URL 与导航域名名单；咪咕仅额外允许实际观察到的 passport.migu.cn 认证域。全局 WebView2 目录覆盖时拒绝创建，以保留会话隔离。新入口不调用 accounts 表，避免为临时登录修改 SQLite schema；数据库仍为 8。浏览数据仅临时目录，关闭清理，退出后需重新登录。

识别前取得读取租约，回调有 8 秒等待与 2MiB 上限，返回后复核完整 URL、导航计数和会话 generation。预览按平台/会话绑定、10 分钟有效，只保留每个平台的当前预览。保存需本人确认，消耗预览令牌并复核会话。远程官网窗口不能调用本地 IPC。

脚本仅读可见 DOM、受限的公开章节身份字段，不读密码输入、Cookie、storage、账户 license，不执行页面脚本、OCR、解密或验证交互。识别状态包括 loading/book/text/restricted/protected/verification/unsupported；非文字状态不能携带段落。核心再校验平台、地址、身份、字段、长度、限制提示和字形。咪咕未验证的 reader 路由即使构造 text 响应也拒绝。

新 TXT 是当前页普通可见文字的独立导出，不写入全书书库。文件名由平台、数字身份和随机 ID 生成；临时文件同步后用 persist_noclobber 提交，不覆盖已有文件。TXT 永久注明当前采集范围和未核验全章/全书；有官网字数时仍检查比例，缺失字数绝不标成完整章。打开文件夹仅接受本次导出 receipt ID，不接受用户传入任意路径。

## 官网调研证据

- 刺猬猫 https://www.ciweimao.com/signup/login 返回 HTTP 200，标题“登录 - 刺猬猫”。章节 114772495 存在公开书章身份、字数 4700 及 J_BookImage 图片呈现；本入口遇到图片即拒绝，不通过 OCR 或其他接口补正文。普通文字分支只在原创页面验证。
- 飞卢 https://u.faloo.com/regist/login.aspx 返回 HTTP 200，标题“飞卢网--用户登录”。公开 1484744_1.html 有 hidNovelID/hidNodeID、书名及 noveContent 普通段落，未观察到可核验的章节声明字数；不把未知完整性说成全章保存。
- 原 read.migu.cn TLS 错误不代表全平台不可用。通过[官方悦看主页](https://www.cmread.com/yuekan/)确认新作品入口：/yuekan/novel/600000401811.html HTTP 200，详情含向 App 的跳转；页面自身链接的 n.cmread.com/nap/p/wap_detail_new.jsp 同样可访问并包含 passport.migu.cn 登录入口。登录页 https://passport.migu.cn/login?sourceid=204001 返回 HTTP 200，标题“咪咕用户中心门户-登录”。本轮只支持作品识别，不能保存咪咕正文。

以上只证明公开 HTTP 和页面结构，不证明完成登录、订阅权益、浏览器实际显示或原生窗口效果；原始页面和技术日志放在忽略目录 .artifacts/account-platforms-20261031，不提交商业正文。

## 验证

- 新增 3 项 Rust 原创测试：平台/干净作品路由、串平台/凭据/未知字段拒绝、受限状态不能携带正文、声明长度、未知咪咕正文拒绝、独立 TXT 范围标记、不覆盖及保存前重新验证。
- 新增 3 项 JS VM 受控页面测试：仅可见文字、加载/验证/订阅/图片状态零正文、刺猬猫图片拒绝和咪咕仅作品识别。
- 新增 3 项实际 Vue 组件事件测试：本人确认前不保存、保护页不展示保存动作、三平台作品链接进入登录识别而非 HTTP 下载；旧起点流程继续回归。
- 初次受控 DOM 用例失败于 fixture 未将段落挂接到 reader，修正原创宿主父子关系后通过，未放宽订阅检查。Clippy 的嵌套 if 警告按原等价条件修正。

最终检查：cargo fmt --all -- --check、cargo clippy --workspace --all-targets --locked -- -D warnings 通过；cargo test --workspace --locked 为 108 项通过、0 失败、3 项既有性能基准默认忽略。前端 lint/typecheck 通过且无警告，npm run test:frontend 为 46 通过、0 失败、1 项可选旧新脚本基准跳过。前端 build 和桌面 release --no-bundle 均通过，JS 194.31kB / gzip 64.71kB。未执行真实应用窗口，因此不是原生 IPC 实测。本阶段不自动 tag/push，不改原书库或旧导出。

## 交付

独立程序 .artifacts/account-platforms-20261031/delivery/OpenNovel-OfficialReader.exe，17303552 字节，SHA-256：

DE403B836B05C9CA83C868E113513723D7F4C056773D00E04E6705D7CDE56805

副本与最终构建输出哈希一致，旧程序保留。沿用用户已授权的桌面最新版入口更新，将现存“OpenNovel Downloader（当前章保存测试版）”指向此候选，原书库参数不变；更新前原件和哈希记录在本轮 desktop-shortcuts-backup 与 delivery.json，保存后回读核对。没有启动窗口、运行安装器、签名或操作原库。程序重启后各平台需本人重新登录；这不是完整目录或自动全书交付。

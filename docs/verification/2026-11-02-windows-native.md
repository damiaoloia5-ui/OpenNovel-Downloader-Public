# v0.3.8 Windows 原生一体化验收

日期：2026-11-02。开发基线 `f47065b38660dc984038c57d050a71e48aa1f226`；本轮不使用 Computer Use、不请求真实小说平台、不读取账户凭据。

## 本次结果

Windows x64 一体化版已以独立原生组件替换用户端 Docker 依赖。首次默认关闭，用户主动启用后记住选择。Python、独立 GPL 引擎及完整对应源码随包，WebView2 使用离线安装器。核心书库与 TXT/EPUB 流程不变。

完整检查回执 `.artifacts/native-full-checks-20261102/report.json`：14 步全部通过。Rust 152 通过、5 项显式忽略；lint、typecheck、前端测试/构建、Python 57 项、签名契约、CLI 原创验证、桌面构建与原生自检通过。额外显式运行 `native_worker_reuses_session_and_commits_validated_txt`：1 项通过，证明原生组件到 Rust 校验、事务入库与自动 TXT 的原创两章链路。

`.artifacts/native-checks-final-20261102/report.json` 记录：

- 同一原生进程连续处理 1000 次原创章节，约 1.031 秒；仅表示受控本地开销，不含真实网络下载。
- 正常关闭清理，取消时丢弃迟到结果，重新创建会话可继续。
- 验证、限流停止且不自动重试；付费标记、错误作品身份拒绝保存。
- 私有文件读取、写入、本机网络连接、子进程创建被 Windows 隔离拒绝。
- 父进程强退后引擎被 Job 回收。
- 清空 PATH 后由内置 Python 和真实打包 worker 完成原创章节，不依赖开发工具解析。

最初控制台子系统引起 `STATUS_DLL_INIT_FAILED`，使用无控制台 Windows 子系统和继承管道后解决；Windows 必需 LOCALAPPDATA 环境保留，由系统重定向容器配置。关闭时针对本次唯一临时 EXE 处理短暂文件占用，不递归删除用户目录。Job 在 CreateProcess 时原子绑定，消除先创建后绑定的强退窗口。

## 最终交付

目录 `.artifacts/native-delivery-20261102`。桌面程序和 CLI 分别为 `OpenNovel.exe` 与 `cli/opennovel.exe`，避免 Windows 大小写不敏感造成覆盖。

| 文件 | 字节 | SHA-256 |
| --- | ---: | --- |
| OpenNovel.exe | 17783808 | 683c20820bc479a6e45f2fa8447893da4d3e7803995dd00ba8aee6b9edee984a |
| cli/opennovel.exe | 8034304 | 977af17341c11adbc2d8638c078bb279037ba6c8a21a15db26e0232f09b9abed |
| OpenNovel Downloader_0.3.8_x64-setup.exe | 269844128 | 2fc674473b72210ff01161d30714901bed68b4bc1e1b83ef919e583e39c41980 |

安装包约 257.34 MiB；生成的 NSIS 脚本确认 `offlineInstaller` 并嵌入微软运行时。最终复制版在去除开发变量及收窄 PATH 的环境中完成原创安装自检，组件 `installed=true, ready=true, enabled=false`。原书库只读备份通过 integrity_check，3 部作品、1459 章、5 项任务，原文件摘要保持 `da0e840834ce09463481e72872825e9ce541de491cccd7b4d559089c9e42cf10`。

原桌面快捷方式更新到新交付目录，保留相同书库参数；旧快捷方式和带摘要保护的还原脚本保存在交付目录。旧 v0.3.7 目录保留。没有自动 tag、push 或公开发布。

## 未覆盖范围

额外核验：实际嵌入的 WebView2 离线安装器 Authenticode 状态为 Valid，签发主体 Microsoft Corporation，SHA-256 为 `62863f0f0611a16ccf124b4f15650c3b66624504eba74d7cde1e2ae5d951249b`。这是微软运行时的签名，不是 OpenNovel 自身已经完成签名。

未运行独立干净 Windows、WebView2 缺失后的实际安装、原生窗口点击或真实账号/全书验收。清空 PATH 和现机自检不能替代这些证据；本次原生迁移没有重新确认历史真实番茄作品结果。安装包尚未正式代码签名。宿主强杀或断电可能留下无正文的临时 EXE/空 AppContainer 配置；不能承诺所有系统故障下零残留，也不称隔离已经过独立安全审计。

# 产品展示名称调整

2026-11-03，基线 `3e3ce8d2ca16c087345ae6b7bf99749b9e629727`，版本仍为 0.3.19。

按用户要求，侧栏统一显示 `OpenNovel Downloader · 0.3.19`，README、快速开始与使用指南去除产品名称中的阶段标签。桌面入口更名为 `OpenNovel Downloader.lnk`，说明为 `OpenNovel Downloader 0.3.19`，并指向本次重新构建的程序。历史验证记录与内部发布状态保留事实，不因此扩大已验收的功能或平台范围。

前端 lint、typecheck、既有测试（78 通过、1 跳过）、build 和一体化 NSIS 构建通过。新构建 JavaScript 已核对无旧侧栏标签；新交付 EXE 在空 PATH 下通过独立原创样本自检，包括 TXT 保存与恢复，可选组件就绪且未启用。未使用 Computer Use，没有将静态文字核对或自检称为实际窗口视觉验收。本轮没有修改 Rust 或下载逻辑，未重复运行 Rust 全套或商业平台测试，未执行远端 CI。

报告与交付位于 `.artifacts/naming-update-20261103/`：`build-report.json`、`native-check/`、`delivery/`。桌面修改前已备份原快捷方式，修改后通过 Windows Shell 重新读取目标、说明和原书库参数；书库文件 SHA-256 与修改前一致。备份与回执放在独立 `private-backup/`，不放入交付包，恢复脚本为该目录的 `restore-shortcut.ps1`。旧程序与旧安装包保留。

用户重开桌面入口即可使用新程序；已运行的旧窗口不会被强制关闭。本轮只调整展示名称，不伪称已经完成正式证书签名、普通干净系统或真实已购全书验收。

# v0.3.5 TXT 文件恢复验证

日期：2026-11-01。基线 `8de0e93f3e62b6222c6b571328c476cdc5dafb68`，修改前 Git 干净，HEAD 是回退点，无需前置备份 commit。图查询返回项目未索引后读取源码；未使用 Computer Use。

## 问题与实现

原自动 TXT 服务只观察书籍和任务变化。即使 TXT 在应用运行时被移动或删除，只要书籍/任务没有变化，后台就跳过生成并保留旧 ready 状态。核心已经能够安全重建缺失文件，但界面 ready 状态没有重新生成入口，用户只能重启或寻找其他导出方式。

本轮在空闲任务和相同内容签名的情况下，约每 5 秒检查 ready 文件是否缺失；确认不存在时才调用核心从本地已保存正文重建。检查不读取整本正文或计算整文件摘要，不在每次界面轮询中执行，不需要网络。其他文件系统错误不触发自动失败循环；失败状态需要明确重试。任务进行中不恢复，等待其空闲后处理。实际检查周期还受后台队列耗时影响，不承诺固定完成时限。

原子写入、防覆盖、内容摘要和导出记录去重仍复用核心实现，未改变核心文件格式或数据库 schema。已修改的文件不会自动覆盖；手动重新生成遇到内容冲突时保留原文件和书库，并显示原因。恢复只包含实际保存的章节，不能补出缺失正文。

界面新增 ready 状态下的“重新生成 TXT”，并明确反馈请求已提交而非宣称完成。忙碌或保存进行中禁止重复提交；切换作品或组件卸载后，旧请求的成功/失败反馈不写入当前作品。原失败状态的“重试生成 TXT”继续可用。

## 回归结果

| 检查 | 本轮结果 |
| --- | --- |
| Rust fmt/check | 通过 |
| Clippy workspace/all-targets，`-D warnings` | 通过 |
| Rust | 152 通过，0 失败，5 忽略 |
| 前端 lint | 0 错误，1 个既有 OfficialReaderPanel.vue 格式警告 |
| 前端类型/构建 | 通过 |
| 前端测试 | 63 通过，0 失败，1 个性能用例跳过 |
| 最终文案后的定向组件复验 | 30 通过，0 失败 |
| Python `-W error` | 43 通过 |

新增后台用例：生成原创两章 TXT 后删除该夹具文件，不另发请求，等待后台恢复，核对相同路径和完整字节、导出记录仍为 1、导入任务仍完成；另用例修改夹具 TXT 后主动重新生成，核对报错并保留批注、正文和记录。

新增 3 项实际编译 Vue 组件的内存事件测试，验证打开缺失文件报错后的本地重建入口、保存中和重复点击保护、切换作品后迟到结果丢弃。没有使用真实浏览器或原生窗口，因此这些不作为布局、窗口点击或 IPC 交互证明。

原生安装自检也改为在全新夹具书库生成 TXT，读取原创两章，删除本次创建的夹具 TXT，等待后台恢复，再比较路径和全文。此过程只删除独立夹具文件，不打开、删除或改动用户文件。JSON 新增 `deleted_sample_txt_recovered`，成功才将其设为 true。

## 边界与证据

未调整平台、账号、引擎镜像或权限策略；普通 CI 和本轮验证不请求商业小说平台、不使用真实账号。真实免费样本历史结果仍以[v0.3.3 记录](2026-11-01-offline-engine.md)为准，不算本轮重测。原生窗口/IPC 点击、独立干净 Windows、WebView2 缺失、真实账号权益仍未验收。可选番茄引擎仍需 Docker Linux 后端，TXT 本地恢复本身不需要它。

本机忽略目录保存完整证据，不将二进制、数据库、正文、缓存或日志提交：`.artifacts/txt-recovery-rust.log`、`txt-recovery-frontend.log`、`txt-recovery-python.log`、`txt-recovery-resources.log`、`txt-recovery-package.log`。

## 最终候选交付

Windows EXE 和 NSIS 已构建，交付目录 `.artifacts/txt-recovery-delivery-20261101`，旧版目录保留。最终复制的 EXE 从不同工作目录、纯 System32 PATH、无引擎/Python 环境变量启动自检；必要检查通过，`deleted_sample_txt_recovered=true`，可选组件就绪但未启用。本轮未执行安装器安装/卸载，不把构建或自检称为原生窗口验收。

| 文件 | 字节 | SHA-256 |
| --- | --- | --- |
| `OpenNovel-TxtRecovery.exe` | 17,753,088 | `cc5538a4088df12f3c957c10bed64614a644df916c2bd19b76ba65bf0c718a1d` |
| `OpenNovel Downloader_0.3.5_x64-setup.exe` | 96,708,547 | `7c495c31d6897b4fcf35140d6ee8b9ff5a96639d1bbb3c8fe02d0add15150caf` |

原书库只读一致性备份 `original-library-backup.db`，integrity_check 为 ok，3 作品/1459 章节/5 任务。原文件备份前后 SHA-256 相同：`da0e840834ce09463481e72872825e9ce541de491cccd7b4d559089c9e42cf10`，没有在原库运行下载或导出。

桌面唯一匹配旧版目标和原书库参数的 OpenNovel 快捷方式已备份、更新、重新读取核对，指向新版 EXE，书库参数不变；没有关闭用户窗口。备份位于交付目录 `previous-desktop-shortcut.lnk`，状态为 `shortcut-before.json`、`shortcut-after.json`，防覆盖恢复脚本 `.artifacts/restore-txt-recovery-shortcut.ps1`。退出旧窗口后可从原快捷方式使用新版。

最终自检在 `installation-checks/check-8a9b4249-006d-42a0-aeef-756d62da90c3/report.json`，交付回执 `DELIVERY.json` 保存产物和备份摘要。未正式签名，未创建 tag 或推送。

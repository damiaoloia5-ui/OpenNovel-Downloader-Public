# Windows 代码签名（可选发布流程）

流程更新：2026-11-03。当前 v0.3.19 候选安装器仍未正式签名；已补齐签名编排和隔离测试，未购买、申请或取得正式证书。下方供应商信息为 2026-10-21 历史调研，未重新核实报价或受理条件。

## 为什么签名

代码签名供 Windows 验证发布者和文件完整性，并非应用运行或小说下载的前提。开发、自用和候选测试可以先使用当前未签名包；公开分发时再办理正式签名。签名不授予平台阅读权限，也不保证新程序立即消除 SmartScreen 提示。[微软 SignTool](https://learn.microsoft.com/en-us/windows/win32/seccrypto/signtool)、[Tauri Windows 签名](https://v2.tauri.app/distribute/sign/windows/)。

自签名只适用于明确的测试场景，不能替代公众信任。当前流程拒绝自签名开发证书，不创建或安装受信任根，不复用其他项目证书。正式证书须由申请者本人完成身份核验、确认费用和签发；不要把证件、私钥、密码或 PIN 发到聊天或仓库。

## 当前可执行命令

需要 Windows x64、PowerShell 7、Windows SDK SignTool。脚本会检查 SignTool 的微软签名。

```powershell
npm run signing:inspect
npm run signing:test
npm run signing:verify -- -FilePath "安装器的完整路径"
```

inspect 不写文件，只报告工具和指定证书是否就绪；证书尚未配置时返回 certificate_ready=false。verify 会对未签名、无时间戳、不受信任或被篡改的文件返回非零退出码。

signing:test 为本地及 CI 发布契约检查，使用当前 Rust 工具链生成原创未签名 EXE，不再依赖历史候选包。它验证微软已签名文件、篡改副本、错误发布者及未签名原创文件等情形，不借用微软证书签署 OpenNovel。

## 正式证书就绪之后

当前实现适用于通过 Windows 证书库暴露私钥提供者的 USB Token / HSM / 云虚拟令牌客户端。比如 Certum SimplySign Desktop 或 SSL.com eSigner CKA；客户端由用户在官方渠道安装并本人登录，秘密值不传给本脚本。[Certum 要求](https://shop.certum.eu/open-source-code-signing-on-simplysign.html)、[SSL.com CKA](https://www.ssl.com/how-to/how-to-install-ssl-com-esigner-cloud-key-adapter-cka/)。

先从证书管理器确认属于本人且获得授权的证书，复制其 40 位十六进制指纹，然后设置本机进程环境变量 `OPENNOVEL_SIGNING_CERT_THUMBPRINT`。指纹不是私钥；不要设置 PFX 密码或把任何秘密写入脚本。

```powershell
npm run signing:inspect
npm run desktop:package:signed
```

签名入口会先检查证书、Code Signing EKU、有效期、私钥提供者和在线信任链，再要求 Git 工作区干净。没有证书时在创建构建目录前停止。

构建使用新的 `.artifacts/signing-build/<版本-UUID>` 目标目录；Tauri 以参数数组调用独立 signer，处理包含空格的路径。安装包内应用、卸载器、安装器由打包过程签名。Tauri 完成后会恢复未签名的独立主程序，所以最后对独立桌面 EXE 和 CLI 分别签名，再验签及计算交付摘要。每次使用 SHA-256 文件摘要与 RFC3161/SHA-256 时间戳；`/sha1` 仅用于按指纹选择证书，并非使用 SHA-1 给文件签名。[微软时间戳建议](https://learn.microsoft.com/en-us/windows/win32/seccrypto/time-stamping-authenticode-signatures)。

`scripts/research/test_signing_orchestration.py` 在临时目录运行发布脚本副本，使用明确标注的原创假构建和假签名器，覆盖独立 EXE 被恢复、签名失败、安装器未签名及卸载器审计缺失。失败不得生成交付目录；成功回执必须使用最终签名后的摘要。它不接触私钥，也不证明正式 CA 签名成功，生产脚本没有增加跳过验签的开关。

签名后逐个用 Authenticode 和 `signtool verify /pa /all /tw /v` 验证，核对指定证书和时间戳。未观察到经过验证的卸载器签名也会停止。成功后才生成独立的 `.artifacts/signed-release/<版本-UUID>` 交付目录及签名后的 SHA-256/manifest，不覆盖 v0.3 原候选包。

仍需实际安装签名包，检查安装后应用及卸载器的签名，并重新执行运行/卸载验收。完整签名构建、云令牌客户端、PIN 提示和签名后安装尚未实测，不能用本轮负例测试称其已完成。

## 中国大陆个人的申请准备

以下是截至本次核实的候选方向，不是保证可签发的推荐或已开通服务。尚未确认大陆证件受理细节；购买前应向官方确认身份材料、总费用和退款条件。

| 路线 | 核实到的条件与限制 |
| --- | --- |
| [SignPath Foundation](https://signpath.org/terms) | 为获批开源项目提供免费签名，证书发布者为基金会。需要可核验的公开项目、发布记录、构建来源、角色和人工批准；新项目不保证通过。当前仓库没有配置 Git 远端，也尚未提供公开项目地址。 |
| [Certum 开源云签名](https://shop.certum.eu/open-source-code-signing-on-simplysign.html) | 页面显示 €49 起，但同时显示缺货，不能当作可立即购买。个人项目需要公开项目及作者关联、身份和地址材料；限非商业开源用途。[材料要求](https://support.certum.eu/en/code-signing-required-documents/) |
| [SSL.com IV](https://www.ssl.com/products/software-integrity/code-signing/iv/) | 个人验证，页面显示一年证书 $129，密钥存储/云签名费用另计。大陆身份和地址材料、云服务资格与最终报价须先确认。[云签名收费](https://www.ssl.com/guide/esigner-pricing-for-code-signing/) |
| [Azure Artifact Signing](https://learn.microsoft.com/en-us/azure/artifact-signing/quickstart) | 当前公众信任的个人身份验证限美国/加拿大，不适用于已说明的中国大陆个人条件。 |

本地 CA 工作流不等于 SignPath 云签名接入。若选择基金会路线，还需公开仓库和下载页、明确维护者/审核者名单、隐私及 Code signing policy、申请审核，并按获批的项目配置接入构建签名。CLI 当前缺少 Windows ProductName/版本资源，也需在该路线审核前补齐。不要在未获批前声称已获得基金会签名。

## 可提交给官方的售前问题

以下文字仅为用户自行提交的草稿，本项目没有向供应商发送消息或上传材料：

> I am an individual developer residing in mainland China. I maintain OpenNovel Downloader, an Apache-2.0 Windows desktop application for local book management and downloading supported public web chapters. Before ordering, please confirm whether you accept my country of residence and identity/address documents, the complete first-year and renewal cost including cloud key storage and signing usage, the Windows SignTool integration method, and the refund policy if identity verification fails. Please specify your secure portal for submitting documents. The public repository URL will be supplied once available.

项目发布、真实主体信息、证件、供应商账户及付款由用户确认和提供。代码签名不自动授权 Git push、公开发布或任何收费操作。

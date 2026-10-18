# GitHub 自动更新

自 1.1.0 起，Windows 便携版支持应用内更新，无需安装包。

## 使用方式

在「设置 → 软件更新」中检查版本，查看更新说明，下载新版，再点击「重启并更新」并确认。下载和重启均由用户主动触发，关闭窗口或退出助手不会偷偷安装。

自动检查默认开启：启动约 30 秒后检查，此后每 24 小时检查一次；失败后延迟重试。同一版本最多发一次系统通知。可选择「暂不提醒此版本」，手动检查仍可重新看到它。

下载支持进度显示和取消。下载成功但暂不更新时，已校验的程序保存在本地，下次启动仍可继续更新。仅接受正式版，不安装预发行版或旧版本。

**1.0.0 需要手动升级到 1.1.0 一次**，因为旧版没有更新器。以后带有签名附件的正式版本才能在应用内更新。

## 网络

更新模块通过独立的 Windows WinHTTP 会话访问 GitHub。默认使用 `WINHTTP_ACCESS_TYPE_AUTOMATIC_PROXY`，由 Windows 应用系统及当前用户的代理配置，包括手动代理、PAC、自动发现和代理绕过规则。每次检查和下载创建新会话，代理设置变更后可直接重试。另提供明确的「直接连接」选项。

Windows 负责按系统策略选择代理和处理代理故障；应用不会在请求失败后自行改用直连再试。详情见 [Microsoft WinHttpOpen 文档](https://learn.microsoft.com/en-us/windows/win32/api/winhttp/nf-winhttp-winhttpopen)。

仅允许 HTTPS、443 端口以及固定 GitHub API、发布页和资产域名。每次重定向重新校验地址，最多五次。请求有超时、响应大小限制；不会向 GitHub 发送校园网账号、密码、Cookie 或设备档案，也不使用 GitHub 登录凭据。

校园网请求继续使用原有 Rust 客户端的 `.no_proxy()`，与更新会话独立。系统 VPN/TUN 或路由本身仍可能影响实际网络路径。

GitHub 检查使用 ETag 缓存；发生限流时推迟自动重试。界面会显示失败原因，不把网络错误显示为“已是最新”。

## 签名与发布

每个新版本发布五个附件：便携 ZIP、独立 EXE、`SHA256SUMS.txt`、`update.json` 和 `update.json.sig`。

`update.json` 的原始 UTF-8 字节使用 Ed25519 签名，内容绑定项目名、版本、标签、最低更新器版本、Windows x64 下载地址、字节数和 SHA-256。客户端使用 `src-tauri/update-public-key.txt` 中的公钥，先验签，再检查版本、固定仓库地址、文件大小和摘要。安装助手会再次验证。

发布任务运行 `scripts/sign-update.cjs`，从 GitHub Actions 的 `UPDATE_SIGNING_KEY` Secret 读取私钥。签名密钥与客户端公钥不匹配时停止发布。私钥不进入源码、Actions 构建产物或发行附件；此处的更新签名独立于 Windows Authenticode 代码签名。

维护者首次配置的私钥备份位于本地忽略目录 `.local/update-signing.key`，限制为当前 Windows 用户访问。请妥善离线备份；不要随意更换公钥，否则旧客户端无法验证新版。丢失或更换密钥需要额外的迁移方案。

## 替换与回退

V1.2.0 起，更新文件下载到用户目录 `%LOCALAPPDATA%\cn.ruc.webhelper\.updates\download.part`，校验后成为 `new.exe`。更新助手和旧版备份保存在同一用户目录。原进程退出后，助手校验目标仍为原应用，在 EXE 所在文件夹创建临时替换文件，再通过 Windows `ReplaceFileW` 替换，完成后清理临时文件。这样 EXE 与用户目录位于不同磁盘时也能更新，维持原程序文件名和路径。此暂存满足 [ReplaceFileW 的同卷要求](https://learn.microsoft.com/en-us/windows/win32/api/winbase/nf-winbase-replacefilew)。

新版完成桌面初始化后发送启动确认。启动失败或 60 秒内未确认，助手尝试恢复旧程序并重新打开；替换失败则报告错误。只替换 EXE，保留账号、设置、设备档案和 WebView 数据，不主动注销校园网。更新目录不允许链接或联接点。

启动确认只覆盖初始化阶段，不能判断新版的所有功能是否正常；断电、磁盘故障或杀毒软件占用可能仍需手动恢复。异常时检查用户数据目录下的 `.updates/result.txt`，如保留了 `previous.exe`，可退出相关进程后手动恢复。助手没有后台提权能力，应用内替换时 EXE 所在文件夹必须可写；也可下载新版 EXE 手动替换，用户数据不受影响。从旧版自动更新时保留旧更新任务的启动确认兼容。

## 验证

默认 Rust 测试覆盖签名篡改、错误公钥、降级/预发行版本、摘要和大小错误、下载域名约束，以及 Windows 文件替换和恢复。界面测试使用虚构后端，检查代理选项、更新提示、说明安全显示和重启确认。

只读真实 GitHub 连接检查：

```powershell
cargo test --manifest-path src-tauri/Cargo.toml --locked live_github_system_proxy -- --ignored
```

独立目录中的完整安装/回退测试见 `tests/manual/update-install.cjs`。它仅启动测试程序，不访问校园网，不操作日常使用目录；需要本地签名密钥，默认 CI 不运行。

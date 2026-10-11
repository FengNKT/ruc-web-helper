# 开发指南

## 环境与构建

本项目目前面向 Windows，使用 Node.js 22.12+、Rust 稳定工具链、Microsoft C++ Build Tools（MSVC 与 Windows SDK）和 WebView2 Runtime。

```powershell
npm ci
npm run desktop:dev
```

`npm run dev` 仅运行网页预览，无法直接访问桌面接口。`npm run desktop:build` 构建便携 EXE，不生成安装包。

前端为 Vite + JavaScript，入口是 `index.html` 和 `src/main.js`。Rust 后端位于 `src-tauri/src/`，负责 HTTPS 接口、DPAPI 凭据、设备档案及桌面集成。`src-tauri/gen/schemas/` 由 Tauri 生成，不提交。

保留 `package-lock.json` 和 `src-tauri/Cargo.lock`，不要将依赖目录或构建缓存提交到仓库。运行 `npm run version:set -- 1.0.1` 可同步全部版本字段，`npm run version:check` 可验证一致性。GitHub 构建与版本发布流程见 [发布指南](releasing.md)。

## 自动检查

```powershell
npm test
npm run test:ui
npm run test:rust
```

- `tests/unit/`：流量额度、格式化及会话分配单元测试。
- `tests/e2e/`：使用虚构后端的界面测试，需要 Microsoft Edge。
- Rust 单元测试：协议向量、本地 HTTP 登录流程、加密文件和设备档案测试。
- `tests/fixtures/`：仅包含虚构账号、密码、IP 和 token 的测试向量，不可用于真实登录。

自动检查不执行真实校园网登录、注销或设备下线。截图统一写入 `test-results/e2e/`。Windows 凭据测试只操作临时目录中的虚构凭据。

## 真实环境手动检查

`tests/manual/` 不属于默认测试命令，也不应加入无人值守 CI。运行前需自行启动待测应用，并临时将 WebView2 调试端口限定为 `127.0.0.1:9227`。结束后关闭调试应用，以普通方式重新启动；普通程序默认不开放该端口。

| 脚本 | 行为与要求 |
| --- | --- |
| `native-smoke.cjs` | 读取现有会话、设备与统计，可能更新本地发现档案；不改变连接、账号或设置 |
| `registry-native.cjs` | 检查在线与历史设备合并，可能增量更新本地档案 |
| `native-theme.cjs` | 临时更改本地主题并恢复，要求 `RUC_DELIVERY_DIR` 和 `RUC_ALLOW_LOCAL_TEST_MUTATIONS=1` |
| `device-names-native.cjs` | 临时修改本地设备名称并恢复，要求 `RUC_ALLOW_LOCAL_TEST_MUTATIONS=1` |
| `login-cycle-native.cjs` | 会断开并重新登录本机校园网，要求明确授权及 `RUC_ALLOW_NETWORK_DISCONNECT=1` |

`RUC_DELIVERY_DIR` 应指向待测程序及运行数据所在目录。手动检查截图写入 `test-results/manual/`，可能含账号、MAC、IP 或用量，不能作为公开文档图片。部分手动检查需要现有在线会话、已保存凭据或可用历史记录。

例如，只读检查可从项目根目录运行：

```powershell
$env:RUC_DELIVERY_DIR = '待测程序目录的绝对路径'
node tests/manual/native-smoke.cjs
Remove-Item Env:RUC_DELIVERY_DIR
```

手动脚本中的显式开关仅用于防止误运行，不代表授权来源。日常开发使用虚构后端完成验证。

## 图标

可编辑源文件是 `src-tauri/icons/ruc-web-helper.svg`。已有的 `icon.png`、`icon.ico` 可直接用于构建。

如需重新导出，安装本机 Edge、Python 和 Pillow 后运行 `npm run icons`。脚本同步 `public/app-icon.svg`，更新构建图标，并将高清预览与其他尺寸写入 `test-results/icons/`。

## 贡献

README 图片使用真实前端与虚构数据生成。安装本机 Edge 后运行 `npm run docs:screenshots`，图片输出至 `docs/images/`，随文档提交。生成脚本仅允许访问本地预览，不读取本地账号文件，也不连接校园网。

提交变更时说明用户可见的行为及验证方式。问题报告和截图请使用虚构或脱敏数据，不附带账号文件、设备档案、WebView2 缓存、真实密码或带登录参数的 URL。新增测试也应使用虚构数据。

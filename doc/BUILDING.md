# 生产构建与打包

如何把 cc-monitor 编译成可分发的 NSIS / MSI / `.exe`。

开发环境 / dev mode → [DEVELOPMENT.md](DEVELOPMENT.md)。发版 release SOP → [RELEASING.md](RELEASING.md)。

---

## 构建命令

```powershell
powershell -NoProfile -File scripts\run.ps1 build
# 等价于（在已注入 vcvars 的 PS 里）：
npx tauri build
```

约 3-8 分钟。输出（`<version>` 是 `tauri.conf.json` 里的版本号）：

```
src-tauri/target/release/
├── cc-monitor.exe                                       ← 主程序（需 WebView2）
└── bundle/
    ├── msi/
    │   └── cc-monitor_<version>_x64_zh-CN.msi           ← Windows Installer 包
    └── nsis/
        └── cc-monitor_<version>_x64-setup.exe           ← NSIS Setup 安装器
```

---

## 产物对比

| 格式 | 体积（约） | 适合 |
|---|---|---|
| **cc-monitor.exe** | ~10 MB | 开发者本机；需 WebView2 已装 |
| **MSI** | ~5 MB | 企业 IT 部署、Windows 11 原生 |
| **NSIS Setup** | ~5 MB | 普通用户双击安装、体积小 |

体积关键：`Cargo.toml::[profile.release]` 已配 `opt-level = "z"` + `lto = true` + `strip = true` + `codegen-units = 1` + `panic = "abort"`。

---

## NSIS 配置

`src-tauri/tauri.conf.json::bundle.windows.nsis` 关键字段：

- **installMode**: `perMachine`（默认）→ 装到 `C:\Program Files\cc-monitor\` 需管理员
- 改 `perUser` → 装到 `%LOCALAPPDATA%`，不需要管理员，但每个 user 各自一份
- **displayLanguageSelector**: `false`（用户安装时不弹语言选择，默认中文）
- **languages**: `["SimpChinese", "English"]`（NSIS 包内嵌的两种语言资源）

---

## MSI 配置

通过 WiX 工具链生成。Tauri 首次构建会自动下载 WiX 到 `%LOCALAPPDATA%\tauri\WixTools3`，网络不通时手装。

MSI **企业部署友好**：
- 静默安装：`msiexec /i cc-monitor_<ver>_x64_zh-CN.msi /qn`
- 卸载：`msiexec /x cc-monitor_<ver>_x64_zh-CN.msi /qn`
- 适合 Intune / SCCM / Group Policy

---

## WebView2 Runtime

**默认不内置**（cc-monitor.exe 启动时检查系统是否已装 WebView2 Runtime）。

- Win11 自带 WebView2 → 用户无需任何操作
- Win10 用户首次启动报"WebView2 Runtime not found" → 需要自己装

**要内置 Bootstrap installer**（首次启动自动下载安装）：在 `src-tauri/tauri.conf.json::bundle.windows` 加：

```json
"webviewInstallMode": {
  "type": "downloadBootstrapper",
  "silent": true
}
```

注意会让安装包变大 + 首次启动需网络。当前**默认不内置**理由：99% 目标用户在 Win11；Win10 用户少且通常装过 Edge → Edge 自动装 WebView2。

---

## Code Signing（未启用）

未签名 exe 首次启动被 Windows SmartScreen 拦"未知发布者"，用户得点「更多信息 → 仍要运行」。

要消除这个警告，需要 Code Signing 证书：

- **OV 证书**（Organization Validation）≈ $200/年，几小时到几天去除"未知发布者"警告
- **EV 证书**（Extended Validation）≈ $400/年，立即去 SmartScreen 警告（最佳）
- **证书来源**：DigiCert / Sectigo / GlobalSign / SSL.com

接入位置：

```json
// src-tauri/tauri.conf.json
{
  "bundle": {
    "windows": {
      "certificateThumbprint": "<SHA1 thumbprint of code signing cert>",
      "digestAlgorithm": "sha256",
      "timestampUrl": "http://timestamp.digicert.com"
    }
  }
}
```

证书 thumbprint 用 `Get-ChildItem Cert:\CurrentUser\My | Format-List Thumbprint, Subject` 查。

当前 v1.x 决定**不签名**：cc-monitor 是社区工具，签名成本不值得；用户首次运行点一下"仍要运行"可接受。

---

## 典型构建错误

| 错误 | 原因 | 修复 |
|---|---|---|
| `linker link.exe not found` | 没注入 vcvars | 用 `scripts\run.ps1 build` 而非 `cargo build` |
| `Microsoft Visual C++ 14.0 is required` | 缺 MSVC 或缺 VCTools workload | VS Installer 加 workload |
| 卡在 `Compiling cc-monitor` | Rust 首次编译慢 ~5 min | 等。后续增量 < 1 min |
| NSIS `MakeNSIS exited with code 1` | 图标 `.ico` 损坏 / 路径含中文 | 检查 `src-tauri/icons/icon.ico` |
| MSI 报 `WiX is not installed` | Tauri 自动下载到 `%LOCALAPPDATA%\tauri\WixTools3`；网络不通时手装 | 检查网络或手装 WiX |
| `EACCES: permission denied ::1:24174`（或任意 dev 端口） | dev 端口被 Hyper-V 保留 | 这是 dev 错误不是 build 错误 → [DEVELOPMENT.md § 端口冲突](DEVELOPMENT.md#端口冲突) |

---

## 体积优化

当前 release profile 已经把能开的开关都开了：

```toml
[profile.release]
opt-level = "z"      # 优化 size，不优化 speed
lto = true           # link-time optimization
strip = true         # 删 debug symbols
codegen-units = 1    # 牺牲编译速度换最优 codegen
panic = "abort"      # panic 不 unwind，省 ~100KB
```

进一步优化方向（如果未来 monitor.exe > 20MB）：
- 移除 `tauri-plugin-dialog` 如果不用 file picker
- `tracing` → `log` + `env_logger` 省 ~3MB
- 替换 `marked` + `katex` + `hljs` 为更轻的 markdown 库（可能影响渲染效果）

**当前体积 ~10MB**，相对 Electron 同等功能 100+ MB 已经很小，不优先优化。

# 构建与打包

怎样把 cc-monitor 编成可分发的包：Windows 的 NSIS 安装器 · MSI · 裸 `cc-monitor.exe`，Linux 的 `.deb` · 裸 `cc-monitor`。发版产物由 `.github/workflows/release.yml` 在 CI 上编，本篇讲它由哪几件组成、本机怎么编出同样的东西。

开发环境与 dev 模式 → [DEVELOPMENT.md](DEVELOPMENT.md)。发版流程 → [RELEASING.md](RELEASING.md)。

---

## 一个安装包由哪几件组成

| 件 | 是什么 | 怎么来 |
|---|---|---|
| 前端 | 三个入口页（主窗 · 设置 · 只读窗）的 JS / CSS | `npm run build`（`tsc && vite build`），产物在 `.build/dist/` |
| 远端用的字节 | 后端的 musl 静态二进制，x86_64 / aarch64 各一份，编进 monitor，第一次连上远端时部署过去 | `cargo zigbuild --release --locked --target <arch>-unknown-linux-musl`，铺进 `src/frontend/shell/embedded-backends/` |
| 本机后端 | 本机原生 target 的后端，随包带上，本机起它 | `cargo build --release --locked`，发版时作 `externalBin` 注入，并铺进 `src/frontend/shell/native-backend/` 供自释放 |
| 文件窗口程序 | monitor 包的第二个二进制 `cc-monitor-filewin`，安装包把它装在主程序旁；也编进 monitor，单文件版开窗时放到 `~/.cc-monitor/bin/` 再起 | 先 `cargo build --release --locked --bin cc-monitor-filewin`、铺进 `src/frontend/shell/native-backend/`，再编 monitor（同一趟 cargo 里 monitor 嵌不到它） |
| monitor 本体 | Tauri 壳 ＋ 打包 | `npx tauri build`（先跑上面的 `npm run build`，再 `cargo build --release`，最后打包） |

- 那三个落点目录都在 `.gitignore` 里，干净 clone 里没有。没有时 monitor 照样编得过，只是那一份能力诚实关闭（不能部署远端 / 起不了本机后端），不会编出一份假装能用的包。
- `externalBin` 只写在 `src/frontend/shell/tauri.sidecar.conf.json` 里，打包那一步用 `--config` 注入；不进主配置，否则 `cargo test` 也要求当前 target 的二进制在场。
- Cargo 的输出不落在源码树里：壳在 `.build/shell/`，后端在 `.build/backend/`（各自的 `.cargo/config.toml` 定）。

### 内嵌字节与 `BUILD_ID`

后端的身份是 `src/backend/lib.rs` 里的 `BUILD_ID`，编进字节里的身份戳。monitor 编译时（`src/frontend/shell/build.rs`）逐份读内嵌字节的戳，与源码对不上就让编译失败——装出去一份自报旧身份的后端，monitor 会永远判它过期、无限重装。

所以 bump 了 `BUILD_ID` 就同拍重铺内嵌字节：

```bash
bash tests/scripts/re-embed.sh            # 重编两个 musl arch 并铺回落点（要 cargo-zigbuild 与 zig）
bash tests/scripts/re-embed.sh --native   # 编本机原生后端与文件窗口程序，铺进 native-backend/
bash tests/scripts/re-embed.sh --check    # 只查：盘上的字节与源码的 BUILD_ID 对不对得上，不产字节
bash tests/scripts/re-embed.sh --clean    # 删掉落点：自动部署诚实关闭，编译立刻恢复
```

配方与 `release.yml` 产字节那一步同源（门禁 `release-gate` 那一格对拍）。本机编出来的字节够开发期自洽，但不等于发版产物（发版用钉死版本的 zig / cargo-zigbuild）。要在本机编一份起得来本机后端、部署得了远端的包，先跑前两条再打包。

---

## 构建命令

### Windows

要 Visual Studio Build Tools 2022（VCTools workload）与 WebView2 Runtime。Tauri 在 Windows 上要靠 MSVC 的 `link.exe` 链接，当前 shell 没注入 vcvars 时会链到 Git Bash 带的同名工具、崩在链接阶段。

```powershell
# 在 Developer PowerShell for VS 2022 里（已注入 vcvars）：
npm run build
npx tauri build --config src/frontend/shell/tauri.sidecar.conf.json --bundles nsis,msi
```

`tests\scripts\run.ps1 build` 会用 `vswhere.exe` 找到 MSVC、注入环境后跑 `npx tauri build`，免去手开 Developer PowerShell。用 sidecar 配置打包前，先把本机后端铺到 `src/frontend/shell/binaries/cc-monitor-backend-<host triple>.exe`（`release.yml` 的 `Stage local backend for externalBin` 那一步照做即可）；只想要一份本机自用的包，去掉 `--config …` 那一段。

### Linux（x86_64）

要 `libwebkit2gtk-4.1-dev` · `libgtk-3-dev` · `libayatana-appindicator3-dev` · `librsvg2-dev`（Debian / Ubuntu 包名，与 CI 同）。

```bash
npm run build
npx tauri build --bundles deb
```

发版那一趟另外带 `--config src/frontend/shell/tauri.sidecar.conf.json`，并先把本机后端铺到 `src/frontend/shell/binaries/cc-monitor-backend-<host triple>`。

### 产物

在 `.build/shell/release/`：

```
.build/shell/release/
├ cc-monitor(.exe)                                  裸二进制（主 `[[bin]]` 的名字；包名仍是 monitor）
├ cc-monitor-filewin(.exe)                          文件窗口程序（安装包装在主程序旁；发版时也编进上面那一个）
└ bundle/
  ├ nsis/cc-monitor_<version>_x64-setup.exe         NSIS 安装器
  ├ msi/cc-monitor_<version>_x64_en-US.msi          MSI（WiX 没配语言 ⇒ en-US）
  └ deb/cc-monitor_<version>_amd64.deb              Linux 包
```

`<version>` 是 `src/frontend/shell/tauri.conf.json` 里的 `version`。CI 另算校验和 `SHA256SUMS.txt`（Windows）与 `SHA256SUMS-linux.txt`（Linux）。

---

## 打包配置

### NSIS

`src/frontend/shell/tauri.conf.json` 的 `bundle.windows.nsis`：

- `installMode: perMachine`：装到 `C:\Program Files\cc-monitor\`，要管理员；改 `perUser` 装到 `%LOCALAPPDATA%`，不要管理员，每个用户各一份。
- `displayLanguageSelector: false` ＋ `languages: ["SimpChinese", "English"]`：安装时不弹语言选择。

### MSI

由 WiX 工具链生成，Tauri 第一次构建时下载到 `%LOCALAPPDATA%\tauri\WixTools3`，网络不通时手装。适合 Intune / SCCM / 组策略：

```powershell
msiexec /i cc-monitor_<version>_x64_en-US.msi /qn   # 静默安装
msiexec /x cc-monitor_<version>_x64_en-US.msi /qn   # 静默卸载
```

### WebView2 Runtime

默认不内置：Windows 11 自带；Windows 10 大多随 Edge 装过，没有时首次启动报 `WebView2 Runtime not found`。要在首次启动时自动下载安装，在 `bundle.windows` 里加：

```json
"webviewInstallMode": { "type": "downloadBootstrapper", "silent": true }
```

代价是首次启动要联网。

### 代码签名

不签名：首次运行时 SmartScreen 拦「未知发布者」，用户点「更多信息 → 仍要运行」。要签就买 OV / EV 代码签名证书，在 `bundle.windows` 里填：

```json
"certificateThumbprint": "<证书 SHA1 指纹>",
"digestAlgorithm": "sha256",
"timestampUrl": "http://timestamp.digicert.com"
```

指纹用 `Get-ChildItem Cert:\CurrentUser\My | Format-List Thumbprint, Subject` 查。

### 体积

`src/frontend/shell/Cargo.toml` 的 `[profile.release]` 已经开到头：`opt-level = "z"` · `lto = true` · `strip = true` · `codegen-units = 1` · `panic = "abort"`。

---

## 常见构建错误

| 报错 | 原因 | 办法 |
|---|---|---|
| `linker link.exe not found` / 链接阶段崩 | 没注入 vcvars | 在 Developer PowerShell 里跑，或用 `tests\scripts\run.ps1` |
| `Microsoft Visual C++ 14.0 is required` | 缺 MSVC 或 VCTools workload | VS Installer 里加 workload |
| 编译期 panic「内嵌 backend … 问不出身份」/「半 bump」 | 内嵌字节与源码 `BUILD_ID` 对不上 | `bash tests/scripts/re-embed.sh`（重铺）或 `--clean`（删掉落点） |
| `MakeNSIS exited with code 1` | 图标损坏或路径问题 | 检查 `src/frontend/shell/icons/icon.ico` |
| `WiX is not installed` | 自动下载失败 | 检查网络或手装 WiX |
| `EACCES: permission denied ::1:24174` | dev 端口落进了 Windows 的保留段 | 是 dev 的问题，见 [DEVELOPMENT.md § 端口冲突](DEVELOPMENT.md#端口冲突) |

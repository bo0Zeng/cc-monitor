# 开发环境与调试

本机起 cc-monitor dev 模式 + 调试技巧。

生产构建 → [BUILDING.md](BUILDING.md)。发版 → [RELEASING.md](RELEASING.md)。

---

## 前置依赖

| 工具 | 版本 | 检查 |
|---|---|---|
| **Node.js** | LTS (≥ 18) | `node -v` |
| **Rust** | stable (≥ 1.75) | `rustc --version` |
| **MSVC Build Tools 2022** | 含 VCTools workload | `where link.exe` 应找到 MSVC 的 link |
| **WebView2 Runtime** | Win11 自带 / Win10 [手装](https://developer.microsoft.com/microsoft-edge/webview2/) | — |

`scripts/run.ps1` 走 `vswhere.exe` 自动找 MSVC（非默认路径也行），无需手动 vcvars。

---

## 起 dev server

```powershell
cd D:\path\to\cc-monitor

npm install                                              # 一次性
powershell -NoProfile -File scripts\run.ps1 dev          # 弹 1100x800 窗口
```

直接 `npx tauri dev` 也行，**前提是当前 PowerShell 已注入 vcvars**（否则 link.exe 找的是 Git Bash 的 GNU coreutils 假冒，编译挂）。

### dev 模式的特殊行为

- **自动打开 DevTools**（`lib.rs::setup()` 内 `#[cfg(debug_assertions)] window.open_devtools()`），可看前端 console
- **HMR**：保存 `src/` 下 TS / CSS 会触发 vite HMR 自动刷新前端；保存 `src-tauri/` 下 Rust 会触发增量 cargo build + 重启 monitor
- **`main.ts` 强制 full reload**：HMR 检测到任何 TS 改动直接 `location.reload()`，不做部分热替换。避免长跑监控时旧 listener 与新代码并存导致消息重复 / event_replay 状态不一致

### 其它常用命令

```powershell
powershell -NoProfile -File scripts\run.ps1 check    # cargo check
powershell -NoProfile -File scripts\run.ps1 clean    # cargo clean
powershell -NoProfile -File scripts\run.ps1 build    # 生产构建（详 BUILDING.md）
```

`scripts/run.ps1` 子命令清单 → [`../scripts/README.md`](../scripts/README.md)。

---

## 端口冲突

### 默认 dev 端口 5174

如果启动报：
```
Error: listen EACCES: permission denied ::1:5174
```

**原因**：Windows 把这个端口加到了 Hyper-V / WSL2 / WinNAT 动态保留范围，应用层无法 bind。netstat 看不到占用进程但 listen syscall 失败。

**确认 + 找可用端口**：

```powershell
function Test-Port($addr, $port) {
    try {
        $l = New-Object System.Net.Sockets.TcpListener($addr, $port)
        $l.Start(); $l.Stop(); "OK"
    } catch { "FAIL: $($_.Exception.Message)" }
}
"IPv4 5174 : " + (Test-Port ([System.Net.IPAddress]::Loopback) 5174)
"IPv4 5400 : " + (Test-Port ([System.Net.IPAddress]::Loopback) 5400)
"IPv4 3000 : " + (Test-Port ([System.Net.IPAddress]::Loopback) 3000)
```

如果 5174 不行换一个端口跑：

```powershell
$env:VITE_PORT = "5400"
# 临时改 src-tauri/tauri.conf.json 的 devUrl 到同端口
# **不要 commit 这个改动**，测完改回 5174
powershell -NoProfile -File scripts\run.ps1 dev
```

**真正的根因解决**：
- `Restart-Service winnat`（管理员 PS）会释放部分端口，但影响 Docker / WSL 网络，慎用
- 或者重启电脑

vite.config.ts 已支持 `VITE_PORT` env override，HMR 端口自动设为 `VITE_PORT + 1`。

---

## DevTools 用法

### 看前端 console

dev mode 自动开 DevTools。生产 build 没 DevTools（`tauri.conf.json` 默认禁用）。

调试前端 bug 必须先起 dev mode。

### 查 IPC 调用

DevTools Network tab 看不到 Tauri IPC（不走 HTTP）。要看 IPC：
- 在 ts 端 `invoke()` 前后加 `console.log`
- Rust 端 `tracing::info!`（dev 模式 stdout 可见；生产 build 看不到，issue #4 会加 log 文件）

### 查 capability 报错

报 `Permission xxx not allowed`：

1. 看 `src-tauri/gen/schemas/acl-manifests.json` 确认 plugin 的 permission set 实际内容
2. 看 `src-tauri/capabilities/default.json` 当前 grant 了哪些
3. 通常需要加 inline scoped permission，详 [CONTRIBUTING § 2.6](CONTRIBUTING.md#26-添加新-tauri-capability-permission)

---

## 跑测试

```powershell
cd src-tauri
cargo test --lib          # 全部单元测试
cargo test --lib profile_installer    # 单个模块
cargo test --lib -- --nocapture       # 看 println! 输出
```

前端**当前没有自动化测试套件**（v1.7 范围内）；改前端靠 dev mode 手测 + DevTools。

---

## 后端日志（dev mode）

`scripts/run.ps1 dev` 启动后，dev shell 的 stdout 会显示：

- vite dev server log
- cargo build 进度
- monitor.exe 自己的 `tracing::*!` 输出（默认 INFO 级）

调级别：

```powershell
$env:RUST_LOG = "debug"; powershell -NoProfile -File scripts\run.ps1 dev
# 或更细：
$env:RUST_LOG = "monitor=debug,tauri=warn"; ...
```

生产 build 没 stdout（`windows_subsystem = "windows"`）→ 看不到 tracing 输出。**已在 v2.0.0+ 实现**：tracing 输出到 `<monitor_data_dir>/logs/monitor.YYYY-MM-DD.log` 文件 + 设置面板 → 诊断区可调日志级别 + ERROR 级 toast 反馈。详 `src-tauri/src/logging.rs` + 设置面板。

---

## 调试技巧 / 常见问题

### Tab 不出现
- 跑 `claude` 后 `~/.claude/sessions/` 应该有新 `<PID>.json` 文件
- 没有：claude 启动有问题
- 有但 Tab 不出现：dev console 看 `jsonl-line` / `jsonl-batch` 事件是否到前端

### cc 集成握手不成功
- `~/.claude/claudecode-frontend/ps-await/<PID>.json` 写了又被删 → monitor 收到了
- 写了**没被删** → monitor 没 watch 到 / parse 失败
- 看 dev stdout 是否有 `bind: parse ... failed`

### profile 写入失败
- 设置面板 [安装] 后看 alert 报错
- 检查 `~/.claude/claudecode-frontend/...` 下不存在（v1.7.10 的"防御性 abort"会触发）
- 检查目标 profile 路径是否真存在 + 权限

### tooltip 不显示 / 显示在错位置
- DevTools Elements tab 看 `.settings-info-tooltip` 是否真挂到 `<body>` 末尾
- 看 inline style 的 `left/top` 值是否合理（不应大于 `window.innerWidth` 或负数）
- 详见 [ARCHITECTURE.md § 5 CSS portal](ARCHITECTURE.md#5-关键设计选择--理由) 的设计理由

### Web 字体 / 图片 / KaTeX 加载导致消息错位
- `stream.ts` 的 `MessageStream` 用 `ResizeObserver` 应对，append 后下一帧自动校准滚动位置
- 如果错位仍发生，DevTools 看是否真的触发了 ResizeObserver 回调

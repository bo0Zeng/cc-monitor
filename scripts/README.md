# `scripts/` 目录

| 脚本 | 作用 | 状态 |
|---|---|---|
| [`run.ps1`](run.ps1) | 自动注入 MSVC dev shell 环境后跑 tauri 命令 | ⭐ 活跃使用 |
| [`session-register.ps1`](session-register.ps1) | Claude Code `SessionStart` hook，曾用于注册活跃 session | ❌ 已废止 |

---

## run.ps1

### 用途

Tauri 在 Windows 上要靠 MSVC link.exe / cl.exe 编译 Rust 后端。如果当前 PowerShell 没注入 vcvars，会出现各种诡异错误（最常见：链接到 Git Bash 的 GNU coreutils 假冒 link，崩在链接阶段）。

`run.ps1` 通过 `vswhere.exe` 自动定位 MSVC 安装位置，调 `Launch-VsDevShell.ps1` 注入 PATH/LIB/INCLUDE，然后跑 tauri 命令。

### 用法

```powershell
powershell -NoProfile -File scripts\run.ps1 [dev|build|check|clean]
```

| 子命令 | 等价于 |
|---|---|
| `dev` | `npx tauri dev`（弹 1100x800 窗口，HMR） |
| `build` | `npx tauri build`（产 msi + nsis + exe） |
| `check` | `cargo check`（不编 release） |
| `clean` | `cargo clean`（清 `src-tauri/target/`） |

### 前置依赖

- **Visual Studio Build Tools 2022** + **VCTools workload**（提供 link.exe / cl.exe / Windows SDK）
- `vswhere.exe` 默认在 `C:\Program Files (x86)\Microsoft Visual Studio\Installer\vswhere.exe`（VS 安装时自动放）

脚本会在缺失依赖时抛错并提示安装。

### 工作原理

```powershell
1. vswhere.exe -latest -requires Microsoft.VisualCpp.Tools.HostX64.TargetX64
   → 找到 MSVC 安装路径，比如 D:\Microsoft Visual Studio\2022\BuildTools

2. & "<install>\Common7\Tools\Launch-VsDevShell.ps1" -SkipAutomaticLocation -Arch amd64
   → 注入 PATH / LIB / INCLUDE / INCLUDE 等环境变量到当前 PS session

3. Set-Location <repo root>
4. npx tauri $Cmd
```

---

## session-register.ps1

### 状态：❌ 已废止

最初的 M2 计划是在 `~/.claude/settings.json` 安装一个 `SessionStart` hook 调这个脚本，让脚本写 `~/.claude/claudecode-frontend/sessions.json` 来登记活跃 session。

**变更后**：Claude Code 自己在 `~/.claude/sessions/<PID>.json` 维护活跃 session 状态，monitor 直接读，**无需 hook，无需安装，零侵入**。详 [`../src-tauri/README.md`](../src-tauri/README.md) 的 IPC / session_map 模块说明。

### 为什么仍保留？

- 备查（git 历史也可以）
- 若未来 Claude Code 改变 sessions 维护方式，可能要重启用类似机制
- 已加 `$env:CLAUDE_CONFIG_DIR` 兜底（与 paths.rs 一致），将来若重新启用不用改

### 不要安装它

`~/.claude/settings.json` 里没有 SessionStart hook 指向本脚本，**不要手动加**。当前 cc-monitor 不依赖。

# monitor

Claude Code CLI 的只读输出渲染窗口（Tauri 2 + Vanilla TS）。

设计文档：`../doc/`

## 当前状态

**M0 脚手架**——9 个 Rust 模块 + 前端模块都是 stub（`todo!()` / 空函数），仅启动一个空窗口。下一步从 M1（watcher + 解析 + 单 Tab 渲染）开始填空。

## 前置依赖

| 工具 | 用途 | 装哪 |
|---|---|---|
| Node.js LTS | 前端构建 | 全局 |
| Rust (rustup, msvc toolchain) | 后端编译 | 全局 |
| **MSVC Build Tools 2022 + VCTools workload** | link.exe / cl.exe / Windows SDK | 本机装在 `D:\BuildTools`（可换路径） |
| pwsh 7+（M2 后才需要） | SessionStart hook 用 `-AsHashtable` | 全局 |

## 开发

```powershell
cd D:\Sync\文档\claudecode-frontend\monitor

# 1. 装前端依赖（一次性）
npm install

# 2. 起 dev server（弹窗 1100x800）—— 自动注入 MSVC dev shell 环境
powershell -NoProfile -File scripts\run.ps1 dev
```

`scripts\run.ps1` 会通过 `vswhere.exe` 自动找到 MSVC，并调 `Launch-VsDevShell.ps1` 注入 PATH/LIB/INCLUDE。这样不用每次手动 `vcvarsall`。

直接跑 `npx tauri dev` 也行，**前提是当前 PowerShell 已经过 vcvars 注入**（否则 link.exe 找的是 Git Bash 的 GNU coreutils 假冒，编译会挂）。

## 生产构建

```powershell
powershell -NoProfile -File scripts\run.ps1 build
# 产物：src-tauri/target/release/monitor.exe + bundle/{msi,nsis}/
```

## 其它命令

```powershell
powershell -NoProfile -File scripts\run.ps1 check    # cargo check
powershell -NoProfile -File scripts\run.ps1 clean    # cargo clean
```

## 项目结构

```
monitor/
├── src/                         # 前端 (Vanilla TypeScript)
│   ├── main.ts
│   ├── tabs.ts          (M1)
│   ├── stream.ts        (M1/M6)
│   ├── render.ts        (M1/M3)
│   ├── events.ts
│   ├── config.ts
│   ├── cards/           (M1/M3/M4)
│   └── settings/        (M5)
├── src-tauri/
│   └── src/
│       ├── main.rs              # 入口 → lib::run()
│       ├── lib.rs               # Tauri Builder
│       ├── messages.rs          # JSONL enum (已完整)
│       ├── parser.rs    (M1)
│       ├── watcher.rs   (M1)
│       ├── session_map.rs (M2)
│       ├── focus.rs     (M2)
│       ├── bridge.rs            # 事件 schema
│       ├── config.rs    (M5)
│       └── hook_installer.rs (M2)
└── scripts/
    └── session-register.ps1     # SessionStart hook (M2 注册)
```

## 里程碑

| 阶段 | 范围 | 状态 |
|---|---|---|
| M0 | 脚手架 / 空窗口 | ✅ |
| M1 | watcher + 全类型 JSONL 解析 + 基础 Markdown + 单 Tab | ⏳ |
| M2 | SessionMap + SetWinEventHook 焦点 + hook 自动注册 + 进程树穿透 | ⏳ |
| M3 | LaTeX + hljs + tool 卡 + thinking + ai-title | ⏳ |
| M4 | subagent Task 折叠卡 | ⏳ |
| M5 | 设置面板 GUI | ⏳ |
| M6 | 快捷键 / 命令面板 / 归档懒加载 | ⏳ |
| M7 | 打包 + 自动 hook 安装 | ⏳ |

详见 `../doc/技术实现文档.md §10`。

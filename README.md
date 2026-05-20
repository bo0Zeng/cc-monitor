# cc-monitor

Claude Code CLI 的只读输出渲染窗口（Tauri 2 + Vanilla TS，Windows 11 only）。

> 实现状态权威说明见 [`../doc/实现状态.md`](../doc/实现状态.md)。其余设计 / 架构 / 技术实现 / 需求 4 份文档为前期规划稿，**与代码不一致时以实现状态.md + 代码为准**。

## 当前能力（M1–M5 完成、M6 部分完成、M7 未做）

- 监听 `~/.claude/projects/*.jsonl` 实时渲染对话（marked + KaTeX + highlight.js + DOMPurify）
- 多 Tab 自动管理：每个活跃 session 一个 Tab，`[项目名] aiTitle` 标题
- Tool use / tool result / thinking 块折叠卡；Task → subagent 嵌入式懒加载渲染
- 抽屉式外观设置面板（颜色 10 + 字体 3 个 token，热应用）
- F5 / DevTools reload 后由后端 `event_replay` 回放历史（5000 条 cap）
- 快捷键：`Ctrl+Tab` / `Ctrl+Shift+Tab` 切 Tab，`Ctrl+W` 关 archived Tab，`Ctrl+,` 开设置，`Esc` 关设置

## 不做（v1 范围外）

- macOS / Linux 适配（代码不预留跨平台抽象）
- 终端 → Tab 焦点自动同步（Windows 11 默认 WT 单进程多窗口架构无 OS API 支持，详见 `../doc/实现状态.md §2.3`）
- 命令面板 / 全局搜索 / 虚拟滚动 / NSIS 安装器

## 前置依赖

| 工具 | 用途 | 说明 |
|---|---|---|
| Node.js LTS | 前端构建 | 全局 |
| Rust (rustup, msvc toolchain) | 后端编译 | 全局 |
| MSVC Build Tools 2022 + VCTools workload | link.exe / cl.exe / Windows SDK | 本机若装在非默认路径，`scripts/run.ps1` 走 `vswhere.exe` 自动找 |

> 不再需要 PowerShell 7+ —— SessionStart hook 路线已废止（Claude Code 自己写 `~/.claude/sessions/<PID>.json`，monitor 直接 watch）。

## 开发

```powershell
cd D:\Sync\文档\claudecode-frontend\cc-monitor

# 1. 装前端依赖（一次性）
npm install

# 2. 起 dev server（弹窗 1100x800）—— 自动注入 MSVC dev shell 环境
powershell -NoProfile -File scripts\run.ps1 dev
```

`scripts\run.ps1` 通过 `vswhere.exe` 找 MSVC，调 `Launch-VsDevShell.ps1` 注入 PATH/LIB/INCLUDE，避免每次手动 `vcvarsall`。

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
cc-monitor/
├── src/                         # 前端 (Vanilla TypeScript)
│   ├── main.ts                  # 入口、错误捕获、快捷键
│   ├── tabs.ts                  # TabManager：Tab 状态机 + 工具组聚合 + 关闭
│   ├── stream.ts                # MessageStream：ResizeObserver 贴底滚动
│   ├── render.ts                # marked + KaTeX + hljs + DOMPurify
│   ├── events.ts                # 订阅 jsonl-line / session-ended
│   ├── config.ts                # invoke load/save_config 桥
│   ├── theme.ts                 # CSS 变量 token / 启动应用
│   ├── cards/                   # 折叠卡组件
│   │   ├── index.ts             # renderMessage 分发 + tool group
│   │   ├── slash.ts             # / 命令紧凑卡
│   │   ├── compact.ts           # /compact 续接消息折叠
│   │   └── subagent.ts          # Task → subagent 懒加载
│   └── settings/
│       └── panel.ts             # 抽屉式外观设置 GUI
├── src-tauri/
│   └── src/
│       ├── main.rs              # 入口 → lib::run()
│       ├── lib.rs               # Tauri Builder + workers 编排
│       ├── messages.rs          # JsonlRecord enum (覆盖全部 type)
│       ├── parser.rs            # 增量按行解析
│       ├── watcher.rs           # notify 递归监听 ~/.claude/projects
│       ├── session_map.rs       # 读 ~/.claude/sessions/<PID>.json + 探活
│       ├── subagent.rs          # load_subagent IPC + 关联策略
│       ├── event_replay.rs      # 内存 buffer + 5000 条 cap + F5 重放
│       ├── config.rs            # load/save_config + Windows 原子写
│       └── bridge.rs            # Tauri emit 事件 schema
└── scripts/
    └── run.ps1                  # MSVC dev shell 注入 + 跑命令
```

## 里程碑

| 阶段 | 范围 | 状态 |
|---|---|---|
| M1 | watcher + 全类型 JSONL 解析 + 基础 Markdown + 单 Tab | ✅ |
| M2 | SessionMap + 进程探活；焦点自动同步整功能已移除 | ⚠️ 部分（焦点同步 OS 限制无法做） |
| M3 | LaTeX + hljs + tool 卡 + thinking + ai-title | ✅ |
| M4 | subagent Task 折叠卡 + 关联（description 精确匹配） | ✅ |
| M5 | 设置面板 GUI（颜色 + 字体） | ✅ |
| M6 | 快捷键 / 命令面板 / 虚拟滚动 / 性能 | ⚠️ 部分（Ctrl+Tab/W/, ✅；命令面板、虚拟滚动 ❌） |
| M7 | NSIS / MSI 安装器 / 体积优化 | ⚠️ 体积优化 ✅；安装器未配置 |

详见 `../doc/实现状态.md §1`。

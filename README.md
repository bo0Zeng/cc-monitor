# cc-monitor

> **Claude Code CLI 的只读输出渲染窗口** — Tauri 2 + Vanilla TypeScript，Windows 桌面应用

把 Claude Code CLI 写入 `~/.claude/projects/*.jsonl` 的实时对话用现代 UI 渲染：Markdown / LaTeX / 代码高亮 / 工具调用折叠卡 / 多 Tab 自动管理 / 历史会话浏览与恢复。**完全只读、零侵入**（不修改 Claude Code 任何文件）。

![功能截图占位 — 在 doc/screenshots/ 添加后取消注释](doc/screenshots/main.png)

---

## 功能特性

### 实时监控
- 自动监听 `~/.claude/projects/**/*.jsonl`，新行 200ms 内出现在窗口
- 多 Tab：每个活跃 Claude session 一个 Tab，标题 `[项目名] aiTitle`
- 进程探活：用 Windows `OpenProcess + GetProcessTimes` 校验 PID + 创建时间，防 PID 复用
- session 退出后 Tab 灰显归档，可手动关闭（Ctrl+W）

### 富渲染
- **Markdown**：GFM + 表格 + 任务列表（marked.js）
- **LaTeX**：`$...$` 行内、`$$...$$` 块级（KaTeX）
- **代码高亮**：30+ 主流语言（highlight.js/common），无 lang 时不做昂贵 auto-detect
- **工具调用**：`tool_use` + `tool_result` 合并到同一折叠卡，长输出嵌套二级折叠
- **subagent**：`Task` / `Agent` 工具自动嵌入子 JSONL 内容（懒加载）
- **/compact 摘要**：折叠展示，避免污染视图
- **代码块复制**：每个 code block 右上角"复制"按钮

### 历史浏览器（v1.5 新增）
- 顶栏 📜 按钮 / `Ctrl+H` 切换；按**工作目录分组**展示
- 项目组默认折叠；点击展开**懒加载**该项目的所有会话
- 操作：⭐ 标星 · ✏️ 重命名（中文 OK）· 🙈 隐藏 · ↩️ 恢复（`claude --resume`）· 🗑️ 物理删除
- 点击会话条目进入**只读消息查看器**，复用实时 Tab 的渲染管线

### 设置面板（Ctrl+,）
- **数据**：可配置 Claude 数据目录（支持 `CLAUDE_CONFIG_DIR` 环境变量兜底）
- **字体**：正文 / 等宽字体 5 种预设 + 字号
- **颜色**：10 个 token（背景 / 文本 / 强调色 / 状态色），实时预览
- 持久化到 `~/.claude/claudecode-frontend/config.json`

### 终端跳焦点
- 每个 live Tab 有 ↗ 按钮 / `Ctrl+\`` 调出对应终端窗口（4 阶段 HWND 匹配，详 [`doc/实现状态.md §11`](doc/实现状态.md)）

### 快捷键
| 按键 | 作用 |
|---|---|
| `Ctrl+Tab` / `Ctrl+Shift+Tab` | 切下一个 / 上一个 Tab |
| `Ctrl+W` | 关闭当前 archived Tab |
| `Ctrl+H` | 打开 / 关闭历史浏览器 |
| `Ctrl+\`` | 调出当前 Tab 对应的终端窗口 |
| `Ctrl+,` | 打开设置面板 |
| `Esc` | 关历史只读视图 → 关历史视图 / 关设置 |

---

## 安装（用户向）

### 系统要求
- Windows 11 / 10 (1809+)
- [Microsoft Edge WebView2 Runtime](https://developer.microsoft.com/microsoft-edge/webview2/)（Win11 自带，Win10 需安装）
- [Claude Code CLI](https://github.com/anthropics/claude-code) 已安装并跑过至少一次

### 下载与安装
1. 从 [Releases](https://github.com/local/cc-monitor/releases) 下载最新 `cc-monitor_1.5.0_x64-setup.exe`（NSIS 安装包）或 `cc-monitor_1.5.0_x64_zh-CN.msi`（MSI 包）
2. 双击运行；首次会提示 Windows SmartScreen "未知发布者"（未签名），选"仍要运行"
3. 安装完启动 cc-monitor.exe；如果还没活跃 Claude 会话，窗口显示"等待活跃 Claude Code 会话…"
4. 在另一个终端跑 `claude` → cc-monitor 会自动出现对应 Tab

---

## 开发（开发者向）

### 前置依赖

| 工具 | 版本 | 说明 |
|---|---|---|
| **Node.js** | LTS (≥ 18) | 前端构建 |
| **Rust** | stable (≥ 1.75) | rustup + msvc toolchain |
| **MSVC Build Tools 2022** | 含 VCTools workload | `link.exe` / `cl.exe` / Windows SDK |
| **WebView2 Runtime** | 自带 | 上面用户向章节 |

> `scripts/run.ps1` 走 `vswhere.exe` 自动找 MSVC 安装位置（非默认路径也行）。

### 起 dev server

```powershell
cd D:\path\to\cc-monitor

npm install                                              # 一次性
powershell -NoProfile -File scripts\run.ps1 dev          # 弹 1100x800 窗口
```

直接 `npx tauri dev` 也行，**前提是当前 PowerShell 已注入 vcvars**（否则 link.exe 找的是 Git Bash 的 GNU coreutils 假冒，编译挂）。

### 生产构建

```powershell
powershell -NoProfile -File scripts\run.ps1 build
```

产物：
- `src-tauri/target/release/cc-monitor.exe`
- `src-tauri/target/release/bundle/msi/cc-monitor_1.5.0_x64_zh-CN.msi`
- `src-tauri/target/release/bundle/nsis/cc-monitor_1.5.0_x64-setup.exe`

详细打包流程见 [`doc/BUILD.md`](../doc/BUILD.md)。

### 其它命令

```powershell
powershell -NoProfile -File scripts\run.ps1 check    # cargo check
powershell -NoProfile -File scripts\run.ps1 clean    # cargo clean
```

---

## 项目结构

```
cc-monitor/
├── README.md                # 本文件
├── LICENSE                  # MIT
├── package.json             # 前端依赖 + scripts
├── vite.config.ts           # Vite 配置（端口可通过 VITE_PORT env 覆盖）
├── tsconfig.json            # TS strict mode
├── index.html               # 单页面入口
├── src/                     # 前端 (Vanilla TypeScript)
│   ├── README.md            # 前端模块导览
│   ├── main.ts              # 入口、快捷键、HMR 全 reload
│   ├── events.ts            # 订阅 jsonl-line / session-ended（批量调度让出主线程）
│   ├── tabs.ts              # TabManager：Tab 状态机
│   ├── stream.ts            # MessageStream：ResizeObserver 贴底滚动
│   ├── render.ts            # marked + KaTeX + hljs + DOMPurify
│   ├── config.ts            # invoke load/save_config
│   ├── paths.ts             # claudeDir 设置读写
│   ├── theme.ts             # CSS token 应用
│   ├── styles.css           # 全部样式 + token 系统
│   ├── cards/               # 折叠卡组件
│   │   ├── index.ts         # renderMessage 分发 + tool group + tool_result 合并
│   │   ├── slash.ts         # / 命令紧凑卡
│   │   ├── compact.ts       # /compact 续接折叠
│   │   └── subagent.ts      # Task 折叠卡（懒加载）
│   ├── settings/
│   │   └── panel.ts         # 设置面板（数据目录 + 主题）
│   └── views/
│       ├── history.ts       # 历史浏览器（项目分组、懒加载、增删改）
│       └── session-viewer.ts # 只读消息查看器
├── src-tauri/               # 后端 (Rust + Tauri 2)
│   ├── README.md            # 后端模块导览
│   ├── Cargo.toml           # 依赖 + 包元数据
│   ├── tauri.conf.json      # 应用元数据 + bundle 配置
│   ├── build.rs             # tauri_build::build()
│   ├── capabilities/        # IPC 权限
│   ├── icons/               # 全套图标（ico/icns/png）
│   └── src/
│       ├── main.rs          # → lib::run()
│       ├── lib.rs           # Tauri Builder + 工作线程编排
│       ├── paths.rs         # CLAUDE_CONFIG_DIR 三级解析
│       ├── messages.rs      # JsonlRecord enum
│       ├── parser.rs        # 按行解析
│       ├── watcher.rs       # 递归监听 projects + 活跃过滤
│       ├── session_map.rs   # 直读 sessions/<PID>.json + 探活 + 终端跳焦
│       ├── subagent.rs      # load_subagent + 关联策略
│       ├── event_replay.rs  # F5 重放（顺序严格）
│       ├── history.rs       # 两级懒加载历史 + 元数据 + 删除 + resume
│       ├── config.rs        # load/save_config + Windows 原子写
│       └── bridge.rs        # IPC 事件常量
└── scripts/
    ├── README.md            # 脚本说明
    ├── run.ps1              # MSVC dev shell 注入 + 命令路由
    └── session-register.ps1 # 已废止 hook 脚本（仅保留备查）
```

详见 [`src/README.md`](src/README.md) / [`src-tauri/README.md`](src-tauri/README.md) / [`scripts/README.md`](scripts/README.md)。

---

## 文档导航

| 文档 | 用途 | 权威性 |
|---|---|---|
| [`doc/README.md`](../doc/README.md) | 文档总目录 + 阅读顺序 | — |
| [`doc/实现状态.md`](../doc/实现状态.md) | **当前实现的权威说明** | ⭐ 权威 |
| [`doc/BUILD.md`](../doc/BUILD.md) | exe / msi / nsis 打包流程 | ⭐ 权威 |
| [`doc/CHANGELOG.md`](../doc/CHANGELOG.md) | v1.0 → v1.5 版本演进 | ⭐ 权威 |
| [`doc/架构文档.md`](../doc/架构文档.md) | 前期架构规划 | ⚠️ 参考 |
| [`doc/技术实现文档.md`](../doc/技术实现文档.md) | 前期技术细节 | ⚠️ 参考 |
| [`doc/设计文档.md`](../doc/设计文档.md) | UI / 交互设计稿 | ⚠️ 参考 |
| [`doc/需求文档.md`](../doc/需求文档.md) | 原始需求 | ⚠️ 参考 |

⚠️ 标的文档与代码不一致时**以实现状态.md + 代码为准**。

---

## 不做（v1 明确范围外）

- **macOS / Linux 适配** — 核心 Win32 调用（探活、HWND 匹配）无跨平台抽象，v2 才考虑
- **终端 → Tab 焦点自动同步** — Windows 11 默认 Windows Terminal 单进程多窗口架构，无 OS API 可区分 tab/window（详 [`doc/实现状态.md §2.3`](../doc/实现状态.md)）
- **历史全文搜索** — 当前只搜项目名 / 标题；session 内容搜索留 v2 用 SQLite/FTS
- **历史软删除 / 回收站** — 当前直接物理删除，二次确认
- **命令面板 (Ctrl+K) / 虚拟滚动**

---

## 许可

[MIT](LICENSE)

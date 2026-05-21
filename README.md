# cc-monitor

> **Claude Code CLI 的只读输出渲染窗口** — Tauri 2 + Vanilla TypeScript，Windows 桌面应用

把 Claude Code CLI 写入 `~/.claude/projects/*.jsonl` 的实时对话用现代 UI 渲染：Markdown / LaTeX / 代码高亮 / 工具调用折叠卡 / 多 Tab 自动管理 / 历史会话浏览与恢复。**完全只读、零侵入**（不修改 Claude Code 任何文件，唯一例外是用户在历史浏览器里**显式**点删除）。

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

### 历史浏览器
- 顶栏 `◷` 按钮 / `Ctrl+H` 切换；按**工作目录分组**展示
- 项目组**默认折叠**；点击展开**懒加载**该项目的所有会话（初次打开几百项目 < 100ms）
- 每行操作（纯字符按钮，不用 emoji 避免跨平台字体差异）：
  - `★/☆` 标星（黄色高亮表示已星标）
  - `✎` 重命名（支持中文）
  - `–/+` 隐藏 / 取消隐藏（不删 jsonl，仅默认列表不显示）
  - `↺` 恢复（在新终端窗口跑 `claude --resume`）
  - `✕` 物理删除（二次确认；jsonl 文件被直接删，Claude Code 之后也无法 resume）
- 点击会话条目进入**只读消息查看器**，复用实时 Tab 的渲染管线

### 设置面板（Ctrl+,）
- **数据**：可配置 Claude 数据目录（三级回退：设置面板配置 > `$CLAUDE_CONFIG_DIR` 环境变量 > `~/.claude`）
- **字体**：正文 / 等宽字体 5 种预设 + 字号
- **颜色**：10 个 token（背景 / 文本 / 强调色 / 状态色），实时预览
- 持久化到 `~/.claude/claudecode-frontend/config.json`

### 终端跳焦点
- 每个 live Tab 有 ↗ 按钮 / `Ctrl+\`` 调出对应终端窗口（4 阶段 HWND 匹配）

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

从 Releases 页下载：

- `cc-monitor_1.5.0_x64-setup.exe` — NSIS 安装器（推荐普通用户）
- `cc-monitor_1.5.0_x64_zh-CN.msi` — MSI 包（适合企业 IT 部署）

双击运行；首次会提示 Windows SmartScreen "未知发布者"（未签名），选「更多信息 → 仍要运行」。

安装完启动 `cc-monitor.exe`；如果还没活跃 Claude 会话，窗口显示"等待活跃 Claude Code 会话…"。在另一个终端跑 `claude` → cc-monitor 自动出现对应 Tab。

### 首次使用

1. 启动 cc-monitor
2. 任一终端跑 `claude`（cc-monitor 立刻多一个 Tab）
3. 在 claude 里输入一句话 → cc-monitor Tab 内 200ms 内出现 user / assistant 消息
4. 点 📜 浏览历史；点设置 ⚙ 调主题 / Claude 数据目录

### 故障排查

| 现象 | 排查 |
|---|---|
| 启动报 "WebView2 Runtime not found" | 安装 [WebView2 Runtime](https://developer.microsoft.com/microsoft-edge/webview2/) |
| 跑 claude 后 Tab 不出现 | 检查 `~/.claude/sessions/` 下是否有 `<PID>.json`；如果没有，说明 Claude Code 没正常启动 |
| `npm run tauri dev` 报 `EACCES: permission denied :::5174` | dev 端口被 Windows Hyper-V 动态保留占用（`netsh interface ipv4 show excludedportrange protocol=tcp` 查）。临时换端口：`$env:VITE_PORT=3000; powershell -NoProfile -File scripts\run.ps1 dev`，同步把 `src-tauri/tauri.conf.json` 的 `devUrl` 改成相同端口 |
| 历史浏览器 `↺` 恢复失败 | 确认终端 PATH 里有 `claude` 命令；Windows 自带 cmd.exe 总能用，wt.exe 是可选 |
| Claude 数据装在非默认路径 | 设置面板 → 数据 → Claude 数据目录；或设环境变量 `CLAUDE_CONFIG_DIR` 后重启 |
| 鼠标光标卡住 | 已在 v1.5 修复；如仍出现，DevTools (F12) 查看 Performance |

---

## 开发（开发者向）

### 前置依赖

| 工具 | 版本 | 检查 |
|---|---|---|
| **Node.js** | LTS (≥ 18) | `node -v` |
| **Rust** | stable (≥ 1.75) | `rustc --version` |
| **MSVC Build Tools 2022** | 含 VCTools workload | `where link.exe` 应找到 MSVC 的 link |
| **WebView2 Runtime** | Win11 自带 / Win10 [手装](https://developer.microsoft.com/microsoft-edge/webview2/) | — |

`scripts/run.ps1` 走 `vswhere.exe` 自动找 MSVC（非默认路径也行），无需手动 vcvars。

### 起 dev server

```powershell
cd D:\path\to\cc-monitor

npm install                                              # 一次性
powershell -NoProfile -File scripts\run.ps1 dev          # 弹 1100x800 窗口
```

直接 `npx tauri dev` 也行，**前提是当前 PowerShell 已注入 vcvars**（否则 link.exe 找的是 Git Bash 的 GNU coreutils 假冒，编译挂）。

### 其它常用命令

```powershell
powershell -NoProfile -File scripts\run.ps1 check    # cargo check
powershell -NoProfile -File scripts\run.ps1 clean    # cargo clean
powershell -NoProfile -File scripts\run.ps1 build    # 生产构建（见下）
```

---

## 生产构建 / 打包

### 一、Pre-build Checklist

- [ ] **版本号三处对齐** —— 改动后必须同步：
  - `package.json` → `version`
  - `src-tauri/Cargo.toml` → `[package].version`
  - `src-tauri/tauri.conf.json` → `version`
- [ ] **`Cargo.lock` 提交** —— Rust 应用必须锁版本
- [ ] **[CHANGELOG.md](CHANGELOG.md) 更新** —— 标注本版本新功能 / 修复
- [ ] **`npm run build` + `cargo check` 全绿** —— 类型 + 编译
- [ ] **手测核心路径** —— 启动 / Tab 出现 / 历史浏览 / 设置面板 / resume
- [ ] **WebView2 在干净 Win10 上能跑** —— 用户机器不一定预装

### 二、构建命令

```powershell
powershell -NoProfile -File scripts\run.ps1 build
# 等价于（在已注入 vcvars 的 PS 里）：
npx tauri build
```

约 3-8 分钟。输出：

```
src-tauri/target/release/
├── cc-monitor.exe                                       ← 主程序（需 WebView2）
└── bundle/
    ├── msi/
    │   └── cc-monitor_1.5.0_x64_zh-CN.msi               ← Windows Installer 包
    └── nsis/
        └── cc-monitor_1.5.0_x64-setup.exe               ← NSIS Setup 安装器
```

### 三、产物对比

| 格式 | 体积（约） | 适合 |
|---|---|---|
| **cc-monitor.exe** | ~10 MB | 开发者本机；需 WebView2 已装 |
| **MSI** | ~5 MB | 企业 IT 部署、Windows 11 原生 |
| **NSIS Setup** | ~5 MB | 普通用户双击安装、体积小 |

体积关键：`Cargo.toml::[profile.release]` 已配 `opt-level = "z"` + `lto = true` + `strip = true`。

### 四、首次发版注意

- **SmartScreen 警告**：未签名 exe 首次运行被拦"未知发布者"；用户点「更多信息 → 仍要运行」。长期方案是买 Code Signing 证书。
- **NSIS 默认**：`installMode: perMachine`，装到 `C:\Program Files\cc-monitor\` 需管理员；改 `perUser` 装到 `%LOCALAPPDATA%`（在 `src-tauri/tauri.conf.json::bundle.windows.nsis` 改）
- **WebView2 自动安装**：默认不内置；Win10 用户需手装。要内置加 `webviewInstallMode: { type: "downloadBootstrapper" }`
- **配置不删**：卸载不删 `~/.claude/claudecode-frontend/`（用户的标星 / 重命名等元数据保留）

### 五、典型打包错误

| 错误 | 原因 | 修复 |
|---|---|---|
| `linker link.exe not found` | 没注入 vcvars | 用 `scripts\run.ps1 build` 而非 `cargo build` |
| `Microsoft Visual C++ 14.0 is required` | 缺 MSVC 或缺 VCTools workload | VS Installer 加 workload |
| 卡在 `Compiling cc-monitor` | Rust 首次编译慢 ~5 min | 等 |
| NSIS `MakeNSIS exited with code 1` | 图标 `.ico` 损坏 / 路径含中文 | 检查 `src-tauri/icons/icon.ico` |
| MSI 报 `WiX is not installed` | Tauri 自动下载到 `%LOCALAPPDATA%\tauri\WixTools3`；网络不通时手装 | 检查网络或手装 WiX |

### 六、Release SOP

1. checklist 一、全过
2. 改三处版本号到 `x.y.z`
3. 更新 [CHANGELOG.md](CHANGELOG.md) 加新版本段
4. `git commit -am "release: vx.y.z"` + `git tag -a vx.y.z`
5. `powershell -NoProfile -File scripts\run.ps1 build`
6. 测试 msi / nsis-setup.exe / cc-monitor.exe 在干净 Win10 + Win11 上启动
7. `Get-FileHash` 生成校验和
8. GitHub Release 上传二进制 + 校验和 + 引用 CHANGELOG 对应段

### 七、Code Signing 接入位置

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

证书来源：DigiCert / Sectigo / GlobalSign（OV ~$200/年 / EV ~$400/年 + 立即去 SmartScreen）。

---

## 项目结构

```
cc-monitor/
├── README.md                # 本文件（用户向 + 开发者向 + 打包流程）
├── CHANGELOG.md             # 版本历史
├── LICENSE                  # MIT
├── package.json             # 前端依赖 + scripts
├── vite.config.ts           # Vite 配置（VITE_PORT env 可覆盖端口）
├── tsconfig.json            # TS strict mode
├── index.html               # 单页面入口
├── src/                     # 前端 (Vanilla TypeScript)；详 src/README.md
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
│   ├── settings/            # 设置面板
│   └── views/               # 历史浏览器 + 只读会话查看器
├── src-tauri/               # 后端 (Rust + Tauri 2)；详 src-tauri/README.md
│   ├── Cargo.toml           # 依赖 + 包元数据
│   ├── tauri.conf.json      # 应用元数据 + bundle 配置
│   ├── build.rs             # tauri_build::build()
│   ├── capabilities/        # IPC 权限
│   ├── icons/               # 全套图标（ico/icns/png）
│   └── src/
│       ├── main.rs / lib.rs # 入口 + Tauri Builder
│       ├── paths.rs         # CLAUDE_CONFIG_DIR 三级解析
│       ├── messages.rs      # JsonlRecord enum
│       ├── parser.rs        # 按行解析
│       ├── watcher.rs       # 递归监听 projects + 活跃过滤
│       ├── session_map.rs   # sessions/<PID>.json + 探活 + 终端跳焦
│       ├── subagent.rs      # load_subagent
│       ├── event_replay.rs  # F5 重放
│       ├── history.rs       # 两级懒加载历史 + 元数据 + resume
│       ├── config.rs        # load/save_config + Windows 原子写
│       └── bridge.rs        # IPC 事件常量
└── scripts/                 # 详 scripts/README.md
    ├── run.ps1              # MSVC dev shell 注入 + 命令路由
    └── session-register.ps1 # 已废止 hook 脚本（仅保留备查）
```

子目录 README 是开发者深入时的导览：
- [`src/README.md`](src/README.md) — 前端模块表 / 数据流 / 添加新功能入口
- [`src-tauri/README.md`](src-tauri/README.md) — 后端模块表 / 完整 IPC 清单 / 工程坑
- [`scripts/README.md`](scripts/README.md) — 脚本说明

---

## 不做（v1 明确范围外）

- **macOS / Linux 适配** — 核心 Win32 调用（探活、HWND 匹配）无跨平台抽象，v2 才考虑
- **终端 → Tab 焦点自动同步** — Windows 11 默认 Windows Terminal 单进程多窗口架构，无 OS API 可区分 tab/window
- **历史全文搜索** — 当前只搜项目名 / 标题 / 已展开项目内的会话元数据；session 内容全文搜索留 v2 用 SQLite/FTS
- **历史软删除 / 回收站** — 当前直接物理删除，二次确认
- **命令面板 (Ctrl+K) / 虚拟滚动**

---

## 许可

[MIT](LICENSE) © 2026 cc-monitor contributors

# cc-monitor

> **Claude Code CLI 的只读输出渲染窗口** — Tauri 2 + Vanilla TypeScript，Windows 桌面应用
>
> [English](./README.en.md) · 中文 | License: MIT | 平台: Windows 10/11 | 当前版本: v2.14.0

把 Claude Code CLI 写入 `~/.claude/projects/*.jsonl` 的实时对话用现代 UI 渲染：Markdown / LaTeX / 代码高亮 / 工具调用折叠卡 / 多 Tab 自动管理 / 历史会话浏览与恢复。**完全只读、零侵入**（不修改 Claude Code 任何文件，唯一例外是用户在历史浏览器里**显式**点删除）。

**项目状态**：稳定可用。后端 155 + 远端 daemon 30 + 前端 node 纯函数 & vitest+jsdom DOM 单测，tsc 严格类型检查，CI 全绿（`npm test`）。当前发布 **v2.14.0**（修复 **maximize / 全屏内容错位**——改到 WebView2 原生层重钉合成层；修复 **F11 真全屏**——补齐 Tauri 能力权限），能力已覆盖 **SSH 远端模式**（同一窗口聚合本地 + 多台远端机器的会话，#15/#17/#18/#20/#30/#31）——含 **daemon 自动部署 + 一键安装/卸载**（内嵌 musl 二进制经 SFTP 自动推送 #29；设置面板每台机器卡片可手动装/卸 daemon 与 ccm 助手、附安装位置提示）、**远端全文搜索**（#28）、**远端历史删除 / resume 命令助手**、**历史按机器分组折叠**（#30/#31）、**版本协商 + 拥塞提示**（#32/#33）、**会话红绿灯**（#23）、**本地会话 resume 后 Tab 自动复活**（崩溃/退出→灰显，`/resume` 后免 F5 恢复）、AskUserQuestion 选项 / API 报错直接可见（#21）、单键快捷键 + Tab 撕离独立窗口等。详 [CHANGELOG](CHANGELOG.md) / [doc/ARCHITECTURE.md](doc/ARCHITECTURE.md)。

---

## 功能

### 实时渲染
- 自动监听 `~/.claude/projects/**/*.jsonl`，新行 200ms 内出现在窗口
- 多 Tab：每个活跃 Claude session 一个 Tab，标题 `[项目名] aiTitle`
- session 退出后 Tab 灰显归档，可手动关闭（W / 中键 / ×）；**本地会话 `/resume` 后 Tab 自动复活成 live**（崩溃或退出导致灰显，重新 resume 同一会话即恢复，无需 F5）
- **Tab 独立窗口**（issue #10）：右键 Tab「在新窗口打开」/ `N`，**或直接把 Tab 往标签栏下方一拖松手**（tear-off），把会话拉到独立只读窗口（双屏并排 / 长任务常驻），与主窗口实时同步
- **会话红绿灯**（issue #23）：每个本地 Tab 的状态点实时反映 Claude 状态——🟢 运行中 / 🟡 等你决定（权限确认 / 弹窗选择，呼吸闪烁）/ 🔴 答完等输入；Agents 展开区每个 subagent 一行独立状态灯

### SSH 远端模式（issue #15）
- 在**同一个窗口**聚合本地 + **多台**远端机器（NanoPi / 任意 Linux / WSL）上的 Claude 会话，远端 Tab 标题带 `[host]` 前缀；历史浏览器按机器**分组 / 筛选**（#30/#31）
- 远端 daemon 经 SSH 实时流式传回会话；**断线自动重连**（指数退避 2→30s，issue #17）、重连后按 seq 去重补放
- **daemon 自动部署**（#29）：cc-monitor 内嵌交叉编译的 aarch64/x86_64 musl daemon 二进制，连接时按 arch + build_id 版本门控经 SFTP 自动推送——零手动部署
- **远端全文搜索**（#28）：顶栏「全文」搜索覆盖远端会话内容（daemon 服务端 `--search`），命中带 `[host]`
- **远端历史删除**（SFTP 移除 + 二次确认）/ **resume**（复制 `claude --resume` 命令到远端终端粘贴）
- **设置面板每台机器卡片**：一键 **安装 / 卸载 daemon**、**装 / 卸 ccm 助手**（写进远端 `~/.bashrc`），并有**安装位置提示**告诉你装到哪（daemon→`~/.cc-monitor/bin/`、ccm→`~/.bashrc` 标记块）；卡片可折叠成机器名
- **版本协商**（#33）+ **慢消费者 overflow 信号**（#32）：daemon/client build_id 不符或管道拥塞 → 远端健康 toast 提示
- 远端 Tab 也能 ↗ 拉前对应终端（issue #18）
- 部署见 [doc/REMOTE-PHASE0-DEPLOY.md](doc/REMOTE-PHASE0-DEPLOY.md)（自动部署 + 手动回退）

### 富渲染
- **Markdown**：GFM + 表格 + 任务列表（marked.js）
- **LaTeX**：`$...$` 行内、`$$...$$` 块级（KaTeX）
- **代码高亮**：30+ 主流语言（highlight.js/common）
- **工具调用**：`tool_use` + `tool_result` 合并到同一折叠卡，长输出嵌套二级折叠
- **代码改动 diff**（issue #14）：`Edit` / `Write` / `MultiEdit` 工具展开为**行级红删绿增 diff**（替代原始 JSON），超长自动折叠 + 「显示完整」；异常一律回退原 JSON
- **subagent**：`Task` / `Agent` 工具自动嵌入子 JSONL 内容（懒加载）
- **/compact 摘要**：折叠展示
- **代码块复制**：每个 code block 右上角"复制"按钮

### 历史浏览器
- 顶栏 `◷` 按钮 / `H` 切换；按**工作目录分组**展示
- 项目组**默认折叠**；点击展开**懒加载**该项目的所有会话
- **全文搜索**（issue #6）：顶栏切「全文」模式，搜所有会话的消息内容，命中片段高亮，点击跳进只读视图并定位；可选「含工具内容」，可按范围（user/assistant）/ 时间筛选
- 每行操作：
  - `★/☆` 标星
  - `✎` 重命名（支持中文）
  - `–/+` 隐藏 / 取消隐藏（不删 jsonl）
  - `↺` 恢复（v2.8.1：新 **PowerShell** 窗口跑 `cc --resume`，无 `cc` 时回退 `claude`；加载 profile 故代理 / env 生效）
  - `✕` 物理删除（二次确认；jsonl 文件被真删）
- 点击会话条目进入**只读消息查看器**

### 设置面板（,）

5 大折叠分组（除「行为」默认展开）：

- **行为**：自动跟随用户在终端的输入切 Tab、是否拉前 monitor 窗口
- **快捷键**：打开编辑器自定义全部 22 个可用 action 的 chord
- **数据源 & 集成**：Claude 数据目录（三级回退：设置 > `$CLAUDE_CONFIG_DIR` > `~/.claude`）+ PowerShell `__ccm_bind` 一键装
- **外观**：13 个 token（字体 + 颜色），实时预览，持久化到 `~/.claude/claudecode-frontend/config.json`
- **诊断 & 存储**：tracing 等级 toggle + log 文件路径 + 所有持久化路径透明展示

### 终端跳焦（可选）
- 每个 live Tab 有 ↗ 按钮 / `反引号` 调出对应终端窗口
- 需要装 PowerShell 集成（设置面板内一键装），细节见下文「PowerShell 集成（可选）」

### 快捷键

| 按键 | 作用 |
|---|---|
| **]** / **[** | 切下一个 / 上一个 Tab |
| **1** .. **9** | 跳到第 N 个 Tab |
| **W** | 关闭当前 archived Tab |
| **E** | 打开当前 Tab 的工作目录（资源管理器） |
| **`**（反引号） | 调出当前 Tab 对应的终端窗口 |
| **H** | 打开 / 关闭历史浏览器 |
| **,** | 打开设置面板 |
| **M** | 最小化主窗口 |
| **F11** | 切换**真全屏**（borderless 覆盖任务栏；非字母键，中文输入法下也好用） |
| **N** | 把当前 Tab 在独立窗口打开（issue #10；也可把 Tab 往标签栏下方拖出来） |
| **T** | Task 面板开 / 关 |
| **Esc** | 关历史只读视图 → 关历史视图 / 关设置 / 关弹层 |

> **默认全为单键**——cc-monitor 是只读监视窗口，无需组合键。在输入框 / 历史搜索 / 重命名等可编辑处聚焦时，快捷键自动让位给打字（不会误触发）。全部 chord 可在 **设置 → 快捷键** 编辑器里改成任意组合键；行为/面板类还有 2 个默认未绑的 toggle 可手动赋键。
>
> ⚠ **中文 / 东亚输入法**：处于中文输入模式时，裸字母键（**W / E / H / M / N / T**）会被输入法在 OS 层截走组字、快捷键收不到——按这些键前**先切英文输入法**，或在快捷键编辑器里改绑成带 `Ctrl`/`Alt` 的组合键 / 非字符键（如 `Delete`）。数字键 `1`–`9`、`[` `]`、`` ` ``、`Esc`、鼠标点 `×` 不受影响。

---

## 安装

### 系统要求
- Windows 11 / 10 (1809+)
- [Microsoft Edge WebView2 Runtime](https://developer.microsoft.com/microsoft-edge/webview2/)（Win11 自带，Win10 需安装）
- [Claude Code CLI](https://github.com/anthropics/claude-code) 已安装并跑过至少一次

### 下载

从 [Releases](https://github.com/bo0Zeng/cc-monitor/releases) 页下载最新版（文件名形如 `cc-monitor_<version>_x64-setup.exe`）：

- `*-setup.exe` — NSIS 安装器（推荐普通用户）
- `*_zh-CN.msi` — MSI 包（适合企业 IT 部署）

双击运行；首次会提示 Windows SmartScreen "未知发布者"（未签名），选「更多信息 → 仍要运行」。

### 首次使用

1. 启动 cc-monitor.exe
2. 任一终端跑 `claude`（cc-monitor 立刻多一个 Tab）
3. 在 claude 里输入 → cc-monitor 200ms 内出现 user / assistant 消息
4. 想要 Tab ↗ 跳焦终端 → 见下文 PowerShell 集成

---

## PowerShell 集成（可选）

为了让 **Tab ↗ / `反引号` 跳焦**能精确拉对应终端窗口，需要在你的 PowerShell profile 里装 `__ccm_bind` helper。

1. 打开 cc-monitor → `,` 设置面板 → **PowerShell 集成**
2. 选 profile 位置（下拉 5 项）：
   - `PowerShell 5.1 - $PROFILE（默认）` — 装到 `Microsoft.PowerShell_profile.ps1`（CurrentUserCurrentHost），只有 powershell.exe 控制台读
   - **`PowerShell 5.1 - 所有 host（profile.ps1）`** ⭐ 推荐 — VSCode 终端 / ISE / SSH 都生效
   - PowerShell 7.x 同上两项
   - 自定义路径
3. **默认不勾选"同时安装 cc wrapper"** — 只装 `__ccm_bind` helper 不动你已有命令
4. 点 [安装] → 重启 PowerShell
5. 在你自己启动 claude 的 wrapper（function / 别名）开头加一行 `__ccm_bind`

如果你想让 cc-monitor 直接帮你建一个 cc wrapper：勾上"同时安装 cc wrapper"，装 `function cc { __ccm_bind; & claude $args }`，用 `cc` 启动 claude。**注意会覆盖** profile 里已有的同名 function。

可以勾选"用 cc 启动 claude 时自动打开 monitor"。

**安全保证**：[安装] 前自动备份原 profile 到 `<profile>.ccm-backup-<时间戳>`，写后回读校验，写入失败自动从备份恢复；用 Win32 `ReplaceFileW` API 保留原 NTFS ACL。设置选择持久化到 localStorage。

不装这个完全 OK，只是 ↗ / `反引号` 不工作；实时渲染 / Tab / 历史浏览全都正常。

---

## 故障排查

| 现象 | 排查 |
|---|---|
| 启动报 "WebView2 Runtime not found" | 安装 [WebView2 Runtime](https://developer.microsoft.com/microsoft-edge/webview2/) |
| 跑 claude 后 Tab 不出现 | 检查 `~/.claude/sessions/` 下是否有 `<PID>.json` |
| Tab ↗ / `反引号` 拉不出终端 | 没装 PowerShell 集成；或装了但 wrapper 里没调 `__ccm_bind` |
| 装完 cc 集成跑 `cc` 提示绑定超时 | monitor 没在跑：先开 monitor 再开 PS；或设置面板勾选"自动打开 monitor" |
| 装 cc 集成后 PowerShell 启动报 `Access to the path … is denied` | profile NTFS ACL 在旧版本被覆盖（v1.7.10 已修），用管理员 PS 跑 `icacls "<profile>" /grant "$env:USERDOMAIN\$env:USERNAME:(F)"` |
| 历史浏览器 `↺` 恢复失败 | v2.8.1 起在 PowerShell 里跑 `cc`/`claude --resume`：确认 PowerShell profile 已装 `cc`（或 `claude` 在 PATH）；恢复窗口现在会加载 profile，代理 / `cc` 设置生效 |
| Claude 数据装在非默认路径 | 设置面板 → 数据 → Claude 数据目录；或设 `CLAUDE_CONFIG_DIR` 环境变量后重启 |

---

## 项目结构

```
cc-monitor/
├── src/                    前端 (Vanilla TS + Vite)
│   ├── main.ts             入口：bindEvents + TabManager + 全局快捷键
│   ├── events.ts           Tauri IPC listen + batch 调度
│   ├── tabs.ts             多 Tab 管理 + active 同步 + behavior
│   ├── record-timeline.ts  ⭐ v2.6 按 seq binary insert（取代 inPrependMode）
│   ├── render-stream-record.ts ⭐ v2.6 三 caller 共享渲染管线 + tool-group 后处理
│   ├── stream.ts           单 Tab 消息流 + stickToBottom
│   ├── render.ts           marked + KaTeX + hljs + DOMPurify（opts.lazy 参数化）
│   ├── branch-fold.ts      issue #8 ESC 回退分支折叠
│   ├── branching.ts        Kahn 拓扑算 mainBranch
│   ├── tasks-panel.ts      issue #11 Task 面板（status-bar chip + popover）
│   ├── error-toast.ts      v2.0 ERROR 级 tracing 弹 toast + showActionFailureToast
│   ├── local-storage.ts    ⭐ v2.6 LS_KEYS 集中 + safeGet/safeSet
│   ├── format.ts           ⭐ v2.6 formatTimestampShort/Smart + formatBytes
│   ├── cards/              卡片渲染：index, slash, compact, subagent
│   ├── settings/           设置面板 5 大组
│   ├── keybindings/        issue #5 快捷键编辑器
│   └── views/              历史浏览器 + SessionViewer
│
├── src-tauri/              后端 (Rust + Tauri 2)
│   └── src/
│       ├── lib.rs          setup + invoke_handler 注册
│       ├── watcher.rs      jsonl 文件 watcher（per-file seq 单调）
│       ├── event_replay.rs 启动重放 + chunked emit
│       ├── parser.rs       JSONL 行解析（BOM 剥）
│       ├── messages.rs     JsonlRecord enum schema
│       ├── session_map.rs  ~/.claude/sessions/ 监听
│       ├── bind.rs         PowerShell ps-await/ps-registry 握手
│       ├── tasks.rs        issue #11 tasks watcher
│       ├── history.rs      历史浏览器 IPC（流式）
│       ├── profile_installer.rs PS profile 安装（ACL 保留）
│       ├── auto_launch.rs  cc 启动时自动开 monitor
│       ├── logging.rs      tracing + ErrorEmitter
│       ├── data_paths.rs   issue #3 透明化所有持久路径
│       ├── config.rs       config.json R/W
│       ├── paths.rs        Claude 数据目录三级回退
│       ├── bridge.rs       事件常量 + payload schema（含 v2.6 seq 字段）
│       ├── subagent.rs     Task/Agent tool 子 jsonl 按需加载
│       └── utils.rs        ⭐ days_from_civil + NetTicks/FileTime newtype + scan_dir_jsons + atomic_write_json + parse_iso8601_ms 等共享 helper
│
├── doc/                    架构 + 协议 + 不变量等深度文档
├── scripts/                run.ps1（msvc dev shell + tauri dev/build）
└── CHANGELOG.md            版本历史
```

⭐ 标记的是 v2.6 B 重构新增 / 大改的模块。

## 文档

| 文档 | 给谁看 | 内容 |
|---|---|---|
| **本 README** | 用户 / 新贡献者第一站 | 安装 / 使用 / 故障排查 / 项目结构 |
| [CHANGELOG.md](CHANGELOG.md) | 升级用户 | 版本变更历史 |
| [doc/ARCHITECTURE.md](doc/ARCHITECTURE.md) | 新贡献者深入第一站 | 数据流图 + 模块表 + 设计分层 |
| [doc/IPC-PROTOCOL.md](doc/IPC-PROTOCOL.md) | 改协议的贡献者 | 跨进程文件 IPC + sessions/status + 远端 wire 完整 schema + 握手时序 |
| [doc/REMOTE-PHASE0-DEPLOY.md](doc/REMOTE-PHASE0-DEPLOY.md) | 部署远端的人 | SSH 远端 daemon 自动部署（#29）+ 手动部署 runbook（issue #15） |
| [doc/INVARIANTS.md](doc/INVARIANTS.md) | 全员 | 全局不变量清单（零侵入 / 编码 / ACL / 顺序保证 / seq 单调） |
| [doc/STATE-MATRIX.md](doc/STATE-MATRIX.md) | 改 IPC 命令的贡献者 | Tauri State 注册矩阵 + 修改规则 |
| [doc/CONTRIBUTING.md](doc/CONTRIBUTING.md) | 贡献者 | 操作 checklist + cookbook（加 IPC / jsonl 类型 / 设置项 / 快捷键） |
| [doc/DEVELOPMENT.md](doc/DEVELOPMENT.md) | 开发者 | dev 环境 / 端口冲突 / 调试技巧 |
| [doc/BUILDING.md](doc/BUILDING.md) | 发版者 | 生产构建 / 打包 / Code Signing |
| [doc/RELEASING.md](doc/RELEASING.md) | 发版者 | 发版 SOP + CHANGELOG 写法 |
| [src/README.md](src/README.md) | 前端开发 | 前端模块导览 |
| [src-tauri/README.md](src-tauri/README.md) | 后端开发 | 后端模块导览 + IPC 清单 |
| [scripts/README.md](scripts/README.md) | 用脚本的人 | 脚本说明 |

## 项目当前状态

- **版本**：v2.14.0（Released）
- **平台**：Windows 10 (1809+) / 11（远端 daemon 跑 Linux / aarch64）
- **测试**：后端 cargo test 155 + 远端 daemon 30 + 前端 node 纯函数 + vitest+jsdom DOM 单测，全绿（CI 门禁 `npm test`）
- **架构**：Tauri 2 + Vanilla TS（前端零框架依赖，~12K LOC TS + ~11K LOC Rust，含 ~1.3K 远端 daemon）
- **设计原则**：只读零侵入（INVARIANT § 1）/ 可选性 / Windows-first / 长期记忆机制（CHANGELOG + doc/ 专题文档 + 各模块 README）

---

## License

[MIT](LICENSE) © 2026 cc-monitor contributors

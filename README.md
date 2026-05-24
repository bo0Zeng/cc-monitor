# cc-monitor

> **Claude Code CLI 的只读输出渲染窗口** — Tauri 2 + Vanilla TypeScript，Windows 桌面应用
>
> [English](./README.en.md) · 中文

把 Claude Code CLI 写入 `~/.claude/projects/*.jsonl` 的实时对话用现代 UI 渲染：Markdown / LaTeX / 代码高亮 / 工具调用折叠卡 / 多 Tab 自动管理 / 历史会话浏览与恢复。**完全只读、零侵入**（不修改 Claude Code 任何文件，唯一例外是用户在历史浏览器里**显式**点删除）。

---

## 功能

### 实时渲染
- 自动监听 `~/.claude/projects/**/*.jsonl`，新行 200ms 内出现在窗口
- 多 Tab：每个活跃 Claude session 一个 Tab，标题 `[项目名] aiTitle`
- session 退出后 Tab 灰显归档，可手动关闭（Ctrl+W）

### 富渲染
- **Markdown**：GFM + 表格 + 任务列表（marked.js）
- **LaTeX**：`$...$` 行内、`$$...$$` 块级（KaTeX）
- **代码高亮**：30+ 主流语言（highlight.js/common）
- **工具调用**：`tool_use` + `tool_result` 合并到同一折叠卡，长输出嵌套二级折叠
- **subagent**：`Task` / `Agent` 工具自动嵌入子 JSONL 内容（懒加载）
- **/compact 摘要**：折叠展示
- **代码块复制**：每个 code block 右上角"复制"按钮

### 历史浏览器
- 顶栏 `◷` 按钮 / `Ctrl+H` 切换；按**工作目录分组**展示
- 项目组**默认折叠**；点击展开**懒加载**该项目的所有会话
- 每行操作：
  - `★/☆` 标星
  - `✎` 重命名（支持中文）
  - `–/+` 隐藏 / 取消隐藏（不删 jsonl）
  - `↺` 恢复（在新终端窗口跑 `claude --resume`）
  - `✕` 物理删除（二次确认；jsonl 文件被真删）
- 点击会话条目进入**只读消息查看器**

### 设置面板（Ctrl+,）
- **数据目录**：可配置 Claude 数据目录（三级回退：设置 > `$CLAUDE_CONFIG_DIR` > `~/.claude`）
- **PowerShell 集成**：装 `__ccm_bind` helper 让 Tab ↗ 跳焦精准拉对应终端窗口
- **字体 / 颜色**：13 个外观 token，实时预览，持久化到 `~/.claude/claudecode-frontend/config.json`

### 终端跳焦（可选）
- 每个 live Tab 有 ↗ 按钮 / `Ctrl+\`` 调出对应终端窗口
- 需要装 PowerShell 集成（设置面板内一键装），细节见下文「PowerShell 集成（可选）」

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

为了让 **Tab ↗ / `Ctrl+\`` 跳焦**能精确拉对应终端窗口，需要在你的 PowerShell profile 里装 `__ccm_bind` helper。

1. 打开 cc-monitor → `Ctrl+,` 设置面板 → **PowerShell 集成**
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

不装这个完全 OK，只是 ↗ / `Ctrl+\`` 不工作；实时渲染 / Tab / 历史浏览全都正常。

---

## 故障排查

| 现象 | 排查 |
|---|---|
| 启动报 "WebView2 Runtime not found" | 安装 [WebView2 Runtime](https://developer.microsoft.com/microsoft-edge/webview2/) |
| 跑 claude 后 Tab 不出现 | 检查 `~/.claude/sessions/` 下是否有 `<PID>.json` |
| Tab ↗ / `Ctrl+\`` 拉不出终端 | 没装 PowerShell 集成；或装了但 wrapper 里没调 `__ccm_bind` |
| 装完 cc 集成跑 `cc` 提示绑定超时 | monitor 没在跑：先开 monitor 再开 PS；或设置面板勾选"自动打开 monitor" |
| 装 cc 集成后 PowerShell 启动报 `Access to the path … is denied` | profile NTFS ACL 在旧版本被覆盖（v1.7.10 已修），用管理员 PS 跑 `icacls "<profile>" /grant "$env:USERDOMAIN\$env:USERNAME:(F)"` |
| 历史浏览器 `↺` 恢复失败 | 确认终端 PATH 里有 `claude` 命令 |
| Claude 数据装在非默认路径 | 设置面板 → 数据 → Claude 数据目录；或设 `CLAUDE_CONFIG_DIR` 环境变量后重启 |

---

## 文档

| 文档 | 给谁看 | 内容 |
|---|---|---|
| **本 README** | 用户 | 安装 / 使用 / 故障排查 |
| [CHANGELOG.md](CHANGELOG.md) | 升级用户 | 版本变更历史 |
| [doc/ARCHITECTURE.md](doc/ARCHITECTURE.md) | 新贡献者第一站 | 数据流图 + 模块表 + 设计分层 |
| [doc/IPC-PROTOCOL.md](doc/IPC-PROTOCOL.md) | 改协议的贡献者 | 跨进程文件 IPC 完整 schema + 握手时序 |
| [doc/INVARIANTS.md](doc/INVARIANTS.md) | 全员 | 全局不变量清单（零侵入 / 编码 / ACL / 顺序保证） |
| [doc/STATE-MATRIX.md](doc/STATE-MATRIX.md) | 改 IPC 命令的贡献者 | Tauri State 注册矩阵 + 修改规则 |
| [doc/CONTRIBUTING.md](doc/CONTRIBUTING.md) | 贡献者 | 操作 checklist + cookbook（加 IPC / jsonl 类型 / 设置项 / 快捷键） |
| [doc/DEVELOPMENT.md](doc/DEVELOPMENT.md) | 开发者 | dev 环境 / 端口冲突 / 调试技巧 |
| [doc/BUILDING.md](doc/BUILDING.md) | 发版者 | 生产构建 / 打包 / Code Signing |
| [doc/RELEASING.md](doc/RELEASING.md) | 发版者 | 发版 SOP + CHANGELOG 写法 |
| [src/README.md](src/README.md) | 前端开发 | 前端模块导览 |
| [src-tauri/README.md](src-tauri/README.md) | 后端开发 | 后端模块导览 + IPC 清单 |
| [scripts/README.md](scripts/README.md) | 用脚本的人 | 脚本说明 |

---

## License

[MIT](LICENSE) © 2026 cc-monitor contributors

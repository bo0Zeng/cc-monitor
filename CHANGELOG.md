# Changelog

本文档记录 cc-monitor 用户**可感知**的功能 / 修复 / 行为变更。
内部重构与文档调整通常不入。

格式参考 [Keep a Changelog](https://keepachangelog.com/zh-CN/1.1.0/)。
版本遵循 [SemVer](https://semver.org/)。

---

## [1.7.6] — 2026-05-24

### 改动

- **Wrapper 命令名默认值改回 `cc`**（v1.7.5 改空又改回来）。
  placeholder 提示 "cc / ccm / 留空只装 helper"，仍允许留空 / 改别的。
  留空 + 已有同名 cc function 时的"只装 helper"逻辑保留。
  填 `claude` 阻止（防无限递归）保留。

## [1.7.5] — 2026-05-24

### 新增

- **"打开 profile"按钮** —— 设置面板 PowerShell 集成区加按钮，调系统默认编辑器
  打开当前路径的 profile（用 `tauri-plugin-opener`）。方便用户手动编辑 profile
  加 `__ccm_bind` 调用。

### 改动（UI 默认值调整）

- **Wrapper 命令名默认留空** —— 之前默认 `cc`，但 `cc` 是用户自己常用的别名，
  cc-monitor 不该默认抢这个名字。**新默认：留空**，placeholder "留空只装 helper（推荐）"。
  - 留空：只装 `__ccm_bind` helper，**不装任何 wrapper function**。
    用户在自己的 wrapper（如自定义 `cc` / `mc` / 直接在 prompt 里）调 `__ccm_bind` 即可。
  - 填名字（如 `ccm`）：装 `function 名字 { __ccm_bind; & claude $args }`。
  - **填 `claude` 时阻止**：弹 alert 警告——PowerShell function 跟 exe 同名时
    function 优先，会**无限递归**。
- 移除 v1.7.3 加的"也装默认 function cc"复选框——逻辑改成"命令名是否非空"，
  UI 更简洁。
- 介绍文案重写：不再假设用户用 `cc` 命令，引导用户"自己有 wrapper 就在里面调 `__ccm_bind`"。

### 修复（release-blocker，v1.7.0 起的）

- **cc 命令握手成功但 ps-registry 不生成** —— monitor 处理 ps-await 文件
  但 `find_window_for_marker` 返回 None，导致绑定永远建立不起来，Tab ↗
  始终报"未绑定窗口"。
  - 根因：`bind.rs::find_window_for_marker` 的 `EnumWindows` callback 过滤
    `GetWindow(hwnd, GW_OWNER) != 0` 的窗口（只看顶层无 owner 窗口）。
    这是从 v1.6.x 4-tier 算法继承的——当时为了排除 popup/dialog。
  - 实测：用户的 PS 是从 explorer 启动的，Windows Terminal 接管 console。
    `$Host.UI.RawUI.WindowTitle = $marker` 设的 title 同步到 **WT 内的
    Microsoft.UI.Xaml.* 子窗口（owner != 0，owner = WT 主窗口）**，
    而**不是** WT 主窗口本身。monitor 因为 owner 过滤直接跳过这些窗口。
  - 影响版本：v1.7.0 / v1.7.1 / v1.7.2 / v1.7.3 / v1.7.4 全部带病——
    cc 集成实际上从来没在 WT 接管 console 的常见场景下 work 过。
    单测全过 + 终端流程跑通 + 文件 trace 正确，但**窗口找不到**，binding
    永不生成。
  - 修法：去掉 `GW_OWNER` 过滤。marker 字符串 = `ccm-bind-{PID}-{UUID 8 char}`
    极独特，不需要 owner=0 这个"防 popup 误命中"的保险。

### 诊断脚本（如本次复现）

附 `ccm-diag.ps1`（本仓库外）可在用户 PS 跑：模拟 cc 握手并对比 PS 端 vs
monitor 端 `EnumWindows` 看到的窗口差异。本 bug 就是这样定位的——PS 端能找到
marker，monitor 端找不到 → 一定是过滤条件差异。

### v1.7.x 教训

v1.7.0-1.7.4 看似都"装上能用"，实际除非用户是从 WT 内开新 tab 启动 PS
（owner=0 那种），否则握手永远失败。这次 bug 之所以拖到 v1.7.5 才发现：
1. 自动化测试全是纯函数单测，没法测真实窗口枚举
2. monitor 处理 await 后 silent drop（没写 ps-registry 也没报错日志可见）

---

## [1.7.4] — 2026-05-24

### 修复（release-blocker，v1.6.7 起的回归）

- **历史浏览器打不开**："加载失败：state not managed for field `map` on command
  `list_history_projects`. You must call `.manage()` before using this command"。
  - 根因：v1.6.7 撤 `bring_terminal_to_front` 时把 `app.manage(session_map.clone())`
    一并删了，但 `history.rs::list_history_projects` 和
    `list_history_sessions_in_project` 也接 `State<Arc<SessionMap>>`，没补回去就 dead。
  - v1.6.7 / 1.7.0 / 1.7.1 / 1.7.2 / 1.7.3 都带这个 bug——单测过（不跑 IPC dispatch），
    我也没实测过历史浏览器。
  - 修法：lib.rs setup 补 `app.manage(session_map.clone())`。

## [1.7.3] — 2026-05-23

### 修复

- **v1.7.2 一键安装会覆盖用户已有的 `function cc`** —— 模板默认包含完整
  `function cc { __ccm_bind; & claude $args }`，安装到 profile 时由于
  PowerShell **后定义同名 function 覆盖前面**的机制，用户在 profile 中已有的
  自定义 `function cc`（含 cd / 代理 / 自定义参数处理等逻辑）会被无声覆盖。
  虽然 BEGIN/END 块外的代码本身没被改，但运行时实际生效的是 cc-monitor 的版本。

### 改动

- **模板拆成 `__ccm_bind` helper + 可选 `function cc` 两部分**
  - `cc.ps1.tpl` 用 `{{CC_FUNCTION_BLOCK}}` placeholder，`render_cc_code`
    根据 `include_cc_function` 决定是否填充
  - `__ccm_bind` 永远装（cc 集成的核心）
  - `function cc` 现在是**可选**部分
- **UI 智能默认值**：扫描结果发现 profile 已含自定义 `function {命令名}` 时
  自动取消勾选"也装默认 function cc"复选框，安装时跳过 cc function 段
- 用户已有 cc 时的指引：在 cc 开头加一行 `__ccm_bind` 即可。例如：
  ```powershell
  function cc {
      __ccm_bind                    # ← 加这一行
      if ((Get-Location).Path -eq $env:USERPROFILE) {
          Set-Location 'D:\Sync\文档\claude-conversation'
      }
      # ... 用户自定义代理 / 其他逻辑 ...
      claude @args
  }
  ```

### IPC 改动

- `cc_integration_preview({command_name, include_cc_function})` ← 新增 bool 参数
- `cc_integration_install({path, command_name, include_cc_function})` ← 新增 bool 参数

### 用户操作

v1.7.2 已安装 + 自定义 cc 被覆盖的用户：
1. 装 v1.7.3 → 启动 monitor
2. 设置面板 → PowerShell 集成
3. 扫描会发现你已有 `function cc` → 复选框自动取消勾选
4. 点"安装" → 只装 `__ccm_bind` helper（不动你的 cc）
5. 编辑 profile，在你的 `function cc` 开头加一行 `__ccm_bind`
6. 重启 PS

## [1.7.2] — 2026-05-22

### 修复（release-blocker）

- **v1.7.0/1.7.1 装错 profile 文件名导致 cc 集成形同虚设** ——
  - 错的：`Documents/WindowsPowerShell/profile.ps1`（CurrentUserAllHosts，PS 启动**不**自动读）
  - 对的：`Documents/WindowsPowerShell/Microsoft.PowerShell_profile.ps1`（CurrentUserCurrentHost，即默认 `$PROFILE`）
  - 用户在 PS 里跑 `$PROFILE` 看到的就是后者。v1.7.0/1.7.1 装到前者 PowerShell 启动根本不加载，整个 cc 集成无效。
  - v1.7.2 `profile_installer::discover_profiles` 改用正确文件名。
  - 新增 `scan_legacy_profiles()` 检测 v1.7.0/1.7.1 错位的 profile.ps1 中是否含
    cc-monitor 块。UI 在状态扫描时显示警告 + 列出文件路径，引导用户手动清理。

### 改动（UX 大改）

- **设置面板"PowerShell 集成"区单卡片重构**：
  - PowerShell 版本下拉（Windows PowerShell 5.1 [默认] / PowerShell 7.x / 自定义路径）
  - profile 路径**可编辑输入框**（默认按版本下拉自动填充 `Microsoft.PowerShell_profile.ps1`，
    用户可手动改成任意路径——比如非标准的 OneDrive 同步路径、portable PowerShell、
    或者特殊 host 的 profile）
  - 选"自定义路径..."后路径输入框获焦让用户填
  - "重新扫描"按钮配合 flash 视觉反馈（之前点了没反应的设计 bug）
  - 状态徽章 (未安装/已安装/文件不存在)
  - 旧位置遗留警告框（紧贴主操作下方）
- **自动识别**：PS 5.1 永远显示（Windows 自带）；PS 7.x **只在 `Documents/PowerShell/` 目录存在时**才作为可选项展示，否则隐藏（绝大多数用户没装 7.x，UI 不再误导）

### 重构（后端 IPC）

- `cc_integration_install({path, command_name})` ← 之前 `{kind, command_name}` 改成接受路径直接
- `cc_integration_uninstall({path})` ← 同上
- 新增 `cc_integration_scan_path({path, command_name})` —— 用户改路径后扫描那个路径
- `cc_integration_status` response 新增 `legacy_profile_paths_with_block` 字段
- `ProfileKind` 加 `Custom` 变体

### 用户操作流（v1.7.2 安装）

1. 装 v1.7.2 后**首次启动 monitor**（auto-launch.json 会自动更新 monitor_exe_path）
2. 设置面板 → PowerShell 集成
3. 版本下拉默认 **PS 5.1**，路径已自动填 `Microsoft.PowerShell_profile.ps1`
4. 如果有 v1.7.0/1.7.1 遗留块，会看到"⚠ 检测到旧位置遗留" + 路径列表 → 手动用编辑器
   打开那个 profile.ps1 删除 BEGIN/END 之间内容（或整个文件删掉）
5. 点"预览代码"看完整内容
6. 点"安装" → 把 cc function 写到正确的 `Microsoft.PowerShell_profile.ps1`
7. **重启 PowerShell**
8. 跑 `cc` → 应该自动握手成功，Tab ↗ 能拉对应 WT 窗口

## [1.7.1] — 2026-05-22

### 新增

- **cc → 自动启动 monitor**（可选 toggle）—— v1.7.0 要求先开 monitor 后跑 cc，
  顺序反了 cc 会 fail-open（仍能启 claude，但没绑定）。v1.7.1 让 cc function
  能主动启动 monitor，但**不硬编码安装路径**（保持 portable exe 特性）：
  - monitor 每次启动调 `std::env::current_exe()` 写自身路径到
    `<monitor_data_dir>/auto-launch.json` 的 `monitor_exe_path` 字段
  - 用户移动 exe 后下次启动会自动更新（不需要重新装 cc function）
  - 设置面板新加 toggle "用 cc 启动 claude 时自动打开 monitor"
  - cc function 读 auto-launch.json：
    - `auto_launch_enabled` = true 且 monitor 没在跑且记录的路径存在 →
      `Start-Process` 启动 + `Start-Sleep -Milliseconds 2000` 等 watcher 起来
    - 已在跑（按绝对路径比对 Get-Process 的 .Path）→ 跳过启动
    - 任何检查失败 → fail-open（仍走握手，超时后 fail-open 启动 claude）
- 新 IPC：`cc_get_auto_launch` / `cc_set_auto_launch`
- 新模块 `src-tauri/src/auto_launch.rs`（含 3 个单测）

### 改动

- `scripts/cc.ps1.tpl` 加 auto-launch 段（读 auto-launch.json + Start-Process）
- 设置面板 PowerShell 集成区底部新增 toggle + monitor 路径显示

### 用户操作

第一次启用 auto-launch：
1. 至少启动一次 v1.7.1 monitor（让它记录自身路径到 auto-launch.json）
2. 设置面板 → PowerShell 集成 → 勾选 "用 cc 启动 claude 时自动打开 monitor"
3. 之后即使 monitor 没在跑，跑 cc 时会自动启动 monitor + 等 ~2s + 正常握手

## [1.7.0] — 2026-05-22

### 新增

- **cc 命令注入式绑定 Tab ↔ 终端窗口**——v1.6.x 的 4-tier 启发式算法在
  explorer 启 PowerShell + WT DefTerm 接管 console 的常见架构下不可靠（claude
  祖先链与 WT 窗口完全脱节）。v1.7 改成 PS 主动跟 monitor 握手：
  - 用户用 `cc` 命令替代 `claude` 启动会话（cc 是 PS function，包装 claude）
  - cc function 写 `ps-await/<PID>.json` + 设独特 WindowTitle marker
  - monitor 后台 watcher 调 EnumWindows 找含 marker 的窗口 → 拿到 hwnd
  - 写 `ps-registry/<PID>.json`（PS_PID ↔ hwnd 映射）→ 解除 PS 阻塞
  - 之后 claude 启动写 `sessions/<PID>.json`，monitor 用 ToolHelp 查
    claude.exe 的 parent_pid 反推 PS_PID → ps-registry → 拿 hwnd
  - 写永久 `sid-hwnd-cache.json`（含复合指纹：hwnd + owner_pid + procStart）
  - Tab ↗ / Ctrl+\` 查缓存 + 校验指纹 + SetForegroundWindow

- **设置面板"PowerShell 集成"区** —— 一键扫描 + 安装 + 卸载 cc function
  到 PS profile：
  - 同时扫描 PS 5.1 (`Documents/WindowsPowerShell/profile.ps1`) + PS 7.x
    (`Documents/PowerShell/profile.ps1`) 两个 profile 路径
  - 检测命令名冲突（profile 已有同名 function 时 UI 警告，建议改名）
  - 命令名可自定义（默认 `cc`，用户可输入 `ccm` / `monclaude` 等）
  - "预览代码"按钮弹 modal 展示完整将要写入的代码（含 BEGIN/END marker）
  - 块标记隔离：`# === cc-monitor BEGIN v1 ===` / `# === cc-monitor END ===`
    重装时整块替换、卸载时整块删除，用户在块外任何内容不动
  - 实时显示当前活跃 PS 注册数

- **rust 后端新增模块**：
  - `bind.rs`：BindRegistry（ps-await 监听 + EnumWindows + ps-registry 持久化）
    + SidHwndCache（sid → hwnd 持久化）+ verify_binding / activate 拉前
    + 心跳 10s 清死 PS 注册
  - `profile_installer.rs`：profile 路径解析 + 块插入/卸载 + 命令名冲突检测
  - `scripts/cc.ps1.tpl`：cc function 模板（include_str! 嵌入二进制）

- **rust 后端新增 4 个 Tauri IPC 命令**：
  - `cc_integration_status` — 扫描两个 profile 状态
  - `cc_integration_preview` — 渲染将要写入的代码（不修改文件）
  - `cc_integration_install` — 写入指定 profile（PS 5.1 或 PS 7.x）
  - `cc_integration_uninstall` — 移除 BEGIN/END 块
  - `bring_terminal_to_front` — 拉前命令（v1.6.7 删除后恢复，但实现完全重写）

### 改动

- Cargo.toml 恢复 `Win32_System_Diagnostics_ToolHelp`（用于 claude.exe →
  parent_pid 查询）+ `Win32_UI_WindowsAndMessaging`（EnumWindows / GetWindowTextW /
  SetForegroundWindow）feature
- 前端恢复 Tab ↗ 按钮 + Ctrl+\` 快捷键 + 失败时右下角 fixed toast

### 关键决策

| 决策 | 选择 | 理由 |
|---|---|---|
| profile 修改方式 | 一键安装 + 预览 + 卸载 | 默认便利但完全透明，BEGIN/END 块隔离不动用户其他内容 |
| 默认命令名 | `cc` | 短易记；UI 可改 |
| 没装 cc 时 | 报"未绑定窗口"不 fallback | 老 4-tier 算法已彻底删除 |
| 复合指纹 | hwnd + owner_pid + owner_proc_start + ps_proc_start | 防 HWND 复用 + PID 复用 |

### 用户操作流（首次安装）

1. 设置面板（Ctrl+,）→ 滚到"PowerShell 集成"区
2. 点 PS 5.1 或 PS 7.x 卡片的"安装"
3. 重启 PowerShell
4. 新 session 启动时 PS function 自动跟 monitor 握手（< 100ms，无感知）
5. 用 `cc` 替代 `claude` 启动会话
6. 之后 Tab ↗ / Ctrl+\` 直接拉对应 WT 窗口

### 设计文档

`D:/Sync/文档/cc-monitor-v1.7-cc-integration-plan.md`（plan + 时序图 + 数据结构）

## [1.6.7] — 2026-05-22

### 移除

- **`bring_terminal_to_front` 整条链路撤回**（v1.6.0–1.6.6 的"Tab ↗ 拉对应
  终端窗口"功能）。在 explorer 启 PowerShell + Windows Terminal DefTerm 接管
  console 的常见架构下，claude.exe 的祖先链与 WT 窗口完全脱节，4-tier
  启发式（祖先链 / 终端类进程 + title 匹配）无法可靠定位"哪个 WT 窗口跑了
  这个 session"。Ambiguous 报错让用户疲于配置独特 title，"Claude Code"
  fallback 又引入新歧义（误命中无 ai-title session 的同名窗口）。算法层修不
  动这个问题——需要 OS API 不暴露的"PowerShell PID ↔ WT HWND"映射。
  - Rust：删 `session_map.rs` 里 `bring_terminal_to_front` 方法 + 整个
    WindowMatcher（`SelectResult` / `MatchTier` / `build_ancestors` /
    `build_search_terms` / `classify_window` / `select_best_window` /
    `ProcInfo` / `WindowSnap` / `is_system_shell_process` /
    `is_terminal_process` / `process_info_snapshot` /
    `enumerate_top_level_windows` / `activate_window`）+ 14 个对应单测
  - `lib.rs`：删 `bring_terminal_to_front` Tauri 命令注册
  - `Cargo.toml`：删 `Win32_System_Diagnostics_ToolHelp` /
    `Win32_System_ProcessStatus` / `Win32_UI_WindowsAndMessaging` 三个 feature
  - 前端：删 `tabs.ts` 的 `bringActiveTerminalToFront` / `bringTerminalToFront` /
    `showBringTerminalToast` + Tab 上的 ↗ 按钮 + `main.ts` 的 Ctrl+\` 快捷键 +
    `styles.css` 的 `.tab-focus` / `#bring-terminal-toast` /
    `.status-msg.status-error`
  - 文档：删 `src-tauri/src/README.md`（专讲拉终端机制的设计文档）
- 保留 `SessionInfo.name` 字段（标记 `#[allow(dead_code)]`），为 v1.7 注入式
  绑定方案准备。

### 保留

- session_map.rs 心跳（2s 探活清死 session，v1.6.3 引入）
- watcher.rs force_rescan 通道 + SessionChange.added 字段（v1.6.3 引入，修
  /resume 竞态的 session 新增鲁棒重扫，跟拉终端无关）

### 下一步

v1.7 通过 `cc` 命令注入式绑定实现拉终端：用户用包装后的 `cc` 启动 claude，
wrapper 主动把 (sid, hwnd) 映射注册给 monitor，绕开"无法从进程树定位窗口"
的 OS 限制。

## [1.6.6] — 2026-05-22

### 修复

- **无 ai-title 的 session 拉前歧义** —— claude CLI 启动时默认 console title
  是 "Claude Code"，要等会话生成 ai-title 后才改成项目语义名。**没生成 ai-title
  之前**，对应的 WT 窗口 title 就是 "✳ Claude Code"。之前 `build_search_terms`
  只用 cwd / 项目名做匹配，没有任何 term 能命中 "Claude Code"——所有终端类
  窗口都 tier D，select 报歧义。
  - `build_search_terms`：当 `ai_title is None` 时把 `"Claude Code"` 加入 terms。
    "Claude Code" 窗口 title_match → 升 tier C (TerminalWithTitle)，唯一命中
    时 select 取它。其他有 ai-title 的窗口（"filter-active..." / "Analyze
    shengwu..."）仍 tier D，不参与竞争。
  - 角落情况：多个无 ai-title 的 session 并存 → 所有 "Claude Code" 窗口同 tier
    C 多候选 → 仍歧义，需要用户配独特 title（toast 提示）。

### 测试

- 单元测试 34 → 36。新增 2 个：`search_terms_include_claude_code_fallback_when_no_ai_title`
  + `search_terms_skip_claude_code_fallback_when_ai_title_present`。

## [1.6.5] — 2026-05-22

### 修复

- **点 ↗ 按钮 monitor 假死 + 消息区域被挤位**（强烈关联 Bug 1 "拉不起来"）——
  根因有两个，一起修：
  1. `bring_terminal_to_front` 是 sync `#[tauri::command]`。Tauri 2 sync 命令
     在 main IPC thread 跑（不是 spawn_blocking），命令期间整个 webview 假死
     不响应任何输入。改 `async` + 显式 `tokio::task::spawn_blocking` 包 Win32
     调用，IPC 主线程立即返回，webview 全程可点。
  2. v1.6.4 把错误写进状态栏文字（`statusMsg.textContent`）会触发 flex 重排，
     长错误字符让 `.status-msg` 内部 layout 变化，间接挤压上面的 message stream
     区域 → 用户看到"消息往右移动"。改 fixed 定位的 `#bring-terminal-toast`
     固定在右下角，完全脱离文档流，绝对不影响其他 element。
- **前端 invoke 加 5s timeout** —— 若极端情况下后端仍卡（如 EnumWindows
  callback 撞上 hung window），5s 后强制 reject 显示"invoke 超时"toast，
  不再让 monitor 看上去假死。

### 已废弃

- `.status-msg.status-error` CSS 规则保留但不再使用（v1.6.4 引入 + v1.6.5 替换）。

## [1.6.4] — 2026-05-22

### 修复

- **`bring_terminal_to_front` 失败时用户看不到原因** —— v1.6.3 加了
  Ambiguous / NoMatch 详细错误，但前端只 `console.warn` 没人开 DevTools。
  这次把后端 Err 字符串抬到状态栏显示 8s（红色 `⚠` 前缀 + `title` 属性
  保留完整文本，hover 可看截断前的全文）。现在"拉不起来"能直接读到
  "歧义：A 命中 4 个终端窗口 (sid=..., terms=[...])；候选: [...]；
  修复：在 PowerShell startup 给当前会话窗口设独特 title"。

## [1.6.3] — 2026-05-22

### 修复

- **多 Tab 拉错终端（同一窗口被反复选中）** —— Windows Terminal 单进程多窗口
  共享同一个 PID，所有 WT 窗口都"在 claude 的祖先链上"——`classify_window`
  把它们全部归到 tier A 或 tier B，而 `select_best_window` 旧实现"同 tier 只
  记第一个候选" 导致多个 session 撞同一窗口（EnumWindows Z-order 的第一个）。
  - 新增 `SelectResult { Single | Ambiguous | NoMatch }`：tier 内多候选时返
    `Ambiguous`，调用方报详细错（含命中 tier + 候选 hwnd/title + 配置建议）
    而非随机选一个。**拉错 → 拉不到，但用户得知该如何修**。
  - `build_search_terms` 加完整 cwd 路径作 term（含反斜杠 / 正斜杠两个版本）：
    用户在 PS startup 设 `$Host.UI.RawUI.WindowTitle = $PWD` 时能精确匹配
    每个会话独有的窗口。

- **关闭终端窗口后 Tab 不归档** —— Claude Code 异常退出时
  `~/.claude/sessions/<PID>.json` 可能不会被删，session_map 仅靠文件事件触发
  扫描 → 死 session 永远不发 `session-ended`。`session_map::run_watcher`
  加 **2 秒心跳**：`recv_timeout(2s)`，timeout 分支主动 `is_process_alive`
  探活所有 by_id 条目，死的自动 remove + emit removed → 前端 Tab 在 ≤2s 内灰显。

- **/resume 历史会话偶发不出现 Tab（多个并发时尤明显）** —— jsonl 行可能
  在 `sessions/<PID>.json` 之前到达 watcher；此时 `active(sid)` 返 false →
  `process_file` early return，且无任何机制重新触发该文件的扫描。新增
  **session-added → 强制重扫**安全网：
  - `SessionChange` 加 `added: Vec<String>`
  - `watcher::spawn_watcher` 返回 `WatcherHandle { rx, force_rescan_tx }`
  - lib.rs 收到 session_map 的 added 列表 → 通过 `force_rescan_tx` 通知
    jsonl-watcher 主动重扫该 session 的所有 jsonl 文件
  - jsonl-watcher 主循环改 `recv_timeout(100ms)` 兼容 rescan 通道（jsonl-line
    总延迟从 ~100ms 上升到 ~200ms，对流式渲染可接受）

### 测试

- 单元测试 29 → 34。新增 5 个：tier A 多候选 → Ambiguous、tier D 多候选 →
  Ambiguous、低 tier 唯一命中 → Single、完整 cwd 加入 terms、短 cwd 跳过完整路径。

## [1.6.2] — 2026-05-21

### 修复

- **`/compact` 等本地命令的 stdout 漏到 user 消息里渲染** —— Claude Code CLI
  把 `/compact` 写进 JSONL 时格式是 `<command-name>/compact</command-name>
  <command-message>compact</command-message><command-args></command-args>
  <local-command-stdout>Compacted...</local-command-stdout>`。v1.5 已过滤
  `<local-command-caveat>` 等 3 个标签但漏了 `<local-command-stdout>`，
  整条 user 消息因尾部多了一段无法匹配 slash 紧凑卡正则，回落到普通
  user 气泡把整段连同 stdout 一起渲染出来。这次：
  - 前端 `isInternalUserNoise` 重构为 `stripInternalNoise(text): string`
    返回剥过的文本（而非 boolean）；剥噪声列表补 `local-command-stdout`；
    user 分支用剥过的文本喂下游 `parseSlashCommand` / `buildUserCard`，
    `/compact` 现正确识别为 "⌘ /compact" 紧凑卡。
  - 后端 `history.rs::clean_user_text` 历史预览的 tag 列表同步补一项。

## [1.6.1] — 2026-05-21

### 修复

- **设置面板拖 color picker 卡顿** —— 每次 `input` 事件原本调 `applyTheme()`
  全量遍历 14 个 token 调 `setProperty`，60Hz 拖动下整棵 :root 子树重算被
  压垮。新增 `applyThemeToken(key, value)` 只更单 token；`onFieldChange`
  改调它。重算量降到 1/14。

### 新增

- **设置面板每行 "↺ 恢复默认" 按钮** —— 24×24 单项重置，仅回退该字段到
  styles.css :root 默认值。底栏的全量 "恢复默认" 按钮保留。

## [1.6.0] — 2026-05-21

v1.5.0 的迭代版。首次通过 `release.yml` 自动发布（v1.5.0 tag 指向的 commit
当时 release.yml 还未引入，无法触发自动 build → 跳过 v1.5.0 release）。

### 新增

- **历史浏览器"全量加载"按钮** —— 顶栏新增；点击后并发（max 4）拉取所有项目的会话详情进缓存。完成后搜索可命中 session 内容（ai-title / 自定义标题 / 首条消息 / sessionId）。状态条显示进度 `加载 N/M …`。

### 变更

- **图标改为纯字符**（去 emoji，避免跨平台字体差异）：
  - 顶栏历史按钮 `📜` → `◷` (U+25F7 时钟样圆形)
  - 重命名 `✏️` → `✎` (U+270E pencil)
  - 隐藏 `🙈` → `–` / 取消隐藏 `👁️` → `+`
  - 恢复 `↩️` → `↺` (U+21BA anticlockwise circle arrow)
  - 删除 `🗑️` → `✕` (U+2715 X)
  - 项目组前的 `📁` 移除（折叠指示器 `▸` 已够，多余）
  - **星标 `★/☆` 保留**（颜色高亮区分状态，且没有跨平台问题）
- **GitHub Actions CI** —— `.github/workflows/ci.yml`（push/PR 触发：rust fmt + clippy + test + frontend tsc + vite build）+ `release.yml`（`v*` tag 触发：tauri build + SHA256 + 自动 GitHub Release 发布）。
- **关键路径 tracing 埋点** —— `list_history_projects` / `list_history_sessions_in_project` / `read_session_jsonl` / `replay_and_mark_ready` 各加 elapsed_ms 日志，便于生产诊断慢点。

### 变更

- **TabBar 局部更新（refreshTabBar 差量 DOM）** —— 引入 `TabManager.tabButtons` 缓存：每个 Tab button 只创建一次，refresh 时只同步 class（active/archived/has-unread）+ 文本，按 `orderedIds` 顺序用 `insertBefore` 排序。Visibility 全交 CSS 控制。长 session 每秒数十次 `onLine` 时 DOM thrash 减少约 80%。
- **`TabManager.orderedIds: string[]`** —— 与 `tabs.keys()` 顺序一致的稳定数组，避免 `cycleActive` / `closeTab` 每次 `Array.from` O(N) 分配。
- **`session_map.bring_terminal_to_front` 重构** —— 160 行内嵌逻辑拆为 4 个纯函数（`build_ancestors` / `build_search_terms` / `classify_window` / `select_best_window`）+ `enum MatchTier`。主函数缩到 ~40 行做 orchestration。
- **`utils::days_from_civil`** —— `subagent.rs` 与 `history.rs` 各自的副本合并到新 `utils.rs`，单源。

### 修复

- `session_map.SessionInfo.status` / `SessionMap::load` / `SessionMap::get` / `SessionChange.added` / `messages::ContentBlock` 等死代码清理。cargo check 0 warnings。

### 测试

- 单元测试 15 → 29。新增 14 个覆盖 `build_ancestors`（链 / 环 / 缺失 parent）、`build_search_terms`（边界）、`classify_window`（5 个 tier 分支 + explorer 排除 + unrelated）、`select_best_window`（多 tier 共存 + 全无命中）。

---

## [1.5.0] — 2026-05-20

首个发 exe 的 release。

### 新增

- **历史会话浏览器**（顶栏 📜 / `Ctrl+H` / Esc）
  - 按**工作目录分组**展示所有历史 jsonl，项目默认折叠
  - **两级懒加载**：初次打开仅读项目级元数据（< 100ms，500 项目）；展开某项目才读其下会话详情；同项目再次展开秒开（缓存）
  - 操作：`★` 标星 · `✎` 重命名（中文 OK）· `–` 隐藏 · `↺` 恢复（拉起 wt.exe / cmd 跑 `claude --resume`）· `✕` 物理删除（二次确认）（v1.5 时是 emoji，v1.6 改纯字符）
  - 点击会话行进入**只读消息查看器**（复用实时 Tab 的渲染管线：Markdown / KaTeX / 代码高亮 / 折叠卡）；Esc 二级关闭（先关查看器再关视图）
  - 搜索框：匹配项目名 / 路径；已缓存项目附加匹配 ai_title / customTitle / first_user_excerpt
  - 用户元数据存 `<monitor_data_dir>/history-metadata.json`（永远在默认位置，不随 claudeDir 切换）

- **Claude 数据目录可配置**（设置面板 → 数据 → Claude 数据目录）
  - 三级回退：① 设置面板配置 `claudeDir` → ② `$CLAUDE_CONFIG_DIR` 环境变量 → ③ `~/.claude` 默认
  - 改后弹"需要重启 monitor"提示
  - 支持文件夹选择对话框（`tauri-plugin-dialog`）

- **vite 端口可配** —— `VITE_PORT` 环境变量覆盖默认 1420，HMR 端口自动 = port + 1

### 修复

- **鼠标光标卡死**（选中文本 / 关闭终端后偶发"鼠标卡为手型、点击无响应、滚动可用"）
  - 根因：jsonl-line 事件大量积压时主线程被 `marked.parse` + `hljs.highlightAuto` 同步渲染压垮
  - 修复：`events.ts` 改批量调度（≤40 条/批，≤8ms/批，`setTimeout(0)` 让出主线程）；`render.ts` 砍 `hljs.highlightAuto`（无 lang 时直接 escape，10kB 代码块 30-50ms → ~0ms）

- **resume 报错 0x80070002**（ERROR_FILE_NOT_FOUND）
  - 根因：旧代码 `wt.exe -d <cwd> pwsh -NoExit -Command "..."`，但 `pwsh.exe` 是 PowerShell Core 独立安装包，不是 Windows 自带
  - 修复：改用 `cmd /K "claude --resume <id>"`，`cmd.exe` 永远在系统目录可用；Plan B 用 `CREATE_NEW_CONSOLE` flag 兜底

- **关闭 Tab 后 DOM 引用残留**：`closeTab` 显式 `clear()` toolUseNames / toolUseElements Map，加速 GC

- **跨电脑硬编码路径**：`paths.rs` 抽出 `resolve_claude_dir()` 三级回退；`session_map.rs:147` 把 `cwd.rsplit(['\\','/'])` 换成 `Path::file_name()`

- **生产 panic 路径**：`watcher.rs:32` 把 `.expect()` 改成日志降级；`session_map.rs:78` `.ok()` 吞错改成 `tracing::error!`

### 变更

- **event_replay 取消 5000 条 cap** —— 历史塞全部，重启清
- **watcher 取消初始 1500 行截断** —— 全量读，由 event_replay 持锁保证顺序
- **HMR 强制 `window.location.reload()`** —— 避免部分热替换导致状态错乱
- **过滤 Claude Code CLI 内部 prompt 包装** —— `<task-notification>` / `<system-reminder>` / `<local-command-caveat>` / `<synthetic>` 不入消息流

### 打包

- `productName` 从 `Claude Code` 改为 `cc-monitor`（避免与 Anthropic 官方品牌冲突）
- `identifier` 从 `com.local.monitor` 改为 `com.ccmonitor.app`（稳定反域名）
- 新增 `publisher` / `copyright` / `longDescription` / NSIS `installMode: perMachine` + 中英双语
- 新增项目根 `LICENSE` (MIT)；`Cargo.toml` / `package.json` 补 metadata
- 删除 `tray-icon` feature（实际未使用）

---

## [1.4.0] — 2026-05-15（v1.5 前最后一版基准）

### 新增

- **`bring_terminal_to_front`**（Tab ↗ 按钮 / `Ctrl+\``）
  - 4 阶段 HWND 匹配：① 祖先链 PID + title 含 ai-title/项目名 ② 祖先链任意窗口 ③ 终端类进程 + title 匹配 ④ 终端类任一窗口
  - WT 单进程多窗口下落到 D 级，需用户在 PowerShell startup 设独特 console title 才能区分

- **`tool_result` 合并到 `tool_use` 折叠条**：展开同一个折叠看 args + output，output 自身嵌套二级 details

- **代码块"复制"按钮 + 语言标签**：每个 code block 顶部条

### 修复

- **PID 复用导致 4 个僵尸 Tab**：探活补回 procStart 校验（100ms 容差）
- **save_config 第二次失败**：Windows `std::fs::rename` 目标存在时失败，改用 `MoveFileExW(MOVEFILE_REPLACE_EXISTING)`

### 移除

- **U2 焦点自动同步**：Win11 WT 单进程多窗口 OS API 无法区分 tab/window，整功能删除
- **SessionStart hook 路线**：改为直读 `~/.claude/sessions/<PID>.json`，零侵入
- **subagent 独立 Tab + `↳` 前缀**：嵌入到父 Task 折叠卡

---

## [1.0.0 – 1.3.x] — 2026-04 至 2026-05 早期

M1 + M2 + M3 + M4 + M5 阶段（依次）：

- 单 session MVP + watcher + 全类型 JSONL 解析 + 基础 Markdown
- 多 Tab + SessionMap + 进程探活
- 富渲染：LaTeX + 代码高亮 + tool 卡 + thinking + ai-title
- subagent Task 折叠卡 + description 关联
- 设置面板 GUI（颜色 + 字体）+ Ctrl+Tab/W/, 快捷键
- UI 全面对齐 claude.ai 视觉语言：warm gray-brown + 橙 accent + serif 正文 + user 气泡靠右

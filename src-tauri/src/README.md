# `src-tauri/src/` —— "调出对应终端" 机制设计

本 README 专门讲一个机制：**前端 Tab 的 `Ctrl+\`` 快捷键 / ↗ 按钮，把这个 Tab
对应的那个 Windows Terminal / cmd / pwsh 窗口调到前台**。

为什么单独写一份？因为它是 cc-monitor 里唯一需要做 OS API 级 IPC 的功能，
横跨前后端 4 层，且包含一个 4-tier 启发式匹配算法（A/B/C/D）——读懂 session_map.rs
不靠这份文档很费劲。

模块级（每个 `.rs` 文件做什么）的索引看上一级目录的
[`../README.md`](../README.md)。

---

## 1 · 整条链路

```
┌───────────────────────────────────────────────────────────────────┐
│ 用户操作                                                            │
│   ① Ctrl+`           ② Tab ↗ 按钮点击                              │
├──────────┬─────────────────────┬─────────────────────────────────┤
│ src/     │ main.ts:125-141     │ tabs.ts:374-378                  │
│ (前端)    │  keydown 监听        │  click 监听                       │
│          │   ↓                 │   ↓                              │
│          │ tabs.ts:281-288     │  共用同一个 helper                  │
│          │  状态门控（archived  │                                  │
│          │  Tab 跳过）          │                                  │
│          │   ↓                 │                                  │
│          │ tabs.ts:451-455     bringTerminalToFront(sessionId)    │
│          │                       ─── invoke "bring_terminal_..." ─┐
└──────────┴─────────────────────────────────────────────────────────┘
                                                                    │
┌───────────────────────────────────────────────────────────────────┘
│ src-tauri/src/lib.rs:171-178      #[tauri::command]
│   fn bring_terminal_to_front(session_id, map)
│     -> map.bring_terminal_to_front(&session_id)
│
├─── session_map.rs:114-178   SessionMap::bring_terminal_to_front
│
│     ① 查 by_id 拿 SessionInfo (pid / cwd / name=ai-title)
│     ② process_info_snapshot()         —— Win32 ToolHelp 全进程快照
│     ③ enumerate_top_level_windows()   —— Win32 EnumWindows 全顶层窗口
│     ④ build_ancestors(pid, snap)      —— 沿 parent 链向上收集祖先 PID
│     ⑤ build_search_terms(cwd, name)   —— 构造按优先级排序的 title 匹配项
│     ⑥ select_best_window(...)         —— 4-tier 决策选出最佳窗口
│     ⑦ activate_window(hwnd)           —— SetForegroundWindow（+ 还原最小化）
│
└─── (失败时 Err(String) → 前端 console.warn 不打扰用户)
```

---

## 2 · 4-tier 匹配策略

**核心难点**：一个 Claude Code 会话的 PID 不等于它的终端窗口的 PID。
真实进程树长这样：

```
WindowsTerminal.exe (有窗口)
 └─ OpenConsole.exe
     └─ pwsh.exe
         └─ claude.exe / node.exe   ← sessions/<PID>.json 里记的 pid
```

`claude.exe` 自己**没有 HWND**——它是终端里的子进程。要找它的终端窗口，得沿
parent 链往上走、或者 fallback 到任意终端类进程的窗口。所以有 4 个 tier：

| Tier | 名称 | 命中条件 | 何时使用 |
|---|---|---|---|
| **A** | `AncestorWithTitle` | 窗口的 PID 在祖先链上 **且** title 含 search term | 最理想：独立 conhost / 单 PID 单窗口的 cmd/pwsh，且窗口标题被设过 |
| **B** | `AncestorAny` | 窗口的 PID 在祖先链上（title 不限） | 独立 conhost / 单 PID 单窗口的 cmd/pwsh，但标题没设 |
| **C** | `TerminalWithTitle` | 窗口属于"终端类"进程 **且** title 含 search term | WT 多窗口、用户在 PS startup 里设过 `$Host.UI.RawUI.WindowTitle` |
| **D** | `TerminalAny` | 窗口属于"终端类"进程（兜底） | WT 多窗口、用户**没**设独特 title——所有 session 落同一个 WT 窗口 |

"终端类进程"白名单见 `is_terminal_process()`（session_map.rs:358）：
`windowsterminal.exe / wt.exe / conhost.exe / openconsole.exe / cmd.exe /
powershell.exe / pwsh.exe / mintty.exe / alacritty.exe / wezterm-gui.exe /
tabby.exe`。

"系统 shell"黑名单见 `is_system_shell_process()`（session_map.rs:339）：
`explorer.exe / dwm.exe / svchost.exe / ...`——这些进程经常出现在祖先链顶端
（比如 `pwsh.exe` 从开始菜单启动时 parent 是 `explorer.exe`），但显然不是
"终端窗口"。即使在祖先链里也直接判 `None`。

### 优先级如何在代码里实现？

```rust
enum SelectResult<'a> {
    Single(&'a WindowSnap, MatchTier),
    Ambiguous { tier: MatchTier, candidates: Vec<&'a WindowSnap> },
    NoMatch,
}

fn select_best_window(...) -> SelectResult<'_> {
    let mut buckets: [Vec<&WindowSnap>; 4] = [vec![], vec![], vec![], vec![]];
    for w in windows {
        let Some(tier) = classify_window(w, snap, ancestors, search_terms) else {
            continue;
        };
        buckets[tier as usize].push(w);   // ← 不再"只记第一个"，全部收集
    }
    // 按 tier 0..3 顺序返回首个非空桶：
    //   单元素 → Single
    //   多元素 → Ambiguous（修 Bug 1：WT 多窗口同 tier 时让调用方报歧义错）
    ...
}
```

`MatchTier` 的 4 个变体按 `AncestorWithTitle=0..TerminalAny=3` 排列，
`as usize` 直接当数组下标——加 tier 只要在 enum 里加一个变体 + 改数组长度。

**v1.6.3 修正**：旧实现 `Option<(&WindowSnap, MatchTier)>` + "同 tier 只记首个"
在 WT 单进程多窗口下会随机选窗口。新实现把同 tier 多候选返 `Ambiguous`，
`SessionMap::bring_terminal_to_front` 报包含候选 hwnd/title + WT 配置建议
的详细错给前端 `console.warn`。

---

## 3 · 4 个纯函数（**全部有单元测试**）

| 函数 | 输入 | 输出 | 测试 |
|---|---|---|---|
| `build_ancestors(start_pid, &snap)` | 起始 PID + ToolHelp 进程快照 | `HashSet<u32>` 祖先 PID 集 | 链 / 环 / 缺失 parent |
| `build_search_terms(&cwd, ai_title)` | cwd 字符串 + 可选 ai-title | 按精确度排序的 `Vec<String>` | 全字段 / 短串跳过 / 前缀截断 |
| `classify_window(&w, &snap, &ancestors, &terms)` | 单个窗口快照 + 进程快照 + 祖先链 + 搜索项 | `Option<MatchTier>` | 5 个 tier 分支 + explorer 排除 + 不相关返 None |
| `select_best_window(&windows, ...)` | 全部窗口 + 上述三件套 | `SelectResult { Single \| Ambiguous \| NoMatch }` | 多 tier 共存挑最高 + 同 tier 多候选 → Ambiguous + 无命中 → NoMatch |

**关键设计**：这 4 个函数**不调用任何 Win32 API**。输入全是值类型 + 引用，
输出全是值类型。测试可以直接构造 `HashMap<u32, ProcInfo>` 喂进去验证决策
——不需要起进程、不需要 mock。

测试见 `session_map.rs:706-890`（`#[cfg(test)] mod tests`，
其中 4 个 `#[cfg(windows)] mod matcher_win` 因为依赖 `WindowSnap` 持有
`windows::Win32::Foundation::HWND` 类型）。

### `build_search_terms` 的优先级

```
1. ai-title 全字                 e.g. "filter-active"
2. ai-title 前 12 字前缀         e.g. "abcdefghijkl"（应对 WT title 截断）
3. cwd 最后一段（项目名）         e.g. "claudecode-frontend"
4. 项目名前 8 字前缀             e.g. "claudeco"
```

- 小于 4 字符的 term 直接跳过——避免 "src" 这种短串误命中
- ai-title 来自 `sessions/<PID>.json` 的 `name` 字段（Claude Code 同时把它
  设到 `$Host.UI.RawUI.WindowTitle`，所以 WT tab title 实际就是这个值）
- 项目名取自 cwd 的 basename

---

## 4 · Win32 I/O 三件

| 函数 | API | 干嘛 |
|---|---|---|
| `process_info_snapshot()` | `CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS)` + `Process32First/Next` | 一次性拍下所有进程的 (pid, parent, exe_name) |
| `enumerate_top_level_windows()` | `EnumWindows` + `IsWindowVisible` + `GetWindow(GW_OWNER)==0` + `GetWindowThreadProcessId` + `GetWindowTextW` | 拍下所有可见 top-level（无 owner）窗口的 (hwnd, pid, title) |
| `activate_window(hwnd)` | `IsIconic` + `ShowWindow(SW_RESTORE)` + `SetForegroundWindow` | 拉前。最小化时先还原 |

注意：`enumerate_top_level_windows` 用 `thread_local!` 收集 callback 结果——
EnumWindows callback 是 raw C ABI，无法持有 Rust 闭包环境，要靠 thread-local
桶在 callback 外读取。

---

## 5 · 已知限制

**WT 单进程多窗口共享 PID**：Windows Terminal 一个 wt.exe 进程可以开多个窗口，
所有窗口的 PID 相同，HWND 不同。如果用户没在 PowerShell startup 设独特 title：

```powershell
$Host.UI.RawUI.WindowTitle = Split-Path -Leaf $PWD
```

那 tier A/C 因为 title 不含 search term 不会命中，所有 session 都会落到
tier D 的"第一个 WT 窗口"。诊断信息：tier D 命中时会 `tracing::info!` 打全部
终端类窗口的 (hwnd, title) 列表，方便用户判断需不需要设 title。

**SetForegroundWindow 偶尔被 OS 拒**：Windows 限制只有"近期有用户交互"的进程
能拉别人到前台。被拒时窗口会在任务栏闪烁（OS fallback 行为）。我们不视为
致命错误，返 `Err("SetForegroundWindow refused...")` 让前端 `console.warn`，
不打扰用户。

**非 Windows 平台**：`#[cfg(not(windows))]` 直接返 `Err("only supported on Windows")`。
cc-monitor 本来就只发 Windows 包，但保留 stub 让 Rust 编译能跨平台过 fmt/clippy。

---

## 6 · 解耦评估（当前状态）

✅ **纯逻辑（4 个匹配函数 + MatchTier 枚举）**：零 Win32 依赖，全部可单元测试，
覆盖 14 个测试用例
✅ **Win32 I/O（3 个函数）**：各管一件事，无业务逻辑混入
✅ **进程白/黑名单**：`is_terminal_process` / `is_system_shell_process` 独立常量函数
✅ **前端**：键盘和按钮共用一个 `bringTerminalToFront(sid)` helper，状态门控
（archived Tab 跳过）在 `bringActiveTerminalToFront` 一处
✅ **IPC**：`lib.rs` 的命令是纯 dispatch，没有业务逻辑

⚠️ **小瑕疵（不影响使用）**：`SessionMap::bring_terminal_to_front` 是方法
而非自由函数——`SessionMap` 同时承担"活跃 session 管理"和"终端窗口拉前"两个
职责。理论上可以拆：

```rust
// lib.rs
let info = map.get(&session_id).ok_or(...)?;
session_map::bring_terminal_to_front(&info)
```

不过 `SessionInfo` 的 owner 就是 `SessionMap`，现在这样可接受。如果未来活跃
管理逻辑独立成模块，再拆出 free function 也容易。

---

## 7 · 涉及文件清单

```
src/main.ts                                  ← Ctrl+` keydown 注册
src/tabs.ts                                  ← ↗ 按钮、bringActiveTerminalToFront、invoke 包装
src-tauri/src/lib.rs:171-178                 ← #[tauri::command] bring_terminal_to_front
src-tauri/src/session_map.rs:99-178          ← SessionMap::bring_terminal_to_front 编排
src-tauri/src/session_map.rs:186-329         ← 4 个纯函数 + MatchTier
src-tauri/src/session_map.rs:331-494         ← Win32 I/O 三件
src-tauri/src/session_map.rs:706-890         ← 单元测试（14 个）
```

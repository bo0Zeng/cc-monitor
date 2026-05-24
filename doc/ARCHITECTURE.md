# 架构总览

新贡献者第一站。读完应该能回答：数据从哪儿来、经过谁、停在哪儿、为什么这么分。

各模块的详细职责见 [`../src-tauri/README.md`](../src-tauri/README.md) 和 [`../src/README.md`](../src/README.md)。State / 撤回 checklist 见 [STATE.md](STATE.md)、[CHECKLIST.md](CHECKLIST.md)。

---

## 1. 数据流（实时通道）

```
   ┌──────────────────────┐                    ┌──────────────────────┐
   │  Claude Code CLI     │                    │  PowerShell session  │
   │  (你跑 `claude`)     │                    │  (跑 cc / __ccm_bind)│
   └──────────┬───────────┘                    └──────────┬───────────┘
              │ 写                                         │ 改窗口标题 + 写
              ▼                                            ▼
   ~/.claude/projects/                              ~/.claude/claudecode-frontend/
       <cwd>/<sid>.jsonl                                ps-await/<PID>.json
   ~/.claude/sessions/<PID>.json                        ps-registry/<PID>.json
              │                                            │
              │ notify-debouncer 监听              EnumWindows 找 marker
              │                                            │
              ▼                                            ▼
   ┌───────────────────────────────────────────────────────────────────┐
   │                          Rust 后端 (Tauri)                         │
   │                                                                    │
   │   watcher.rs  ────► parser.rs ────► messages::JsonlRecord         │
   │       │                                       │                    │
   │       ▼ active filter                         ▼                    │
   │   session_map.rs (PID 探活)            event_replay.rs (内存 buf)  │
   │       │                                       │                    │
   │       │                                       │ emit("jsonl-line") │
   │       │                                       ▼                    │
   │   bind.rs (ps-await/ps-registry/SidHwndCache, EnumWindows)         │
   │       │                                       │                    │
   │       │  invoke("bring_terminal_to_front")    │                    │
   │       └──────────────────┬────────────────────┘                    │
   │                          ▼                                          │
   │                     Tauri IPC                                       │
   └──────────────────────────┬──────────────────────────────────────────┘
                              │
                              ▼
   ┌──────────────────────────────────────────────────────────────────┐
   │                  TypeScript 前端 (WebView2)                       │
   │                                                                    │
   │   events.ts (订阅 + 批量调度) ─► tabs.ts (TabManager)              │
   │                                       │                            │
   │                                       ▼                            │
   │                              stream.ts (MessageStream)             │
   │                                       │                            │
   │                                       ▼                            │
   │     render.ts (marked + KaTeX + hljs + DOMPurify) ─► DOM           │
   └──────────────────────────────────────────────────────────────────┘
```

**关键不变量**：

- watcher / session_map 只读 `~/.claude/projects/` 和 `~/.claude/sessions/`，绝不写。
- 唯一写 `~/.claude/` 的路径是用户**显式**触发：历史浏览器删除 + PowerShell profile 安装。
- 跨进程通信（PS ↔ monitor）走文件 IPC（详见 §3），不开网络端口、不长连接。

---

## 2. Tauri State 注册矩阵

`src-tauri/src/lib.rs::run().setup()` 注册 4 个 Arc-shared State：

| State 类型 | 持有者 | 喂给的 IPC 命令 |
|---|---|---|
| `Arc<SessionMap>` | setup 闭包 + `session-changes-emitter` 线程 + 1 个 active-filter 闭包 + `app.manage` | `list_history_projects` / `list_history_sessions_in_project` |
| `Arc<EventReplay>` | setup 闭包 + frontend-ready listener + jsonl async pump + `app.manage` | `forget_session` |
| `Arc<BindRegistry>` | setup 闭包 + `bind-await-watcher` 线程 + `bind-heartbeat` 线程 + `session-changes-emitter` 线程 + `app.manage` | `cc_integration_status` |
| `Arc<SidHwndCache>` | setup 闭包 + `session-changes-emitter` 线程 + `app.manage` | `bring_terminal_to_front` |

**约束**：撤回任何 State 必须先 grep 所有 `State<'_, Arc<X>>` 消费者；漏 manage 不会被 `cargo check` 抓住，运行时 panic。详细教训和 grep 流程见 [STATE.md](STATE.md)。

---

## 3. 跨进程文件 IPC 协议

monitor 与外部进程的所有通信都在 `~/.claude/claudecode-frontend/` 下：

| 路径 | 写入方 | 读取方 | 用途 | 生命周期 |
|---|---|---|---|---|
| `config.json` | monitor 设置面板 | monitor 启动 | 主题 / 字体 / claudeDir override | 持久 |
| `ps-await/<PID>.json` | PowerShell (`__ccm_bind`) | monitor (`bind::BindRegistry`) | PS 通知 monitor "我想绑定，去找标题 = marker 的窗口" | 短暂 (800ms 超时) |
| `ps-registry/<PID>.json` | monitor | PowerShell (查 + 比较 procStart) | monitor 通知 PS "绑定成功，HWND = X" | 与 PS 进程同寿 |
| `sid-hwnd-cache.json` | monitor | monitor 启动恢复 | sid → hwnd 持久缓存，新 session 出现时查这里复用绑定 | 持久 |
| `auto-launch.json` | monitor 设置面板 | PowerShell (`__ccm_bind` 头部) | "用 cc 启动 claude 时自动开 monitor" 开关 + monitor exe 路径 | 持久 |
| `history-metadata.json` | monitor 历史浏览器 | monitor 历史浏览器 | star / 重命名 / 隐藏 | 持久 |

**协议关键点**：

- 所有 JSON 必须 **UTF-8 无 BOM**。PS 5.1 `Out-File -Encoding utf8` 写 BOM，serde_json 解析失败——v1.7.0–1.7.7 cc 集成"装上没用"的真凶。模板 `cc.ps1.tpl` 用 `[System.IO.File]::WriteAllText` + `UTF8Encoding($false)`；Rust 端 `bind::process_await_file` 防御性 `trim_start_matches('\u{feff}')`。
- 握手序列：PS 写 `ps-await/<PID>.json` + 改窗口标题为 marker → monitor `EnumWindows` 找标题匹配的窗口拿 HWND → 写 `ps-registry/<PID>.json` 并 `Remove-Item ps-await/<PID>.json` → PS 检测到 await 被删，恢复原窗口标题。超时 800ms 算失败。
- 「原子写」语义不统一：`config.rs::atomic_replace` 走 `MoveFileExW(REPLACE_EXISTING)` 真原子；其他 4 处（bind / auto_launch / profile_installer / history）走 `write tmp + remove + rename`，不严格原子。

---

## 4. 设计分层

```
src-tauri/src/
├── 入口层      lib.rs (Tauri builder + setup + invoke_handler 注册)
├── 边界层      paths.rs (claudeDir 解析) | bridge.rs (事件常量 + payload)
├── 读取层      watcher.rs | parser.rs | messages.rs | session_map.rs | subagent.rs
├── 业务层      event_replay.rs (重放) | history.rs (历史浏览器)
├── 集成层      bind.rs (cc 集成绑定) | profile_installer.rs (profile 写入) | auto_launch.rs
└── 持久层      config.rs (monitor 自己的配置)
```

```
src/
├── 入口        main.ts (快捷键、HMR reload)
├── 事件        events.ts (订阅 + 批量调度让出主线程)
├── 状态        tabs.ts (TabManager) | stream.ts (MessageStream)
├── 渲染        render.ts (marked + KaTeX + hljs + DOMPurify) | cards/
├── 视图        views/history.ts | views/session-viewer.ts
├── 设置        settings/panel.ts | settings/cc_integration.ts
└── 配置 / 主题  config.ts | paths.ts | theme.ts
```

---

## 5. 几个曾经踩过的坑（避免重蹈）

| 时期 | 坑 | 当前处理 |
|---|---|---|
| v1.6.7 撤回 `bring_terminal_to_front` 时漏删 `app.manage(session_map.clone())` | history 模块运行时 panic，五个版本带病 | STATE.md 强制 grep checklist |
| v1.7.0–1.7.1 装到 `profile.ps1` 而非 `Microsoft.PowerShell_profile.ps1` | PowerShell 启动不读，cc 集成形同虚设 | v1.7.2 改用默认 `$PROFILE`；v1.7.12 又明确认识到 `profile.ps1` (AllHosts) 也是合法位置，作中性提示 |
| v1.7.0–1.7.7 PS 5.1 `Out-File -Encoding utf8` 写 BOM | serde_json 不剥 BOM，解析失败，看似装好实则零握手 | bind 端剥 BOM；模板用无 BOM 写入 |
| v1.6.x 拉错 / 拉不到终端 | 在 explorer 启 PS + WT DefTerm 时 claude 祖先链跟 WT 窗口完全脱节 | v1.7 改 PS profile 注入式绑定，PS 自己把 HWND 注册给 monitor |
| Windows 路径大小写不敏感 | notify 重复回放 | `watcher.rs::path_key()` 用小写归一 |
| v1.7.0–1.7.9 atomic_write 三步走（write tmp + remove + rename）非原子 + tmp 文件继承父目录 ACL 覆盖 dst explicit ACE | rename 失败丢 profile / 用户 Documents 重定向到非默认盘时装完 PS 启动报 Access denied | v1.7.10 改 Win32 `ReplaceFileW`（保留 dst ACL/ADS/创建时间）+ 写前 backup + 写后回读校验 |
| v1.7.9 设置面板 [打开 profile] 按钮点了 alert "opener.open_path not allowed" | capability `opener:default` 不含 `allow-open-path` permission（实测 `acl-manifests.json` default permission set 是 [allow-open-url, allow-reveal-item-in-dir, allow-default-urls]）；单独加 `allow-open-path` 还不够（默认空 scope） | v1.7.11 capability 加 inline scoped permission entry `{ identifier: "opener:allow-open-path", allow: [{ path: "**" }] }` |
| v1.7.13 之前 settings 面板 tooltip 用 `position: absolute` 被 `.settings-body { overflow-y: auto }` 在某个方向裁切 + 改 `position: fixed` 后又被 `.settings-panel { transform }` 把 containing block 从 viewport 重置到 panel | hover 完全看不到 tooltip | v1.7.13 portal：tooltip DOM 挂到 `document.body`（脱离 transformed 祖先子树）+ `position: fixed` + JS 算 viewport 坐标 + 边界感知 |

每个坑修复时都更新了对应模块的 `//!` doc comment，并在 [CHANGELOG.md](../CHANGELOG.md) 的对应版本段写了复盘。新加坑请遵循同样模式。

---

## 6. 当前没解决的小事

- **lib.rs setup 是个 ~150 行的"上帝构造器"**——4 个 State + 3 个 spawn + watcher wiring + active_filter 闭包都堆这里。可以拆 `bootstrap::{init_paths, init_state, spawn_session_emitter, spawn_jsonl_pump}`。
- **6 处独立 "atomic write" 实现**（bind.rs 有 2 处：`atomic_write_json` + `SidHwndCache::persist` 内联；其余 auto_launch / profile_installer / history / config）应合并到 `utils::atomic_write_json`。注意 profile_installer 自 v1.7.10 用 ReplaceFileW（保留 dst ACL），跟其他几处的 remove+rename 行为有别——合并时要区分"内容替换保留 dst metadata" vs "纯内容写"两种语义。
- **事件名 / IPC 命令名**在 TS 端是字面量散布；后端 `bridge::events` 是单一来源但前端没对应 import。
- **没有 graceful shutdown**：所有 spawn 的线程 `loop { recv() }` 无退出信号，靠 app exit OS 杀进程兜底。

这些是 nice-to-have，不影响功能；列出来给重构者参考。

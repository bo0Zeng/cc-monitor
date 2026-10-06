# Tauri State 注册矩阵

> **这份文档是 cc-monitor 撤回 / 修改任何 IPC 命令时的强制 checklist 输入。**
>
> 漏 `app.manage()` 不会被 `cargo check` 抓住——pub fn 签名编译通过，运行时一调命令才 panic。撤代码前先 grep 这个表。改代码前先更新这个表。

---

## 1. 所有注册的 State 类型

`src/frontend/shell/src/lib.rs` 的 setup() 闭包 `app.manage()` 调用。**任何 IPC 命令接受的 `State<...>` 类型必须在这里有对应注册**。

| State 类型 | 注册位置 | 创建位置 | Arc 所有权 |
|---|---|---|---|
| ~~`Arc<session_map::SessionMap>`~~ 已删 | — | — | 本机活会话表改由本机后端那条流的起停帧喂，住进程级的一张（`session_map::local()`，不 manage）；写者只有本机那条流的消费者（`stream_source::consume_local` ⇒ `session_map::feed`），读者：本机 emitter · `frontend-ready` 对账 · `list_active_sessions`〔散文墓碑〕 / `list_session_activity`〔散文墓碑〕 |
| `Arc<event_replay::EventReplay>` | `lib.rs::setup()` `app.manage(replay.clone())` | `EventReplay::new()` | 共享：setup 局部 + frontend-ready listener + jsonl async pump + State |
| `Arc<bind::BindRegistry>` | `lib.rs::setup()` `app.manage(bind_registry.clone())` | `BindRegistry::spawn()` | 共享：setup 局部 + `session-book-emitter` 线程 + `bind-await-watcher` 线程 + `bind-heartbeat` 线程 + State（另起的 `monitor-up-mark` 线程只占着两样系统对象，不持有它） |
| `Arc<bind::SidHwndCache>` | `lib.rs::setup()` `app.manage(sid_hwnd_cache.clone())` | `SidHwndCache::load()` | 共享：setup 局部 + `session-book-emitter` 线程 + State |
| `Arc<logging::LoggingState>` | `lib.rs::setup()` `app.manage(logging_state.clone())` | `logging::init(monitor_data_dir)`（在 `tauri::Builder` 之前） | 共享：`lib.rs::run()` 局部（持有 WorkerGuard 到 setup 结束）+ setup 闭包内 `install_error_emitter` 注入 closure + State |
| ~~`Arc<search::SearchIndex>`~~ (issue #6) 已删 | — | — | 本机全文搜索改问本机后端（`history-search`），monitor 不再建索引 |
| ~~远端会话的窗口缓存~~ (issue #18) 已删 | — | — | 远端 ↗ 点那一刻现查（那台答「此刻谁在显示它」、本机后端对到窗口），monitor 不再为远端会话记窗口 |

---

## 2. 消费者矩阵（IPC 命令）

任何 `#[tauri::command]` 函数签名里出现 `State<...>` 就是这里的消费者。

### `Arc<SessionMap>`
- 历史清单不接 `State`：清单与判活住本机常驻后端（`history-list`）。
- 红绿灯启动快照与启动骨架清单那两条命令〔已删〕：本机骨架与初始灯是会话流里的 `live` / `activity` 成品（就绪点按 `session_book.rs::Book` 重放）。

### `Arc<EventReplay>`
- `lib.rs::forget_session(session_id, replay: State<'_, Arc<EventReplay>>)`
- `chan/webview.rs::chan_subscribe(webview, origin, kind, from, want, id, replay: State<'_, Arc<EventReplay>>)` · `chan_want` · `chan_stop`（会话内容经通道 `subscribe`：主窗口每台机器一条 `session-lines`，独立窗口一条 `session-lines/<sid>`；原来的定向重放命令〔散文墓碑〕退役）

### `Arc<BindRegistry>`
- `lib.rs::bound_terminal_count(bind_state: State<'_, Arc<BindRegistry>>)`（握手终端数单独一问；从前夹在别名读回口里 —— 别名那一族进了那台后端，`aliases-read`）
- `lib.rs::bring_remote_terminal_to_front(chain, bind_state: State<'_, Arc<BindRegistry>>)`（远端会话的 ↗：沿进程链先查握手表）

### `Arc<SidHwndCache>`
- `lib.rs::bring_terminal_to_front(session_id, cache: State<'_, Arc<SidHwndCache>>)`

### 远端 ↗ (issue #18)
- `lib.rs::bring_remote_terminal_to_front(window)`：不接 `State`，交来的是本机后端对上的窗口（`bind::FoundWindow`）。

### `Arc<LoggingState>`
- `lib.rs::get_diagnostics_config(state: State<'_, Arc<logging::LoggingState>>)`
- `lib.rs::set_diagnostics_config(cfg, state: State<'_, Arc<logging::LoggingState>>)`
- `lib.rs::get_log_file_info(state: State<'_, Arc<logging::LoggingState>>)`
- `lib.rs::open_log_file(state: State<'_, Arc<logging::LoggingState>>)`
- `lib.rs::open_log_dir(state: State<'_, Arc<logging::LoggingState>>)`

### ~~`Arc<SearchIndex>` (issue #6)~~ 已删
本机全文搜索改问本机后端（帧命令 `history-search`，界面经通道问，与远端同一条路）；monitor 进程内那份索引与它的三条命令一起删了，这一格 State 不再有。

### 无 State 依赖（自包含 / 用 path 解析）
- `config::load_config / patch_config`（用 `paths::resolve_config_path`；写经进程级锁，见 `config.rs::patch_config_at`）
- `launch::open_terminal_window` · `launch::terminal_dial`（B14-F41；后者用 `lib::load_remote_config_by_label` 读配置文件，ssh 那一行由本机后端 `terminal-ssh` 渲）
- `aliases_block_render / aliases_block_install / aliases_block_remove`（`origin` ＋ `rc_path`；装 / 卸经那台后端写）
- `cc_get_auto_launch / cc_set_auto_launch`（用 `paths::resolve_monitor_data_dir`）
- `history::stream_read_session_jsonl / update_history_metadata / resume_history_session`（v2.6 删了非流式 `read_session_jsonl`；删会话 · F62 分叉两条改由界面经通道直说那台后端，monitor 这一侧已无命令）
- `tasks::get_session_tasks` (v2.3 issue #11)：用 `paths::resolve_claude_dir().join("tasks")`，session_id 参数直接拼路径；watcher 线程独立 spawn 不通过 State 共享
- `data_paths::get_data_paths` (v2.3 issue #3 A)：用 `paths::resolve_monitor_data_dir()` + `AppHandle.path().app_local_data_dir()` 推断 WebView2 路径；纯 stat 不持有状态
- `bring_monitor_to_front` (v2.4 issue #2)：通过 `AppHandle.get_webview_window("main")` 直接拿主窗口；三层 Win32 hack 拉前（详 ARCHITECTURE.md § 5「bring_monitor_to_front 三层 hack」；其中 HWND 跨 windows crate 版本互操作详 INVARIANTS § 19）；无外部 State

---

## 3. 跨线程 / 跨闭包 Arc 持有清单

Arc 不只通过 State 共享，还通过 `.clone()` 喂给 spawn 出去的线程 / async task。**任何 State 类型如果还有 thread 持有它，删 State 也必须看这些 thread 是否还需要**。

| Arc | 还在哪持有 |
|---|---|
| 会话成品缓存 `session_book::book()`（进程级，**非 State**；替掉 LOC1b 那张本机活会话表） | (1) 两条流（`stream_source::stream_loop` · `consume_local`）经 `session_book::feed` 交成品（**唯一写口**） (2) `session-book-emitter` 线程（转交会话流 ＋ 拉前绑定） (3) 就绪点 / 开窗重放（`event_replay.rs::lifecycle_replay`） |
| ~~`remote_active`~~ 已删（THIN 09-29 订正本行） | 连同写它的那条远端会话 emitter 线程一起没了：活 / 可重连 / 已结束的账住那台后端 `src/backend/observe/session_ledger.rs::SessionLedger`（发帧唯一出口上那一问），monitor 只收成品、存进上一行那本 `session_book`（INVARIANTS § 24 顶注） |
| `bind_registry` | (1) `BindRegistry::spawn()` 内部启动的 `bind-await-watcher` + `bind-heartbeat` 两个线程 (2) `session-book-emitter` 线程 (`bind_for_emitter`) (3) `app.manage` |
| `sid_hwnd_cache` | (1) `session-book-emitter` 线程 (`cache_for_emitter`) (2) `app.manage` |
| `replay` | (1) `app.listen("frontend-ready", ...)` 闭包 (2) spawn 的 jsonl 处理 async task (3) `app.manage` |
| `search_index` | (1) `search-index-build` 后台线程（构建期持有，构建完成即 drop 该 clone）(2) `app.manage` |
| `logging_state` | (1) `run()` 局部（持有 WorkerGuard 维持 non_blocking writer thread 存活）(2) setup 闭包内通过 `install_error_emitter` 把 AppHandle wrap 成 closure 存入 state 的 emit_fn 字段 (3) `app.manage` |

**结论**：每个 Arc 都至少有 2 个非 State 消费者。App 退出前 Arc 永不 drop——这是当前架构的隐式契约（无 graceful shutdown 路径）。

---

## 4. 修改规则

### 4.1 撤回某个 State 类型（如：删 `BindRegistry`）

**全套必做步骤**：

```bash
cd src/frontend/shell

# 1. 找所有 State<Arc<X>> 引用
grep -rn 'State<.*Arc<BindRegistry>>' src/

# 2. 找所有 .clone() 跨线程
grep -rn 'bind_registry.clone()\|BindRegistry' src/

# 3. 找 app.manage 调用
grep -rn 'app.manage(bind_registry' src/

# 4. 找前端 invoke 依赖
cd .. && grep -rn 'invoke<.*"aliases_read"\|"bring_terminal_to_front"' src/

# 5. 删完跑：
cd src/frontend/shell && cargo check && cargo test --all
# !! cargo check 不能挡 State 漏 manage 的运行时 panic !!
# 必须额外起 dev mode 实测每个会消费 X 的 IPC 命令的前端入口
```

**为什么强制 grep 而非靠类型系统**：State 漏 manage 在 cargo check 时通过（pub fn 签名合法），运行时调用才 panic。曾经因为漏 grep history.rs 的 SessionMap 消费导致 5 个连续版本带病发布。grep 流程是用代价换来的硬约束。

### 4.2 加新 State 类型

1. 在 lib.rs setup 创建 `let foo = FooState::new();`
2. **立刻**在 setup 末尾 `app.manage(foo.clone());`
3. 在本表 § 1 + § 2 加一行
4. 任何 `fn cmd(foo: State<'_, Arc<FooState>>)` 都对应这一行
5. 如果跨线程持有，加到 § 3 跨线程 Arc 持有清单

### 4.3 给已有 IPC 命令加 State 参数

1. 修改函数签名加 `state: State<'_, Arc<Foo>>`
2. **立刻**到 § 2 消费者矩阵对应 State 下加一行
3. 确认对应 State 已在 setup 里 `app.manage`（看 § 1）

---

## 5. 同步规则

**本文档跟 lib.rs setup 和所有 IPC 命令签名必须对齐**。任何修改导致表格过期：
- 增删 `app.manage()` 调用 → 更新 § 1
- IPC 命令 State 参数变化 → 更新 § 2
- 新增 / 删除跨线程 Arc clone → 更新 § 3

未对齐的修改在 code review 时应该被打回。

# Tauri State 注册矩阵

> **这份文档是 cc-monitor 撤回 / 修改任何 IPC 命令时的强制 checklist 输入。**
>
> 漏 `app.manage()` 不会被 `cargo check` 抓住——pub fn 签名编译通过，运行时一调命令才 panic。撤代码前先 grep 这个表。改代码前先更新这个表。

---

## 1. 所有注册的 State 类型

`src-tauri/src/lib.rs` 的 setup() 闭包 `app.manage()` 调用。**任何 IPC 命令接受的 `State<...>` 类型必须在这里有对应注册**。

| State 类型 | 注册位置 | 创建位置 | Arc 所有权 |
|---|---|---|---|
| `Arc<session_map::SessionMap>` | `lib.rs::setup()` `app.manage(session_map.clone())` | `SessionMap::load_with_changes()` | 共享：setup 局部 + 闭包 + `session-changes-emitter` 线程 + State |
| `Arc<event_replay::EventReplay>` | `lib.rs::setup()` `app.manage(replay.clone())` | `EventReplay::new()` | 共享：setup 局部 + frontend-ready listener + jsonl async pump + State |
| `Arc<bind::BindRegistry>` | `lib.rs::setup()` `app.manage(bind_registry.clone())` | `BindRegistry::spawn()` | 共享：setup 局部 + `session-changes-emitter` 线程 + `bind-await-watcher` 线程 + `bind-heartbeat` 线程 + State |
| `Arc<bind::SidHwndCache>` | `lib.rs::setup()` `app.manage(sid_hwnd_cache.clone())` | `SidHwndCache::load()` | 共享：setup 局部 + `session-changes-emitter` 线程 + State |

---

## 2. 消费者矩阵（IPC 命令）

任何 `#[tauri::command]` 函数签名里出现 `State<...>` 就是这里的消费者。

### `Arc<SessionMap>`
- `history.rs::list_history_projects(map: State<'_, Arc<SessionMap>>)`
- `history.rs::list_history_sessions_in_project(project_dir, map: State<'_, Arc<SessionMap>>)`

### `Arc<EventReplay>`
- `lib.rs::forget_session(session_id, replay: State<'_, Arc<EventReplay>>)`

### `Arc<BindRegistry>`
- `lib.rs::cc_integration_status(command_name, bind_state: State<'_, Arc<BindRegistry>>)`

### `Arc<SidHwndCache>`
- `lib.rs::bring_terminal_to_front(session_id, cache: State<'_, Arc<SidHwndCache>>)`

### 无 State 依赖（自包含 / 用 path 解析）
- `config::load_config / save_config`（用 `paths::resolve_config_path`）
- `subagent::load_subagent`
- `cc_integration_preview / scan_path / install / uninstall`（path 参数直接进）
- `cc_get_auto_launch / cc_set_auto_launch`（用 `paths::resolve_monitor_data_dir`）
- `history::read_session_jsonl / delete_history_session / update_history_metadata / resume_history_session`

---

## 3. 跨线程 / 跨闭包 Arc 持有清单

Arc 不只通过 State 共享，还通过 `.clone()` 喂给 spawn 出去的线程 / async task。**任何 State 类型如果还有 thread 持有它，删 State 也必须看这些 thread 是否还需要**。

| Arc | 还在哪持有 |
|---|---|
| `session_map` | (1) `active_filter` 闭包（喂给 watcher） (2) `session-changes-emitter` 线程 (3) `app.manage` |
| `bind_registry` | (1) `BindRegistry::spawn()` 内部启动的 `bind-await-watcher` + `bind-heartbeat` 两个线程 (2) `session-changes-emitter` 线程 (`bind_for_emitter`) (3) `app.manage` |
| `sid_hwnd_cache` | (1) `session-changes-emitter` 线程 (`cache_for_emitter`) (2) `app.manage` |
| `replay` | (1) `app.listen("frontend-ready", ...)` 闭包 (2) spawn 的 jsonl 处理 async task (3) `app.manage` |

**结论**：每个 Arc 都至少有 2 个非 State 消费者。App 退出前 Arc 永不 drop——这是当前架构的隐式契约（无 graceful shutdown 路径）。

---

## 4. 修改规则

### 4.1 撤回某个 State 类型（如：删 `BindRegistry`）

**全套必做步骤**：

```bash
cd src-tauri

# 1. 找所有 State<Arc<X>> 引用
grep -rn 'State<.*Arc<BindRegistry>>' src/

# 2. 找所有 .clone() 跨线程
grep -rn 'bind_registry.clone()\|BindRegistry' src/

# 3. 找 app.manage 调用
grep -rn 'app.manage(bind_registry' src/

# 4. 找前端 invoke 依赖
cd .. && grep -rn 'invoke<.*"cc_integration_status"\|"bring_terminal_to_front"' src/

# 5. 删完跑：
cd src-tauri && cargo check && cargo test --all
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

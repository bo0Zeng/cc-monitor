# 架构总览

新贡献者第一站。读完应该能回答：数据从哪儿来、经过谁、停在哪儿、为什么这么分。

每个模块的"当下设计 + 为什么"详见各子目录 README — [`../src/README.md`](../src/README.md)、[`../src-tauri/README.md`](../src-tauri/README.md)、[`../scripts/README.md`](../scripts/README.md)。

跨切面文档：
- 全局不变量 → [INVARIANTS.md](INVARIANTS.md)
- 跨进程文件 IPC 协议详解 → [IPC-PROTOCOL.md](IPC-PROTOCOL.md)
- Tauri State 注册矩阵 → [STATE-MATRIX.md](STATE-MATRIX.md)
- 贡献者操作手册 + cookbook → [CONTRIBUTING.md](CONTRIBUTING.md)

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
       <encoded-cwd>/<sid>.jsonl                        ps-await/<PID>.json
   ~/.claude/sessions/<PID>.json                        ps-registry/<PID>.json
              │                                            │
              │ notify-debouncer 监听              EnumWindows 找 marker
              │                                            │
              ▼                                            ▼
   ┌───────────────────────────────────────────────────────────────────┐
   │                          Rust 后端 (Tauri)                         │
   │                                                                    │
   │   watcher.rs ─batch handler─► parser.rs ────► messages::JsonlRecord│
   │       │ (同步同线程调用)                       │                   │
   │       ▼ active filter                          ▼                   │
   │   session_map.rs (PID 探活)        event_replay::on_line_batch     │
   │       │                                       │  按大小分流（每行带 │
   │       │                                       │  watcher 给的 seq）│
   │       │                                       │  小→jsonl-line     │
   │       │                                       │  大→jsonl-batch    │
   │       ▼                                       ▼   (切块 chunk_i/N) │
   │   bind.rs (ps-await/ps-registry/SidHwndCache, EnumWindows)         │
   │       │                                       │                    │
   │       │  invoke("bring_terminal_to_front")    │                    │
   │       │  invoke("bring_monitor_to_front")     │                    │
   │       └──────────────────┬────────────────────┘                    │
   │                          ▼                                          │
   │                     Tauri IPC                                       │
   └──────────────────────────┬──────────────────────────────────────────┘
                              │
                              ▼
   ┌──────────────────────────────────────────────────────────────────┐
   │                  TypeScript 前端 (WebView2)                       │
   │                                                                    │
   │   events.ts (订阅 + 批量调度 + onBatchStart/End 哨兵)               │
   │       │                                                            │
   │       ▼                                                            │
   │   tabs.ts (TabManager: switchTo manual/auto, userActive, override) │
   │       │                                                            │
   │       ▼                                                            │
   │   render-stream-record.ts ⭐ v2.6 三 caller 共享渲染管线 +          │
   │       │                  tool-group 后处理合并（看 timeline 左邻居）│
   │       ▼                                                            │
   │   record-timeline.ts ⭐ v2.6 按 seq binary insert + DOM insertBefore│
   │       │                                                            │
   │       ▼                                                            │
   │   stream.ts (MessageStream: insertNode + 守卫式 snap 贴底;          │
   │             重放旧记录经 live-window.ts(TailWindow)收纳不建卡,      │
   │             上翻补批同步手动补偿——F40a/b,详 §5 与 INVARIANTS §21)   │
   │       │                                                            │
   │       ▼                                                            │
   │   render.ts (marked + KaTeX + hljs + DOMPurify; opts.lazy 参数)    │
   │       │                                                            │
   │       └─► DOM                                                      │
   └──────────────────────────────────────────────────────────────────┘
```

**关键路径**：

- **watcher batch 同步 + seq 分配**（v2.4 重构 + v2.6 加 seq）：`spawn_watcher` 接 `on_batch: BatchHandler` 回调；一次 `process_file` 把读到的所有行收集成 `Vec<JsonlLine>` **同步**调 `on_batch`，**没有 mpsc 中间层、没有 async drain task**。lib.rs 里 closure 在 watcher 线程内 parse + `replay.on_line_batch(handle, payloads)`。v2.6 加 `seqs: HashMap<PathBuf, u64>` 给每行分配 per-file 单调 seq → `JsonlLinePayload.seq` 透传到前端，**前端 RecordTimeline 按 seq 排序，后端 emit 顺序不再影响视觉**。
- **大小分流 emit**（v2.4.2，v2.6 chunked emit 简化，Batch5-F17 async 化）：`event_replay::on_line_batch` 按 batch 大小分流：
  - `payloads.len() < 50`（用户日常敲键 1~N 行）→ 逐条 `emit("jsonl-line")` live 路径
  - `payloads.len() >= 50`（`claude --resume` 灌历史 / 远端 snapshot 攒批 / 大量追加）→ 切块走 `emit("jsonl-batch")`，前端进入 batch 模式（lazy hljs）。v2.6 简化：删 head/older 区分，统一按 `CHUNK_SIZE=600` 末块先发，前端按 seq 自动排到正确位置。**Batch5-F17**：大批块序列 spawn 到 async_runtime（块间 tokio sleep）——spawn 返回≠emit 完成，顺序敏感调用方（ssh_source 攒批 flush，行须先于断连归档）用 `on_line_batch_awaited`（INVARIANTS § 10）；前端 events.ts 另有突发检测兜底（jsonl-line 积压 >50 主动进 batch 模式）
- **启动序**（v2.4 修首次启动乱序；Batch5-F18/F19 骨架+优先级）：前端 DOMContentLoaded 后先 invoke `list_active_sessions` 把本地活跃会话**骨架 tab** 全部建出（远端骨架走 `remote-session-added` 事件），再按 localStorage 记忆选 active（上次所在 tab），然后 `emit("frontend-ready", {prioritySid})`。后端 listener 在 async task 里 spin-wait（10ms poll，10s timeout）等 watcher 同步全量扫置位的 `initial_scan_done` → `replay_and_mark_ready(priority_sid)` **按 session 分组**切块 emit（prioritySid 的块先发、组内末块先发、chunk 全局连续编号保 batch-start 哨兵）→ mark ready → **按活跃集对账补发 `session-ended`**（#19 本地用 session_map / #20 远端用 remote_active；前端把 session-ended 与行同队列同序处理，归档落在全部重放行之后，INVARIANTS § 24）。v2.6 简化：**删了 `replaying` flag + catch-up tail 路径**，chunked emit 期间 watcher 真新行直接走 jsonl-line live emit，前端 timeline 按 seq 自动放到正确位置
- **前端按 seq 排序**（v2.6 B 重构）：`RecordTimeline.insert(seq, element)` 用 binary search 找位置 → `stream.insertNode(element, anchor)` 同步处理 stickToBottom 贴底。**消除了** PayloadSource batch/live / inPrependMode / pendingPrependFragment 等 5 个 flag。tool-group 合并改后处理算法：插入时 `timeline.peekPrev(seq)` 看左邻居，是 tool-group 就 `addToToolGroup`，否则建新 group 入 timeline（详 render-stream-record.ts）
- **active session 自动同步**（v2.4 issue #2）：tabs.ts `onLine` 透传 payload 给 `renderStreamRecord`；sink.onRealUserInput 仅在 `result.kind === "card" && message.type === "user"` 触发（v2.6 删 source 参数后用 message.type 判定）→ TabManager.userActive 检查 `autoFollowUserActive` toggle + 5s `manualOverrideUntil` → `switchTo(sid, "auto")` + 可选 `invoke("bring_monitor_to_front")`
- **cc 集成绑定**：PS 跑 `__ccm_bind` 写 `ps-await/<PID>.json` + 改窗口标题为 marker → `bind.rs` 监听 + EnumWindows → 写 `ps-registry/<PID>.json` + 删 await → PS 检测到删除恢复标题
- **历史浏览（流式）**：v2.2 起，点 Ctrl+H → `list_history_projects`（async + spawn_blocking，不阻塞 IPC）→ 用户展开项目 → 前端创建 `Channel<HistorySessionEntry>` 传给 `stream_history_sessions_in_project` → 后端边解析 jsonl 元数据边 `on_entry.send()` → 前端 onmessage rAF 节流增量插入到 fork 树。点单 session → `Channel<Vec<JsonlLinePayload>>` + `stream_read_session_jsonl` 100 行一 chunk emit → session-viewer 两阶段加载(Batch13-F39):收集全部 payload(不渲染)→ 渲染末尾 150 条首屏(37MB 实测 65.5s→1.1s)→ 上翻增量补批(手动滚动补偿稳视口)+ 深链岛
- **Task 面板（v2.3 issue #11）**：`tasks.rs::spawn_task_watcher` 用 notify-debouncer-mini 监听 `<claude_dir>/tasks/` 递归 → 文件变更（CLI 跑 `TaskCreate` / `TaskUpdate` / `TaskStop`）→ debounce 100ms + 按 sid dedup → 重读整个 `tasks/<sid>/` 目录（跳过 `.lock` / `.highwatermark` / 非数字命名）→ emit `task-update` 携完整 task 列表。前端 `tasks-panel.ts` 按 sid 路由到对应 Tab 的 sticky 折叠卡。Tab 创建时另调 `get_session_tasks` IPC 拿初始快照（async + spawn_blocking）。0 task 时 panel 隐藏

---

### 1.1 ⚠ 上面那张图只画了**本机**那两个源 —— 另外两条链（F19 补）

原图（`Claude Code CLI` + `PowerShell session`）会让新人以为 **monitor 只读本机文件**。
今天的主战场是另外两条：

```
  ┌─ 远端主机 ─────────────────────────────┐
  │  常驻 daemon（remote-daemon-proto）      │
  │   observe/watcher  ──► JSONL 帧 ──┐      │
  │   control/{launch,kill,gate}      │      │   ← monitor 从这条**长连接**发控制命令
  └───────────────────────────────────┼──────┘      （inbound_client：请求/应答 + 背压）
                                      │ stdout（一条 SSH channel）
                                      ▼
        ssh_source.rs::stream_loop ──► 与本机同一套 emitter / event_replay
                                      （帧种类见 doc/IPC-PROTOCOL.md）

  ┌─ POSIX 本机 ──────────────────────────┐
  │  「本地 = 不走 ssh 的远端」——同一套分解，  │  ← 会话容器就是 tmux，
  │  只是没有 SSH 那一跳                     │     平面 ③ 刻意不开 GUI 终端（见 2.1）
  └──────────────────────────────────────┘
```

⚠ 三条链**共用同一个前端管线**（`event_replay` → `jsonl-line` / `jsonl-batch` → 前端按 `seq` 排序）
—— 那是「一份代码、两种承载」在数据流这一侧的样子：**换的是源，不是管线**。

---

## 2. 层边界（不放逐文件模块表 —— 见本节末）

> ⚠ **F19 重写**（2026-08-04）。原来这一节是一张 **122 行的逐文件模块树**，而：
> - 它与 `src-tauri/README.md` 的目录树**各存一份**，**两份都缺 `backend/`**；
> - 本文件自己在开头与结尾**两次**把「模块表」委派给子目录 README —— 它自己就说过不该放；
> - 更要紧的是：**本工作区最大的两件结构性事实在它里面不存在** ——
>   `backend` 这个词全文只出现过 **1 次**（且指的是前端的 `session-backend.ts`），
>   「轮询」**0 次**。真架构住进了 1517 行的 `INVARIANTS.md` ⇒ **分层倒挂**。
>
> ⇒ 本节只写**层边界与它们的理由**；逐文件清单去子目录 README。
> ⚠ 那张树里**嵌着几条真正的架构理由**（U8a 三平面、平面 ③ 搬不走…）——
> 它们**没被删掉**，被提到下面各条里了。

### 2.1 两半：frontend 与 backend

**backend = 读（`observe/`）+ 控制（`control/`）**，**一份代码、两种承载**：
**远端进程** = `remote-daemon-proto/`（独立 crate，**不是 workspace 成员**，见 2.6）·
**本机进程** = `src-tauri/src/backend/`。

两侧都该有 `platform/` `observe/` `control/` `common/` 四层。**远端四层齐全；本机只有一层** ——
下表是**今天真实的落地进度**，且**每一格都由判据现场量**
（`doc_claim_registry::each_registered_status_still_matches_reality`；
⚠ 判据**不存这一列的副本**，它从本文件读这一列、再去代码里量，两边不一致就红）：

| backend 分层（定框 §5） | 远端（daemon）有吗 | monitor 侧今天的状态 |
|---|---|---|
| `control/` | 有 | **已交付** —— 两条改状态的远端 tmux 命令都走它（见 2.3） |
| `observe/` | 有 | **待做** —— **刻意未建**，谁来叫醒见 2.2 |
| `platform/` | 有 | **待做** —— 平台原语还散在 `bind.rs`/`utils.rs`（见 2.4） |
| `common/` | 有 | **待做** —— **刻意不建**：monitor 侧的共用面住 `src-tauri/crates/*`（见 2.6） |

⚠ 这张表量的是「**这一层在 monitor 侧落地了没有**」，**不是**「平台原语已经收敛干净了」——
后者是 C10 的判据（跨 target 编译）的事，今天**不成立**，见 2.4。

**frontend 只剩「在用户桌面上开一个终端窗口」**，窗口里跑什么由 backend 给。

⚠ **那条搬不动的边界**：最后那次 `exec` **必须**在用户自己的终端进程里
（pid 要等于 pidfile 名 · tty 与 Ctrl-C 要落在 agent 上 · `tmux attach` 要占住调用者终端）。
⇒ 「起一个会话」被拆成 **U8a 三平面**：
① **计划面**「跑什么命令」→ daemon `control/resolve` ·
② **远端执行面**「在远端真的建 tmux」→ daemon `control/launch`（**argv 直传、不过 shell**）·
③ **本机开窗面** → **只能是 monitor**（daemon 在远端，开不了你面前的窗）—— **这条永远搬不走**。

⚠ **平面 ③ 在 POSIX 上刻意不开 GUI 终端窗口**：那儿没有「唯一的终端」，挑一个就是平白引入
一个会在别人机器上错的决定；会话容器本来就是 tmux ⇒ 命令直接跑、会话留在 tmux 里等 attach。
由 `no_terminal_emulator_is_ever_spawned_from_this_file` 零命中钉住。

### 2.2 `observe/` vs `control/`：**按用途分，不按读写分**

- **任何改状态的 tmux 命令**（`set-hook` / `set-option` / `new-session` / `kill` / `send-keys`）
  一律归 `control/`；
- **只喂控制决策的只读查询也归 `control/`**（`control/gate.rs` 探 `@ccm_sid` / `session_windows`
  —— 它不产观测帧）；
- **只有产出观测帧的读**才归 `observe/`。

⚠ 反面很具体：按「读/写」分的话，那次 `@ccm_sid` 探测会被判给 `observe/`，
而它唯一的调用方在 `control/` ⇒ **凭空造出一条 `control → observe` 的边**，
而 `layering_guard` 逐字禁止反向依赖（实测：照做时它当场红）。

⚠ **monitor 侧的 `observe/` 今天刻意未建**：那批读面（`config_surface.rs` 1596 行 ·
`search.rs` 1171 · `local_accounts.rs` 707 …共 13 个 reader 文件）正是要**退役**的那批 ——
先搬进来再删掉是纯搬运。**谁来叫醒这个决定**：`local_read_surface_registry` 里那条前提触发器
（`tauri.conf.json` 一出现 `externalBin` 就红）。

### 2.3 控制面今天真的在 backend 了

两条**改状态**的远端 tmux 命令都已切到 daemon：`kill_remote_tmux` → `control/kill.rs` ·
`tmux_send_keys` → `control/launch.rs` 的 `send-into` / `send-keys-raw`。
一次性 SSH 那两条降为**过渡期回落**，且**过门被拒绝一律不回落**
（回落到 shell 路 = 把一次被门拒绝洗成另一条路的成功）。
「能不能回落」的判定**只有一份**（`backend/control/daemon_route.rs`）。

### 2.4 `platform/`：daemon 侧是唯一落点，**monitor 侧还没落地**

`platform/` 应当是**唯一**允许平台原语与平台 cfg 的地方，判据是**跨 target 编译**
（不是 cfg 位置扫描）。daemon 侧成立；**monitor 侧今天不成立** ——
平台原语散在 `bind.rs` / `utils.rs`。⚠ 这条是**如实记下的欠账**，不是「已经做到了」。

### 2.5 零轮询：一律事件驱动，**四张账本覆盖四块**

「不许轮询」不是一句口号，它由**四张登记表**覆盖：

| 账本 | 管哪一块 |
|---|---|
| `no_timer_guard`（daemon 侧） | daemon 里不许有「自己醒过来」的构件（零容忍） |
| `polling_registry` | 前端 TS 与 `shared/ccm` |
| `rust_timer_registry::REGISTERED` | monitor **Rust 级**的 `sleep` / `interval` |
| `rust_timer_registry::SHELL_WAKES` | monitor Rust **拼出来的 shell 循环**（前三张都看不见它） |

⚠ **口径是「每一处都说清事件源在哪、谁退役它」，不是「一处轮询都没有」** ——
实测有两处**今天根本没有内核事件源**（pane 内容变化 · 别人改了账号），
把它们改成等帧只会做出一个永远等超时的东西 ⇒ 如实记未排期。

⚠ **唯一登记在案的例外**：预信任的「等信任框」没有内核事件源 ⇒ `control/` 继续以
**shell 字符串形态**产出它（由目标 shell 执行，因此与「零定时器」共存）。

### 2.6 共享 crate：为什么 daemon **不进** workspace

`src-tauri/crates/*`（6 个）是 monitor 与 daemon 的共同实现落点（判定只许有一个家）。
而 **daemon crate 刻意不是 workspace 成员** —— 它要能在目标机上**原生构建**。

⚠ **代价是实的、要写下来**：在 `src-tauri` 里跑 `cargo fmt --all` / `cargo test`
**覆不到 daemon**（曾因此漏过一次 fmt 红）⇒ 门禁读数必须**八处分别跑**
（monitor · daemon · 6 个共享 crate）。

### 2.7 逐文件清单去哪了

- Rust 侧：`src-tauri/README.md`
- 前端：`src/README.md`
- `backend/` 内部：`src-tauri/src/backend/mod.rs` 的 `BACKEND_FILES` 登记表
  （**加文件不写理由就红** —— 那是这个目录不再变成平铺堆的机制）

---

## 3. Tauri State 注册矩阵 → **只有一个家：`doc/STATE-MATRIX.md`**

〔F19〕这里原本有一张 7 行的 State 表，而 **`doc/STATE-MATRIX.md` 里有严格更全的同一张表**
（多出「注册位置 / 创建位置 / Arc 所有权」三列 + 逐命令 consumer 清单），
且原文自己就写着「详细 consumer 矩阵 → STATE-MATRIX.md」——
**它自己承认家在那边，却又存了一份摘要。** 两份副本必漂 ⇒ 摘要删除，这里只留指针。

⇒ **改 State 前后都读 [STATE-MATRIX.md](STATE-MATRIX.md)**，它是撤回/修改任何 IPC 命令的强制 checklist。

**为什么这件事值得一整份文档**（这条是架构性的，所以留在这里）：
漏一次 `app.manage()` **不会被 `cargo check` 抓住** —— 命令签名照样编译过，
**运行时第一次调用才 panic**。Tauri 的 State 注入是运行期按类型查表的，
编译器在这条路上帮不了你 ⇒ 只能靠一份人维护的清单兜着。

---

## 4. 跨进程文件 IPC 简表

monitor 与外部进程的所有通信都在 `~/.claude/claudecode-frontend/` 下：

| 路径 | 写入方 | 读取方 | 用途 | 生命周期 |
|---|---|---|---|---|
| `config.json` | monitor 设置面板 | monitor 启动 | 主题 / 字体 / claudeDir override / diagnostics | 持久 |
| `ps-await/<PID>.json` | PowerShell (`__ccm_bind`) | monitor (`bind::BindRegistry`) | PS 通知 monitor "去找标题**含** marker 的窗口" | 短暂 (3s 超时) |
| `ps-registry/<PID>.json` | monitor | PowerShell (查 + 比较 procStart) | monitor 通知 PS "绑定成功，HWND = X" | 与 PS 进程同寿 |
| `sid-hwnd-cache.json` | monitor | monitor 启动恢复 | sid → hwnd 持久缓存，新 session 出现时查这里复用绑定 | 持久 |
| `auto-launch.json` | monitor 设置面板 + 启动时回写 | PowerShell (`__ccm_bind` 头部) | "用 cc 启动 claude 时自动开 monitor" 开关 + monitor exe 路径 | 持久 |
| `history-metadata.json` | monitor 历史浏览器 | monitor 历史浏览器 | star / 重命名 / 隐藏 | 持久 |
| `logs/monitor.YYYY-MM-DD.log` (v2.0.0+) | monitor (tracing-appender) | 用户（设置面板 [打开 log] / 编辑器） | GUI app 诊断日志，按天滚动保留 3 天 | 持久（自动清理老文件） |

**只读外部数据源**（不属于 monitor 写入域，但 monitor 读取并展示）：

| 路径 | 写入方 | 读取方 | 用途 |
|---|---|---|---|
| `<claude_dir>/projects/<encoded-cwd>/<sid>.jsonl` | Claude Code CLI | monitor `watcher.rs` / `history.rs` | session 消息流，monitor 实时增量 + 历史浏览 |
| `<claude_dir>/sessions/<PID>.json` | Claude Code CLI | monitor `session_map.rs` | 活跃 session 探活（PID + procStart 双校验；procStart 缺失时自动降级仅 STILL_ACTIVE，详 INVARIANTS § 18） |
| `<claude_dir>/tasks/<sid>/<id>.json` (v2.3) | Claude Code CLI (`TaskCreate`/`TaskUpdate`/`TaskStop` 工具) | monitor `tasks.rs` | Tab task 面板数据源；附 `.lock` / `.highwatermark` 控制文件需忽略 |

每个文件的字段定义、编码约束（UTF-8 无 BOM）、写入方原子性语义、握手时序图 → [IPC-PROTOCOL.md](IPC-PROTOCOL.md)。

---

## 5. 关键设计选择 + 理由

每条都是踩过坑总结出来的"为什么不能用别的方案"。

### 零侵入 = 不写 Claude Code 数据源
watcher / session_map 只读 `~/.claude/projects/` 和 `~/.claude/sessions/`。写入均为用户**显式**触发：①历史浏览器 `delete_history_session`（Batch4-F15 起 exists → 双边 canonicalize → canonical 前缀 + `.jsonl` 扩展名四段守卫，`..`/symlink 穿越拒绝）；②F62 `create_branch_session`（从某轮建分支——**只新增** `<new-sid>.jsonl`，`validate_branch_source` 同源守卫 + `create_new` 原子写**绝不覆盖**，原会话零改动，§1 正交非侵入）；③PowerShell profile [安装]（只动 BEGIN/END **块内**内容，块外用户其他代码完全不动）；④**G6 远端分叉**（`remote_branch::create_remote_branch_session` → ssh → daemon 的 `fork_write.rs`）——与②是**同一件事的远端形态**（用户显式点 `⑂` → 只新增一份 `<new-sid>.jsonl`、原会话零改动），区别只在**动手的是 daemon 而不是 monitor**。daemon 的写面被 `readonly_guard` 两层护栏钉死在那**一个**模块上（且必须 `O_EXCL`、禁删/改名/截断/追加/覆盖），细则见 `doc/INVARIANTS.md` §1 的 G6 段与 §41.6。<br>（2026-08-01 Phase G 订正：本枚举原来只有三条 —— 而 `INVARIANTS.md` 那边已经写上了第四条，两份文档口径不一致，而本文是新人先读的那份。）

**为什么**：cc-monitor 是个监控渲染器，写 jsonl 会破坏用户对"数据源 = 我自己的命令痕迹"的认知；profile 写入则是必要的可选副作用（用户显式 opt-in 装 `__ccm_bind`），仍然走完整的 backup + ACL 保留路径。

### event_replay 顺序保证 = 前端按 seq 排序（v2.6 起；本节曾描述已废弃的"持锁完整 emit"）
v2.6 B 重构前顺序靠"持锁完整 emit"（record 排队等锁）；**现行设计**：`replay_and_mark_ready` 持锁只做 snapshot + 置 ready，emit 全在锁外——顺序保证整体转移给 per-file 单调 seq + 前端 RecordTimeline 二分插入（ADR-021/022）。emit 期间并发到达的 live 行先于 snapshot 旧行到达也无碍：前端按 seq 排到正确位置。

**为什么能放弃持锁 emit**：旧方案的代价是 replay 期间 watcher 阻塞数十毫秒到秒级；seq 排序把"后端保序"变"前端排序"后，emit 顺序成为纯性能自由度（Batch5-F19 的 priority 分组正是利用这一自由度）。跨通道顺序（行 vs session-ended）不由 seq 覆盖，由队列同序（INVARIANTS § 20）与 `on_line_batch_awaited`（INVARIANTS § 10）分别兜住。

### JSONL_BATCH 单次 emit 替代 N 次 JSONL_LINE
replay 时一次性发整个 Vec<JsonlLinePayload>，前端 push 进同一 queue 走原批量调度。

**为什么**：Tauri IPC 每次 emit 都有序列化 + 派发 overhead。N=3000 时累计 ~400ms 主线程阻塞，启动可见显著卡顿。BATCH 单次序列化降到 ~50ms。

### 视口外渲染跳过 = content-visibility + 精确估高（#35 F38，虚拟化第一层）
所有顶层卡片(`.stream-content > *` 与折叠段内 `.branch-fold-body-inner > *`)带 `content-visibility: auto`——视口外与隐藏 tab 的卡片跳过布局/绘制。`contain-intrinsic-size` 初值由 `src/height-estimate.ts` 在建卡时按块类型精确估算(prose 走 @chenglou/pretext canvas 测宽、代码块行数×行高、折叠 details 常数;**估值只是初值**,`auto` 关键字让浏览器渲染过后记住真实尺寸)。约束:估高路径**绝不许抛**(pretext 失败双降级+三振永久禁用);780px 定宽列是估值成立前提;卡片被 reparent 进 fold 后由 inner 规则续保 c-v。这层吃掉了 paint/layout 成本,是后两层(F39/F40 不建 DOM)的地基。

### 启动重放贴底消抖 = 守卫式 snap + overflow-anchor + 尾部优先收纳（F40a）
重放"末块先发"。Batch13-F40a 起 `TabManager.onLine` 按 seq 门控（详 INVARIANTS § 21）：

- **守卫式 `snap()`**：只在落后底部 >1px 时才写 scrollTop，不每帧重钉。
- **窗口内中部插入交给原生 `overflow-anchor`**：不手动补偿 scrollTop（叠加会 double-shift）。
- **尾部优先收纳**：active tab 首条 content 记录钉 `floor`，尾块（`seq ≥ floor`）直渲进步式首屏；更老的块与后台 virgin tab 的全部记录**只进 `TailWindow` 账本不建卡**（meta/branch 经 `routeMetaAndBranch` 照喂）。后台 tab 批后 `requestIdleCallback` 逐个物化尾 150 条；`switchTo` 命中 virgin 同步物化。物化 = `unwrapAll` → `renderContentRecord`× → `reconcilePendingToolResults` → `rebuildNow`（无条件重折）。

**为什么**：旧内容逐条插到贴底视口上方会让浏览器逐帧重排 + 重做 scroll anchoring，HiDPI/高刷屏分数像素下 ±0.5px 高频抖动（deferMode 时代实测 66→1 帧）；F40a 让**启动重放**的上方插入为 0，且 9.4k 条重放只建 ~尾块+150×tabs 张卡（建卡是重放期最大成本——markdown/DOMPurify/pretext 全免）。历史方案 deferMode/`flushDeferred`/`attachBatch` 已退役。大增量批（>600 行切块落已渲染 tab）的老块由 F40b `midBatchBuffer` 缓冲、批末一次挂载。

F40b 上翻补批：active tab 滚到顶部 800px 内自动从 `TailWindow` 弹 200 条/批渲染（`unwrapAll`→`batchInsert`→reconcile(空组壳连根摘并出账)→`rebuildNow`→同步手动补偿 scrollTop），顶端 `.stream-more-above` 哨兵显示剩余条数；选区进行中暂缓；不可滚+账本有余的 tab 在 switchTo 时踢一次 fill 自链（INVARIANTS § 21.3）。

### 独立只读窗口复用主渲染管线 + 定向 replay（issue #10）
`open_session_in_new_window` 建 `viewer-<sid>` WebviewWindow 加载 `index.html?viewer=<sid>`；前端 `main.ts` 检测参数走精简 bootstrap（`bootstrapViewer`）—— **复用 TabManager**（过滤到该 sid、`body.viewer-mode` 隐藏 tab/设置/历史 chrome），自动继承分支折叠 / 启动滚动消抖 / tool-group 合并。顶部一条 slim 栏：项目名标题 + ↗调出终端 + 📂打开 cwd（复用 TabManager 的 active-tab 动作）。

**为什么不另写 viewer 渲染器**：再写一套渲染会与主管线漂移（SessionViewer 漏 pendingToolResults 是历史教训）。复用 TabManager 零功能差。

**历史 + 实时一致性**：独立窗口订阅 `jsonl-line`（按 sid 过滤）拿实时增量；历史经 `replay_session_to_window` 从 event_replay buffer **定向 emit 给本窗口**——两者都是 watcher 的 **per-file seq 空间**，混进同一 RecordTimeline 顺序天然正确，重叠由前端 `seen` set 去重。**不发 frontend-ready**（那会触发对所有窗口的全量 replay）。仅活跃 session 在 buffer；archived 走前端一次性文件读。capability 必须含 `viewer-*`（见 capabilities/default.json）。

**踩过的坑（INVARIANT § 22，含 F82a 新增两条）**：见 §22 全六条。摘要：① 开窗命令必须 `async`（否则主线程自死锁）；② 定向事件 target-kind 对齐（viewer；settings 用广播↔模块级 listen 的 Any↔Any 同步）；③ 异步 listen 先注册再 emit；④ 精简模式别塌 grid 行（viewer 只定义剩余 item 行数；settings 直接隐藏 grid 容器 `#app` 整块）；⑤ **关窗要 `core:window:allow-close` 能力**（`core:window:default` 不含，getCurrentWindow().close() 否则被 ACL 静默拒）；⑥ **复用 dispatcher 的独立窗口必须自调 `dispatcher.start()`+`applyOverrides`**（否则窗内快捷键录制收不到键、Esc 关不了嵌套 overlay；别手搓 window Esc 会双关窗）。

### 独立设置窗口（F82a #56+#47）
`open_settings_window`（async，单例 `settings` 窗）建 `index.html?settings=1`；`main.ts` 检测参数走 `bootstrapSettings`——`body.settings-window-mode` 隐藏 `#app` 整块、`SettingsPanel({windowMode:true})` 铺满整窗，并自调 `dispatcher.applyOverrides+start`（窗内快捷键编辑器/overlay Esc 需要）。设置项经既有 config 命令读写（窗口无关，无 replay/事件流）。**跨窗同步**：设置窗保存主题 / 行为 toggle / resetAll、以及键位编辑器 persist 后 `emit('settings-applied')`（广播）；主窗口 `listen` 后重读并 `loadTheme`+`applyBehavior`+`applyOverrides`（跨 OS 窗口回调够不到）。cancel 不 emit → 天然 cancel-safe。触发器/`app.open-settings` 快捷键改 `invoke("open_settings_window")`。capability 的 `windows` 含 `settings` 且 `permissions` 含 `core:window:allow-close`。**窗体渲染本环境无 GUI 不可自测 → 真机验证累积。**

### 账号子系统：隔离又同步（A2–A6 / #68/#69）
**模型**：一个「账号」= 一个 `CLAUDE_CONFIG_DIR`（各自 `.credentials.json`，两号可同时跑、不互踢），而 skills/memory/history/settings/plugins 经 symlink 共享到同一库——**凭据隔离、其余同步**。隔离/同步管线是远端脚本 `cc-acct-iso`（app 内向导 `settings/acct-deploy.ts` 分步驱动）。

**只读边界**：cc-monitor 侧对账号只**读**——后端 `accounts.rs` 三命令（`list_remote_accounts` / `list_remote_session_accounts` / `check_account_trust`，全 `async(origin: String)`、**无 State**）经 daemon 纯只读查（名/邮箱/是否登录 / 某会话属哪个账号 / 目录是否可信）。动凭据（登录/同步/`--apply`）一律走**真实终端窗口**，不由 monitor 直接改。

**前端族**（`src/account-*.ts` + `settings/acct-deploy.ts`）：`account-chip.ts` 徽章 + 切号菜单（mismatch/align 状态）；`account-commands.ts`(A4) 「按会话选账号起/Resume」的 `withAccount`（账号解析 + `lastAccount` 记账）；`account-restart.ts`(A5) 「换号对齐当前会话」的**破坏性**重启编排。

**A4 `withAccount` 与 A5 restart 为何分离**（架构审计裁定，防重新纠结）：语义天然不兼容——① 不可选账号时 withAccount **降级默认起**、restart **中止**（破坏性重启绝不退化用默认号）；② withAccount run 后**无条件**记 lastAccount、restart **仅 kill+resume 全成后**才记。硬合需给 withAccount 加三个开关、复杂度净增。二者已共用 `accounts.ts` 同一批原语（`fetchAccounts`/`accountConfigDir`/`recordLastAccount`），无逻辑漂移。**失败语义**严格照 DESIGN §5.2：换号重启先请求优雅退出（`Escape` 打断当前轮 → `/exit` → 有界等待 → 兜底 kill）；compact 失败/超时**不阻断**、kill 失败**必须中止**（绝不续 resume，否则新旧两进程抢同一会话）。

### session 探活双重校验（PID + procStart，procStart 可缺）
`OpenProcess(QUERY_LIMITED) + GetExitCodeProcess == STILL_ACTIVE` + 当 sessions/<PID>.json 含 `procStart` 字段时再加 `GetProcessTimes` creation FILETIME 100ms 容差比对。

**为什么不**只查 PID：Windows PID 短期复用非常常见，仅靠 STILL_ACTIVE 会把僵尸条目误判为活跃（旧 PID 被一个无关进程占用）。procStart 二次校验**有的话**就必须做。

**为什么 procStart 可缺**：v2.4.2 实测 Claude Code 2.1.150 在某些启动路径下（/resume 或类似）写 `sessions/<PID>.json` 漏 procStart。之前 schema 必填导致 serde 整条解析失败 → 整个 session 被静默忽略 → monitor 漏 Tab。改 `Option<String>` 后缺失就跳过 procStart 校验仅 STILL_ACTIVE。INVARIANTS § 18 完整论证。

### 本机判活：Windows + Linux 各用平台原生口径（U7d，2026-08-02）

上面那套双重校验（PID + `procStart`）**两个平台都是满精度的**，因为
**`procStart` 是平台原生格式**，各自与本平台的查询口径同源：

| 平台 | `procStart` 写的是 | monitor 拿什么比 |
|---|---|---|
| Windows | .NET `DateTime.ToFileTime()` | `GetProcessTimes` 的 FILETIME |
| Linux | `/proc/<pid>/stat` **第 22 字段**（starttime） | 同一字段，**逐字符相等** |

Linux 那行是实测的：本机 6 个真实会话 `procStart` 与 `/proc` 第 22 字段 **6/6 完全相等**，
量级（~10^6）也一眼不是 .NET Ticks（~6.4e17）。⇒ **不需要任何启发式降级。**

⚠ **解析 `/proc/<pid>/stat` 不能用朴素 `split_whitespace()`**：第 2 字段 `comm` 允许含空格与括号。
实测本机 400 个进程里就有一个踩中 —— **`comm = "tmux: server"`**，朴素切法读到 `0`，正确值 `1042`。
而 tmux server 正是本仓的核心依赖。做法：找**最后一个** `)`，其后是第 3 字段起，starttime 是其后第 19 项（0 基）。

#### ⚠ macOS 仍然不工作

`#[cfg(all(unix, not(target_os = "linux")))]` 分支仍恒返回「不活跃」⇒ **macOS 上本机会话不会被监听**。
那是**如实的未实现**：macOS 没有 `/proc`，要走 `sysctl KERN_PROC` 的 FFI，而本仓没有 macOS CI、
也无法实测 —— 按本仓纪律不写没验过的实现。

为什么返回 `false` 而不是 `unimplemented!()`（daemon 侧那样）：那边是 CLI，panic 是「没人能忽略的信号」；
这边是 GUI 常驻进程，panic 会直接崩窗口。`false` 在这里是 **fail-safe**（少显示，而不是显示永不消失的
僵尸会话），且这条限制写在这里与 README —— **不是静默的谎**。

### HWND 拉前三重校验
`IsWindow(hwnd)` + 当前 `owner_pid == 绑定时 owner_pid` + 当前 owner 的 `procStart == 绑定时 owner_proc_start`。

**为什么**：HWND 复用比 PID 复用还高频（Windows 重用窗口句柄）；如果不校验 owner 的话，会把不相关的窗口拉前。任一失败拒拉前 + 给 toast 原因。

### profile 写入用 ReplaceFileW 不用 MoveFileExW
`ReplaceFileW(dst, tmp, NULL, REPLACEFILE_WRITE_THROUGH, ...)` 原子替换 dst 的**内容**，**保留 dst 的 ACL / ADS / 创建时间**。

**为什么不**用 MoveFileExW：MoveFileExW 用 src（即 tmp 文件）的 ACL 覆盖 dst 的 ACL。如果用户把 Documents 重定向到非默认盘（`E:\<user>\Documents`），那一层目录 ACL 通常只给 Administrators + Everyone 部分权限，没给当前用户 explicit Full Control。原 profile 上的 explicit ACE 被 tmp 的"父目录继承 ACL"覆盖 → 用户自己读不了自己的 profile。ReplaceFileW 专门设计来保留 dst metadata。

### profile 写入必先 backup + 写后校验
写之前 `std::fs::copy(path, <path>.ccm-backup-<ms>)` 备份；写之后 `read_to_string` + 比对长度，不匹配从 backup 回滚。

**为什么**：OneDrive online-only placeholder / 杀软介入等罕见场景下，`read_to_string` 可能返回 `Ok("")` 即"磁盘有内容但读到空"，纯写就是把用户内容冲掉。backup + 校验是双保险。

### marker 握手 + EnumWindows 找窗口（cc 集成）
PS **先**改 `$Host.UI.RawUI.WindowTitle = marker`、**后**写 `ps-await/<PID>.json` → monitor `EnumWindows` 找 `GetWindowTextW.contains(marker)` 的窗口（找不到重试 ≤600ms）。
**顺序不可换**：反过来 monitor 会在文件落地瞬间就去找一个还没设上的标题，扫得越快越容易失败（v2.21 实测「每个新 shell 首次 `cc` 固定烧满超时」）。细节见 [IPC-PROTOCOL.md § 跨进程握手时序图](IPC-PROTOCOL.md)拿 HWND。

**为什么不**直接用 `EnumWindows + GetWindowThreadProcessId`：PowerShell 进程**不直接拥有终端窗口**（Windows Terminal 是单独进程；cmd 走 conhost；VSCode 走 integrated terminal）。window owner 不等于 PS owner。用 PS 改自己窗口标题为 unique marker + 反查 title 是唯一可靠的跨进程握手。

### UTF-8 BOM 双向防御
PS 端模板 `cc.ps1.tpl` 用 `[System.IO.File]::WriteAllText(... UTF8Encoding($false))` 显式写无 BOM；Rust 端 `bind::process_await_file` 用 `raw.trim_start_matches('\u{feff}')` 剥任何 BOM 再 `serde_json::from_str`。

**为什么**：PS 5.1 `Out-File -Encoding utf8` 默认**写 UTF-8 BOM**（前 3 字节 `EF BB BF`），`serde_json` 不剥 BOM 直接解析失败。源头修 + 接收端兜底双保险，避免用户用旧模板还能 work。

### CSS portal tooltip 真挂 document.body
`?` 图标 tooltip 不挂自己子节点，而是 `document.body.appendChild(tip)` + `position: fixed` + JS 算 viewport 坐标。

**为什么**：父 `.settings-panel` 有 `transform: translateX(0)`（slide-in 动画）。CSS spec 规定：祖先有 `transform` 时，`position: fixed` 后代的 containing block 从 viewport **重置到那个祖先** → `left/top` 不再是 viewport 坐标 → tooltip 实际跑出屏幕。挂 body 脱离 panel 子树即可恢复真 fixed 行为。

### logging 子系统：tracing init 在 Builder 之前 + ErrorEmitterLayer + reload Handle（v2.0.0）
`logging::init(monitor_data_dir)` 必须在 `tauri::Builder::default()` **之前**调用（tracing 全局 dispatcher 一旦 init 不能再换）。内部组装 `registry().with(reload<EnvFilter>).with(stdout).with(file).with(ErrorEmitter).init()`。

- **file layer 用 `tracing-appender::rolling::daily` + `non_blocking` writer**：按天滚动 + 不阻塞业务线程。WorkerGuard 必须挂在 LoggingState 上（drop 时 flush）
- **reload::Layer<EnvFilter>**：`set_diagnostics_config` 能改日志级别**不重启就生效**
- **ErrorEmitterLayer**：自定义 Layer 拦 `Level::ERROR` → 通过注入的 emit closure 发 `monitor-error` 事件 → 前端弹红色 toast。limited 60s/20 条避免风暴
- **AppHandle 通过 closure 注入**：tracing init 时 AppHandle 还没建（在 setup 里才有）→ ErrorEmitter 内部用 `RwLock<Option<closure>>`，setup 里调 `install_error_emitter(handle)` 把 emit closure 写进去
- **失败兜底**：log 目录创建失败 / appender 构造失败 → 退化到 stdout-only，monitor 仍能启动（INVARIANT § 15）

**为什么**：v1.7.0-1.7.7 的 BOM 真凶就是因为 `windows_subsystem = "windows"` 无 stderr，`tracing::warn!("bind: parse ... failed")` 没人看见，cc 集成"装上没用"7 个版本无人察觉。issue #4 就是补这个结构性短板。

### Win32 sync 调用走 spawn_blocking
`bring_terminal_to_front` / `cc_integration_*` 都走 `tokio::task::spawn_blocking`。

**为什么**：Win32 同步调用（`EnumWindows` / `SetForegroundWindow` / `ShellExecuteW` 等）可能阻塞数十 ms 到秒级；放到 Tauri 主 runtime 会卡死 IPC 派发。spawn_blocking 隔离到 blocking thread pool，前端再加 5s timeout 兜底。

### bring_monitor_to_front 三层 hack（v2.4 issue #2）
用户在终端敲键 → monitor 不是前台进程 → `SetForegroundWindow` 直接调被 Win10/11 拒绝（OS 防恶意软件偷焦点）。修复方案是叠加三层：

1. **`keybd_event(VK_MENU)` 模拟 Alt 按键**：OS 检测后视当前进程为"刚有用户输入" → 临时获得前台资格（PowerToys / TranslucentTB 同款 trick）
2. **`AttachThreadInput`**：附加到当前前台线程的输入队列，借用其拉前权限
3. **`SetWindowPos(TOPMOST → NOTOPMOST)`**：强制 Z 序拉顶，即使 `SetForegroundWindow` 失败也至少视觉浮顶

单层（v2.4.0 只 set_focus / v2.4.1 加 AttachThreadInput）实测在 Win10 1903+ 都不够，三层叠加才稳。详 `lib.rs::bring_monitor_to_front` + CHANGELOG v2.4.2。

---

## 6. 关于"不在主线 / 已废弃"特性

设计中**主动放弃**的方向，写在这里给后续重构者参考避免重蹈：

### OS 焦点同步（已删 / v2.4 用 watcher 反推替代）
原 `SetWinEventHook` 监听 `EVENT_SYSTEM_FOREGROUND` 然后切对应 Tab 的设计。

**为什么放弃**：Windows Terminal 单进程多窗口/多 tab 架构，`GetForegroundWindow` 只能拿到 WT 主进程的 HWND，**无法区分同一 WT 窗口内哪个 tab active**。SidHwndCache 里 N 个 tab → 同一 HWND 的映射也反查不出。已彻底删除 `lookup_by_foreground_pid` 和 `FOCUS_SWITCH` IPC。

**v2.4 issue #2 用 watcher 反推 `type=user` 替代**：用户在 claude 里敲回车 → claude 写一行 type=user 到 jsonl → watcher 识别 → 切对应 Tab。零侵入、信号准（详 doc/INVARIANTS.md § 20）。OS API 路径仍废弃；公开 API（[microsoft/terminal#19818](https://github.com/microsoft/terminal/issues/19818)）2026 年 5 月仍在 Backlog。

### subagent 实时流（已隔离）
不走主 watcher，由前端 `invoke("load_subagent")` 在用户展开 Task 折叠卡时按需加载。

**为什么**：subagent jsonl 数量大但展开率低，主 watcher 全 emit 会膨胀 event_replay buffer 数倍但绝大多数没人看。隔离到 on-demand IPC 是 ergonomic + memory 的双赢。

### 4-tier 启发式拉终端（已撤回）
v1.6.x 试过的"从 claude PID 走 parent chain + WT 进程 + 终端类进程 + ai-title 匹配"4 层 fallback。

**为什么放弃**：explorer 启 PowerShell + WT DefTerm 接管 console 的常见架构下，claude 祖先链与 WT 窗口完全脱节（claude 的 parent 是 PS，PS 的 parent 是 explorer；WT 是另一个独立进程，跟 claude/PS 没有 parent 关系）。4 层启发式在主流环境下都不可靠。改走 cc 命令注入式绑定（v1.7+），让 PS 主动告诉 monitor "我的 HWND 是 X"。

---

## 7. 入门读图

- 想理解整体数据流：本文 § 1（**三条链**：本机 / 远端 daemon / POSIX）+ § 5
- 想知道 frontend / backend 的边界在哪、哪一半还没搬完：本文 § 2
  （§2.1 那张表是**今天真实的落地进度**，且由判据现场量 —— 它不会停在某个旧的「今天」）
- 想加新 jsonl 类型：见 [CONTRIBUTING.md](CONTRIBUTING.md) § 添加 jsonl 类型
- 想改/加跨进程协议文件：见 [IPC-PROTOCOL.md](IPC-PROTOCOL.md)
- 想加新 IPC 命令：见 [CONTRIBUTING.md](CONTRIBUTING.md) § 添加 IPC + [STATE-MATRIX.md](STATE-MATRIX.md)
- 想改某个具体模块：找对应子目录 README（`src/` 或 `src-tauri/`）的模块表

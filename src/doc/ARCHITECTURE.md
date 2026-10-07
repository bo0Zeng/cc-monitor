# 架构总览

cc-monitor 是 Claude Code 会话的**观察者和启动器**：`claude` 跑在用户自己的终端里，cc-monitor 读它写下的会话记录来显示，替用户起会话、管机器，不接管 `claude` 本身。

本篇回答：由哪几个进程组成、会话内容从哪儿来、经过谁、停在哪儿；一件事该放进哪一层，以及为什么这么分。新贡献者先读这一篇。

相关文档：

- 全局不变量 → [INVARIANTS.md](INVARIANTS.md)
- 帧协议、一次性命令与跨进程文件 → [IPC-PROTOCOL.md](IPC-PROTOCOL.md)
- Tauri State 注册矩阵 → [STATE-MATRIX.md](STATE-MATRIX.md)
- 加东西 / 撤东西的做法 → [CONTRIBUTING.md](CONTRIBUTING.md)
- 逐文件清单 → 前端 [`src/README.md`](../README.md) · 壳 [`src/frontend/shell/README.md`](../frontend/shell/README.md) · 后端 [`src/backend/README.md`](../backend/README.md)

---

## 1. 进程与数据流

### 1.1 进程

| 可执行文件 | 是什么 | 谁起它 |
|---|---|---|
| `monitor` | 界面进程：webview 窗口 ＋ 通信层面 A 的客户端。零 SSH，不写用户文件 | 用户 |
| `ccm`（开发树里叫 `cc-monitor-backend`） | 唯一的后端。每台机器上 `~/.cc-monitor/bin/ccm` 就是那台的后端，也是用户敲的 `claude` 的壳（argv 打头是 `--` 加后端词才进后端，其余交给 `claude`） | 本机：monitor 连上来时起；远端：monitor 接那台时经一次 exec `--resident-ensure` 起（已在跑就用那一个） |
| `cc-monitor-filewin` | 文件管理器窗口，独立前端，一个窗口一个进程 | monitor（交给它一条回环通道和一把钥匙） |

`claude` 不是我们起的进程：后端渲好一条命令串，monitor 交给用户自己的终端去 exec。

```
本机                                                          远端（每台）
┌ monitor（界面，零 SSH）─────┐  回环 ＋ 钥匙  ┌ 文件窗口 ×N ┐
│ webview 窗口 · 通道客户端   │◀─────────────▶│ call /      │
└──────┬─────────────────────┘               │ subscribe   │
       │ 管道                                 └─────────────┘
┌──────▼──────────────────────────┐  SSH（隧道 · exec · SFTP）  ┌ 常驻后端（与本机同形）┐
│ 本机常驻后端                      │ ──────────────────────────▶│ ＋ 中转（进程内）      │
│ 持有到各远端的全部 SSH（含 SFTP） │                            └───────────────────────┘
│ 中转 ＋ 上游选择                  │ ◀── agent 经中转口连进来
└─────────────────────────────────┘
```

- 本机固定两个进程：monitor ＋ 本机常驻后端。Linux 上后端脱离起、只听回环口，monitor 用钥匙接上；monitor 关了，后端按那台机器的「退出行为」留下或退出，下次 monitor 起来接回。Windows 上后端是 monitor 监护的子进程，随 monitor 退出。
- 远端每台一个常驻后端，与本机同形。monitor 经本机后端在那条 SSH 连接上开的隧道接它，不另开公网口；那台后端比 monitor 旧就换一份。远端只支持 Linux（x86_64 / aarch64），别的系统连上时显式拒绝。
- 一个常驻后端可以同时接多个客户，连接数归零那一刻按「退出行为」当场决定退不退。

### 1.2 会话内容流：本机与远端同一条帧路

```
   <claude_dir>/projects/<编码后的 cwd>/<sid>.jsonl        （claude 写）
                 │  那台后端的 observe/watcher 盯文件，逐行出 line 帧（JSONL）
                 ▼
   本机：管道  ·  远端：本机后端持有的那条 SSH 长连接（隧道）
                 ▼
   monitor stream_source/batch.rs 的 LineIntake：攒批 · 静默窗 · 续点
   （本机那条经 local_lines.rs 进同一个收口）
                 ▼
   event_replay.rs：重放缓冲（每个会话只留尾巴）· 大小分流 · 就绪点
                 ▼  通道订阅 session-lines，按 credit 交格（窗口作用域事件 chan-items）
   前端 events.ts → tabs.ts（TabManager）→ render-stream-record.ts
                → record-timeline.ts（按 seq 插入）→ stream.ts → render.ts → DOM
```

- **同一个收口**：远端那台后端的 `line` 帧沿 SSH 长连接回来，交给 `stream_source/batch.rs` 的 `LineIntake`；本机后端的帧走管道，进的是同一个收口。换的是源，不是管线。
- **seq 是后端给的行号**，前端 `RecordTimeline` 按 seq 二分插入，后端交格的先后不影响画面。工具组合并是插入后的后处理：看左邻居是不是工具组。
- **起停与状态也在这条流里**：会话账本整本在后端（`observe/session_ledger`），帧 `session_added` · `session_state` · `sessions_replayed` 随 `session-lines` 一起来，起停帧不吃 credit、不许丢。monitor 只留一份「那台说过的成品」缓存（`session_book.rs`），它自己唯一知道的事实是「到那台的连接断了」，那时那台的成品作废、界面说「说不清」。
- **背压**：前端给 credit；实时行没有 credit 就丢，并在原位报 gap，前端按行号向那台后端补（帧命令 `history-lines`）。
- **大小分流**（`event_replay.rs::on_line_batch_awaited`）：小批逐行一格；大批（`claude --resume` 灌历史、重放）按 `CHUNK_SIZE = 600` 切块、末块先发，每块带 batch 边界，前端进 batch 模式（代码高亮延后）。
- **启动序**：主窗口先经通道订好每台机器的会话流，再发 `frontend-ready`（带优先会话）——那就是这些订阅的就绪点。后端在 `event_replay.rs::ready_point` 里按会话分组切块、优先会话先交，并按活跃集补发 `session-ended`，归档落在全部重放行之后（INVARIANTS §24）。本机活会话的骨架 tab 也由这条流的起停帧给出（旧的 Tauri 命令 `list_active_sessions`〔散文墓碑〕已删）。
- **冷读也问那台后端**：历史清单、整页正文、按偏移读、按行号读、子 agent、全文搜索都是帧命令（`history-*`）。记录解释（一行 jsonl → 渲染模型）只在后端 `agents/claudecode/`，界面按形状收成品；多台的搜索结果由本机后端合并排序。
- **Task 面板**：那台后端盯 `<agent 家>/tasks/`，一批事件按 sid 去重发 `tasks_changed`，界面订 `session-tasks`，收到就重问 `tasks-list`。

### 1.3 每条线的源头

按「源头是谁」切，是不会切错的那种切法：

| 线 | 源头 |
|---|---|
| 会话内容 · 历史 | `<claude_dir>/projects/**/*.jsonl`（冷读与实时读同一份） |
| 判活 | `<claude_dir>/sessions/<PID>.json` ＋ tmux 会话上的 `@ccm_sid` |
| 流量 | HTTP 请求本身（中转看得见的那一份） |
| 起会话 · 控制 | 用户意图 |
| 窗口绑定 · 拉前 | 本机：PowerShell 接入块的 marker 握手；远端：此刻连着那个会话的终端（那台报出它的 `SSH_CONNECTION`，本机后端在连接表里认出这台电脑上开着那条连接的进程链）。两条都只在 Windows 上 |
| 账号 · 上游 | `~/.cc-monitor/accounts/accounts.json` ＋ 每台机器一份 API 号凭据表 |
| 配置 | 各自的配置文件（monitor 的 `config.json` · 每台后端的 `backend.json`） |

### 1.4 界面对后端只有两个动作

```
call(origin, op, payload)  → 一次性请求
subscribe(origin, kind)    → 流
```

- `origin` 是唯一寻址键，本机也有值（`<local>`）。判定只住 `src/frontend/ui/ipc/origin.ts`，别处一律 `isLocalOrigin` / `isRemoteOrigin`。
- webview 那一侧的通道客户端是 `chan.ts`（通信层面 A 成员）；握手时那台交出它能做的命令，做不到的命令界面事前置灰。
- Tauri 命令只剩「monitor 自己的事」：窗口 · 拉前 · 本机 monitor 配置 · 日志与数据位置 · 重放缓冲 · 本机后端起停与引导 · 通道本身 · 放字节 · 足迹里 monitor 自己的事实。它们逐条登记在 `command_home_registry`，碰后端的新 Tauri 命令进不了这张表；全仓只有 `src/frontend/ui/ipc/commands.ts` 直接调 `invoke`。

---

## 2. 层边界

### 2.1 三层

- **前端**：把后端给的东西排版成像素、收手势，并在用户桌面上开终端窗口。不做判定、不拼命令串、不缓存业务数据。界面状态只有一份 pub-sub：`app-store.ts` 的 `Slice`（当前机器 · 账号快照 · 当前 tab），tab 那一层是 `tab-store.ts` ＋ `tab-router.ts`，overlay 视图的开关收在 `overlay-router.ts`。
- **通信层**：搬字节，零业务判断。面 A 是前端 ↔ 各处后端（按 `origin` 寻址，住 `src/comms/inward/`）；面 B 是 agent ↔ 上游 API，也就是中转（按路径前缀寻址，住 `src/comms/outward/`）。它不读盘、不起进程、不绑端口，凭据与端口由后端交给它。
- **后端**：其余一切干活的——读会话、起进程、动 tmux、拨 SSH、管资产、写用户的文件。

### 2.2 后端：一份代码、两种承载

后端（backend = 读 `observe/` ＋ 控制 `control/`）是一份代码、两种承载：本机常驻后端与每台远端的后端是同一个二进制，本机与远端的唯一差别是通道（管道 / SSH）。本机就是不走 ssh 的远端。

进后端的口有两个、平级：帧面（`stream/`，monitor 与外部前端经通信层面 A 走这条）与 CLI 面（`main.rs` 的一次性分派，用户敲 `ccm` 走这条；帧命令派生出同名的 CLI 子命令）。argv 只在 `main.rs` 里取一次。

模块地图（`src/backend/`，逐文件清单见它的 README）：

- `platform/`：平台原语与平台 cfg 的唯一住处（路径 · 进程 · 判活的读法 · 信号 · 文件原语 · 脱离）；`platform/shell/` 是 shell 方言知识的唯一住处（引号 · 定义函数 · 导出环境 · rc / `$PROFILE` 在哪）。
- `observe/`：产出观测帧的读（会话文件 watcher · 会话账本 · tmux 观测 · 历史 / 搜索 / 任务 / 账号查询）。
- `control/`：改状态的动作，以及只喂控制决策的只读查询（起会话与命令渲染 · kill · 送键 · 抓屏 · 分叉落盘 · 常驻与退出行为 · 部署计划 · 写用户文件的那一族 · 传输台）。
- `files/`：文件管理后端的读面（常驻文件名索引 · 按内容搜）。
- `accounts/`：账号域。账号 ＝ 订阅号 ＋ API 号；上游选择（这一发走哪个上游、注入什么凭据）住这里。
- `agents/`：用户的 AI CLI 的适配面（claudecode · codex），一家一行注册表，含记录解释。
- `plugin/`：调外部程序的口（找 · 问 · 起 · 收），cc-bus 那一族命令经它起。
- `relay/`：中转的宿主（门 · 监听）；中转本体是通信层面 B。
- `dial/`：SSH（连接池 · 链路 · SFTP · `~/.ssh/config` 解读 · 端口转发 · 开终端那一行）。
- `assets/`：skill / MCP / 别名的计算、判定与写，资产目录与同步，两台之间的装由本机后端当枢纽。
- `history/`：历史跨机 join 与注解（星标 / 改名 / 隐藏）。
- `faces/`：只读面与功能侧的帧面宿主；`stream/`：帧定义 · 入方向命令信封 · 监听 · 本机后端问远端后端的唯一一处。
- `footprint/`：足迹（这台机器上 cc-monitor 写过什么、能不能撤）；`common/`：共用判定。

后端分两块、零互相依赖：**原生后端**（会话 · tmux · 账号 · SSH · 资产 · 历史 · 中转宿主）与**文件管理后端**（列 · 读 · 搜 · 写用户的文件 · 上传解压 · 传输），两块只共用 `platform/` · `common/` 与基础设施层。

### 2.3 `observe/` 与 `control/`：按用途分，不按读写分

- 任何改状态的 tmux 命令（`set-hook` · `set-option` · `new-session` · `kill` · `send-keys`）归 `control/`。
- 只喂控制决策的只读查询也归 `control/`（例：`control/gate.rs` 探 `@ccm_sid`，它不产观测帧）。
- 只有产出观测帧的读才归 `observe/`。

方向单向：`control/` 引用 `observe/` 零容忍；`observe/` 引用 `control/` 许有，但逐条登记、条数钉死（`tests/backend/layering_guard.rs` 的 `ALLOWED_OBSERVE_TO_CONTROL`，每条写着为什么非得由观测侧发起）。按读写分的话，那次 `@ccm_sid` 探测会被判给 `observe/`，而它唯一的调用方在 `control/`，平白造出一条反向边。

### 2.4 monitor：宿主 ＋ 通信层

monitor 的 Rust 半是 Tauri 壳（`src/frontend/shell/`），只留宿主知识（窗口 · 起子进程 · 开终端 · 放字节 · 本机后端起停）与通信层；读会话、判定、改世界都在后端。壳里调后端的客户端那一组生产段零平台原语、零宿主耦合，由 `backend_client_guard_tests.rs::the_backend_half_stays_platform_agnostic` 钉着。

壳里没有后端那几层的副本：

- 不产观测帧，读都问后端（本机那几问也走 `<local>` 长连接；每问 exec 一次本机后端的那份传输 `local_query`〔散文墓碑〕已删）；
- 调后端控制面只经通信层那一个分流器 `src/comms/inward/backend_route.rs`（`Done` / `Refused` / `NoChannel` 三态，被门拒绝不另找一条路）；
- 与后端共用的只放在共享 crate `src/common/` 里；
- 壳自己的 `platform/` 住壳要的平台原语：文件原语（不覆盖改名 · 置可执行位 · 只给本人的目录）与控制台输出的解码（Windows 按那台的 OEM 代码页）。

monitor 里仍直读本机 agent 目录的地方逐处登记，条数以 `local_read_surface_registry` 的机检为准。

**搬不走的那条边界**：最后那次 exec 必须发生在用户自己的终端进程里——pid 要等于 pidfile 名，tty 与 Ctrl-C 要落在 agent 上，`tmux attach` 要占住调用者的终端。所以起一个会话拆成三个平面：

1. 计划面「跑什么命令」→ 那台后端出成品，**永远只是一行 `ccm …`**（本机帧命令 `launch-local`，远端 `launch-render-cli`；开终端那一行 `terminal-ssh` 只包它）。环境、中转地址、身份标记由那台的 `ccm` 在最终 exec 那一处定；
2. 执行面「在那台真的建 tmux」→ 后端 `control/launch`，argv 直传、不过 shell；
3. 开窗面 → 只能是 monitor（`open_terminal_window`）：后端在远端，开不了你面前的窗。平面 ③ 永远搬不走。

平面 ③ 在 POSIX 上只走规范化出口 `xdg-terminal-exec`，不替用户挑具名终端模拟器；没有这个出口就不开窗、说清楚（`launch_tests.rs::no_terminal_emulator_is_ever_spawned_from_this_file` 钉着）。

### 2.5 `platform/`：判据是跨 target 编译

`platform/` 是唯一允许平台原语与平台 cfg 的地方。判据不是「cfg 出现在哪」，是跨 target 编译：后端 CI 跑 `cargo check --all-targets --target x86_64-pc-windows-msvc`，本地门禁的 `winchk` / `winchk-backend` 两格在 Linux 上对 Windows target 编一遍；`tests/backend/platform/cfgless_guard.rs` 补它的盲区（不带 cfg 的平台代码）。

「怎么读到这个事实」下沉 `platform/`（Linux 读 `/proc`，Windows 读 Win32），「什么叫同一个活进程」留通用层、两平台共用一张判定（`liveness::is_same_live_process`）。连规则也下沉，就成了每个平台一套规则。

### 2.6 零轮询：一律事件驱动，四张账本

零轮询不是一句口号，它由四张登记表覆盖，口径是「每一处周期唤醒都说清事件源在哪、谁退役它」：

| 账本 | 管哪一块 |
|---|---|
| `no_timer_guard`（后端） | 后端里不许有自己醒过来的构件，零容忍 |
| `polling_registry` | 前端 TS 与 `src/shared/` 下的 shell |
| `rust_timer_registry` 的 `REGISTERED` | monitor Rust 里的 `sleep` / `interval` |
| `rust_timer_registry` 的 `SHELL_WAKES` | monitor Rust 拼出来的 shell 循环（前三张都看不见它） |

后端也不产出自带节拍的 shell 串（`no_timer_guard` 的 `f09_external_beat` 零命中）。期限一律由发起那件事的一方给一个绝对时刻，下游只收紧，通信层自己没有期限常量。

### 2.7 共享 crate 与 workspace

`src/common/` 住 monitor 与后端共同 link 的共享 crate。它们只放两边必须对上的**契约**（路径 · 端口 · 文件格式 · 文案表 · 令牌形状 · 字节表键），不放**判定**；判定只在后端。monitor 生产段只许依赖契约类 crate，由 `tests/frontend/shell/contract_crate_guard_tests.rs` 两向钉住。共享常量让两侧「想不一致」得先把 import 删掉，漂移变成不可表示。

后端 crate 刻意不是 workspace 成员：它要能在目标机上原生构建（发版交叉编成 musl 静态二进制）。壳的 workspace 成员是 `monitor` ＋ 共享 crate，个数以 `src/frontend/shell/Cargo.toml` 现量为准。

代价：在 `src/frontend/shell` 里跑 `cargo test --workspace` / `cargo fmt --all` 覆不到后端 ⇒ 测试、格式、clippy 要两处分别跑（壳的 workspace · `src/backend`），门禁的 `cargo` · `backend` 两格就是这两处。

### 2.8 其余几块

- **中转**（通信层面 B）：本机远端同形，是那台常驻后端进程里的一条线程，对外端口由它绑，进门要钥匙。凡经 `ccm` 起的会话都注入中转地址（`ccm` 在最终 exec 那一处问上游选择，开关读 `CCM_RELAY_ALL_SESSIONS`）；用户自己设了 `ANTHROPIC_BASE_URL` 时不注入并说一句。中转只切流，这一发走哪个上游、注入什么凭据由账号域的上游选择定，monitor 对凭据文件零读零写。
- **文件管理**：`cc-monitor-filewin` 是独立前端，经回环通道 ＋ 钥匙只说 `call` / `subscribe`。写用户的文件只经那台后端的文件管理面（`files-peek` · `files-put` 带期望值 · `files-delete` …），本机远端同一条路；monitor 碰用户文件只有一个开口，只差 `origin`。
- **部署**：后端的字节随 monitor 内嵌，全仓只有壳的字节表一个取字节口，按目标机器的 (OS, arch) 选，没覆盖的格子写第一个字节前拒绝。「换不换、换成什么」由后端帧命令 `deploy-plan` 出计划（只升不降，身份读字节里的戳、不跑它），monitor 只按计划放字节。
- **资产**：每台后端一份资产目录，本机后端按事件在各后端之间拉 / 合 / 推；装到别的机器要用户点，先看差异。
- **退出行为**：住那台机器自己的 `~/.cc-monitor/backend.json`，只有那台后端读写（`control/exit_policy`），决定那一刻现读。

---

## 3. Tauri State：只有一个家

State 注册表住 [STATE-MATRIX.md](STATE-MATRIX.md)，改 State 前后都读它，撤回 / 修改任何 Tauri 命令时它是强制 checklist。

为什么值得一整份文档：漏一次 `app.manage()` 不会被 `cargo check` 抓住——命令签名照样编译过，运行时第一次调用才 panic。Tauri 的 State 注入是运行期按类型查表的，编译器在这条路上帮不了你，只能靠一份清单兜着。

---

## 4. 数据放在哪

monitor 自己的文件在 `~/.cc-monitor/`：

| 路径 | 写入方 | 读取方 | 用途 |
|---|---|---|---|
| `config.json` | monitor 设置 | monitor | 主题 · 字体 · 行为开关 · 机器表 · 诊断 |
| `ps-await/<PID>.json` | PowerShell（`__ccm_bind`：每开一个 PowerShell 后台一次 · 敲 `cc` 时前台一次） | monitor `bind.rs` | PS 通知 monitor 去找标题含 marker 的窗口（短暂，3s 超时） |
| `ps-registry/<PID>.json` | monitor | PowerShell · monitor（本机会话 ↗ · 远端会话 ↗ 先查它） | monitor 回告绑定成功与 HWND（与 PS 进程同寿） |
| `sid-hwnd-cache.json` | monitor | monitor | sid → 窗口把手的持久缓存 |
| `auto-launch.json` | monitor 设置 | PowerShell | 「用 `cc` 起 claude 时自动开 monitor」开关 ＋ monitor 路径 |
| `history-metadata.json` | 本机后端 | 本机后端 | 历史注解（星标 · 改名 · 隐藏 · 上次账号） |
| `logs/monitor/` | monitor | 用户 | 按天滚动的诊断日志 |

后端自己的状态（`backend.json` · 中转钥匙 · 监听令牌 · 资产目录 · skill 装记录 · API 号凭据 · 后端日志 · `bin/`）由那台后端写，写者逐文件登记在 `readonly_guard.rs` 的 `OWN_STATE_WRITERS`。

只读的外部数据源：`<claude_dir>/projects/**/*.jsonl`（会话内容）· `<claude_dir>/sessions/<PID>.json`（活跃会话，PID ＋ `procStart` 双校验）· `<claude_dir>/tasks/<sid>/`（Task 面板），都由那台后端读。

字段定义、编码约束（UTF-8 无 BOM）、原子性与握手时序 → [IPC-PROTOCOL.md](IPC-PROTOCOL.md)。每个文件的完整路径在设置的「数据位置」页。

---

## 5. 关键设计选择与理由

每条都是「为什么不能用别的方案」。

### 零侵入：只读 Claude Code 的数据源

后端只读 `projects/` 与 `sessions/`。写入一律是用户显式触发，而且只经那台后端的文件管理面：删历史会话只收 sid（`files-delete-session`）；从某一轮分叉只新增一份 `<new-sid>.jsonl`，`O_EXCL` 新建、绝不覆盖，原会话零改动（`control/fork_write`）。按 sid 找那份会话文件只有一份实现，两处共用（原先收路径的源守卫 `validate_branch_source`〔散文墓碑〕已不在）。装别名块、skill、MCP 是用户点名的写，别名块只动 BEGIN / END 块内。

**为什么**：用户对「数据源就是我自己的命令痕迹」的认知不能破；写是必要时的可选副作用，就得是显式的、可见的（足迹页逐条列出）。

### 起子进程只有一个出口

monitor 起子进程一律经 `spawn_managed.rs`，三个策略都是必填参数、都没有 `Default`：控制台（`ConsolePolicy`：`Hidden` · `NewVisible` · `Inherit`）· 生命周期（`Lifetime`：`JobKillOnClose` · `Detached`）· stderr 去向（`StderrSink`：`ToLog` · `Null` · `Inherit` · `Captured`）。还自造 `Command` 的地方逐处登记。给用户开真终端那一处刻意是 `NewVisible`。

**为什么**：Windows 上 GUI 进程起一个控制台程序而不给 flag，系统会新配一个带窗口的控制台——用户看到就会关，关掉就杀死子进程（退出码 `0xC000013A`）。分进程之后，原来免费的东西都要显式管：谁杀谁、控制台策略、错误怎么跨进程传、两边对版、起不起得来。三个必填参数让坏默认值无法被表达；子进程的 stderr 接进 monitor 日志，死亡码说人话（「被控制台事件杀死」而不是裸退出码）。

### 顺序靠 seq，不靠后端保序

重放时就绪点持锁只做快照、把订阅置为实时，交格全在锁外；顺序保证交给 per-file 单调 seq 加前端二分插入。并发到的实时行先于快照旧行到达也无碍。跨通道的顺序（行 vs `session-ended`）不由 seq 覆盖，由同队列同序（INVARIANTS §20）与大批在调用方任务里发完再返回（INVARIANTS §10）兜住。

**为什么**：持锁完整交付会让重放期间 watcher 阻塞数十毫秒到秒级；seq 排序把「后端保序」变成「前端排序」，交付顺序成了纯性能自由度（优先会话先交就是用的这份自由）。

### 成批交付

重放按块投递，一次 Tauri 事件里一串格，前端推进同一条队列走批量调度。

**为什么**：每次 emit 都有序列化与派发开销，逐条交付几千行时主线程累计阻塞明显，启动可见卡顿。

### 视口外不渲染：content-visibility ＋ 估高 ＋ 骨架

- 顶层卡片带 `content-visibility: auto`，视口外与隐藏 tab 的卡片跳过布局与绘制；`contain-intrinsic-size` 初值由 `height-estimate.ts` 按块类型估（正文用 pretext 测宽、代码块按行数），估值只是初值。估高路径绝不许抛，失败就降级。
- 拿得到会话索引的 tab 与历史查看器接上 `SkeletonView`（`skeleton-view.ts`）：没物化的 seq 区间由占位顶住，一接上滚动条就是全会话的；每次滚动只物化与视口相交的那一段，正文按偏移向那台后端取回。
- 没接骨架的 tab 由 `TailWindow`（`live-window.ts`）收纳：只物化尾部，更老的记录只进账本不建卡，滚到顶部附近再按批补。

**为什么**：建卡（markdown · DOMPurify · 高亮）是重放期最大的成本；不建看不见的卡，上万条记录的会话也能秒开。

### 启动重放贴底不抖

- 守卫式 `snap()`：只在落后底部超过 1px 时才写 `scrollTop`，不每帧重钉。
- 窗口内的中部插入交给原生 `overflow-anchor`，不手动补偿（叠加会双重位移）。
- 尾部优先收纳：当前 tab 的尾块直接渲染，更老的块与后台 tab 的记录只进账本；后台 tab 空闲时物化尾部，切过去时同步物化。

**为什么**：旧内容逐条插到贴底视口上方，会让浏览器逐帧重排并重做滚动锚定，高分屏上分数像素的舍入误差每帧不同，整块上下抖。细则在 INVARIANTS §21。

### 每个窗口一个入口

`index.html` · `settings.html` · `viewer.html` 各有自己的入口（`entry-main.ts` · `entry-settings.ts` · `entry-viewer.ts`）。设置窗的模块图里没有高亮、数学排版与 tab 管理；设置窗关窗是隐藏、复用时重跑取值，主窗销毁时连带销毁它（`lib.rs::windows_to_destroy_after`）。独立只读窗口复用 `TabManager` 过滤到那个会话，订一条 `session-lines/<sid>`，不发 `frontend-ready`。

**为什么**：webview 之间的 JS realm 互不共享，一个入口顶层同步 import 了全部视图，运行期分派到哪一支都要把整张模块图执行一遍——修法只有多入口。viewer 不另写渲染器：再写一套就会与主管线漂移。

### 账号：隔离又同步

一个账号 ＝ 一个 `CLAUDE_CONFIG_DIR`：各自一份登录凭据，两个号可以同时跑、互不踢；skill、记忆、历史、设置经符号链接共享同一份（由后端 `accounts/manage/` 建立和维护）。账号分订阅号与 API 号；API 号的 key 只留在它所在的那台机器，经那台的中转注入，不进命令行、不进环境变量。界面只交意图：建库 · 添加 · 删除 · 设默认 · 修复 · 回滚由后端执行，每次改动前先备份；订阅号的登录走真实终端窗口，由 claude 自己完成。

换号重启直接结束旧会话再 resume 同一个 sid；compact 失败不阻断，kill 失败必须中止——绝不在旧进程还活着时续 resume，否则新旧两个进程抢同一份会话。选不了原账号时不静默换号，拒绝并给出「用当前账号」的显式选择。

### 判活：PID ＋ procStart

活跃会话按 `sessions/<PID>.json` 判：进程在 ∧ 文件里有 `procStart` 时再比进程创建时间。读法住后端 `platform/`（Windows 读 `GetProcessTimes`，Linux 读 `/proc/<pid>/stat` 第 22 字段），判定两平台共用一张。`procStart` 是平台原生格式，各自与本平台的查询口径同源，不需要启发式。

- **为什么不只查 PID**：Windows 的 PID 短期复用很常见，只看进程在不在会把僵尸条目判成活跃。
- **为什么 `procStart` 可缺**：Claude Code 在某些启动路径下写 pidfile 会漏这个字段；缺了就只看 PID，而不是整条解析失败、漏掉一个 tab。
- **解析 `/proc/<pid>/stat` 不能朴素按空白切**：第 2 字段 `comm` 可以含空格与括号（`tmux: server` 就是），要从最后一个 `)` 之后数。

### 拉前三重校验

Windows 上把终端窗口拉到前台前要同时满足：窗口把手还有效 ∧ 当前 owner pid 等于绑定时的 ∧ owner 的进程创建时间等于绑定时的。任一不符就拒绝拉前并说原因。本机的绑定来自握手；远端会话的窗口是点 ↗ 那一刻沿进程链现找的，过同一道校验。

**为什么**：窗口把手复用比 PID 复用还频繁，不校验 owner 会把不相干的窗口拉到前面。

### marker 握手：先改标题，后写文件

PowerShell **先**把窗口标题（WindowTitle，`[System.Console]::Title`）设成唯一 marker，**后**写 `ps-await/<PID>.json`；monitor 收到文件后 `EnumWindows` 找标题含 marker 的窗口，找不到就重试（最多 600ms），找到写回 `ps-registry/<PID>.json`，PS 看到后恢复标题。顺序不可换：反过来 monitor 会在文件落地瞬间去找一个还没设上的标题。每开一个 PowerShell 都在后台做一次（只在 monitor 在跑时，不等、不出声）；敲 `cc` 时前台再确认一次。时序图在 [IPC-PROTOCOL.md § 跨进程握手时序图](IPC-PROTOCOL.md)。

**为什么本机不按进程找窗口**：PowerShell 不拥有终端窗口（Windows Terminal 是单独进程，cmd 走 conhost，VS Code 走集成终端），window owner 不等于 PS，从 claude 往上数常常数不到那个窗口。让 PS 改自己窗口的标题、再按标题反查，不依赖进程关系。远端会话没有这一步可走（终端可能是用户自己开的 `ssh`），它从连接的拥有者往上数（见 §6）。

### UTF-8 BOM 双向防御

PS 模板 `src/shared/cc.ps1.tpl` 用 `UTF8Encoding($false)` 写无 BOM；Rust 端 `bind.rs::process_await_file` 解析前先剥掉任何 BOM。

**为什么**：PowerShell 5.1 的 `Out-File -Encoding utf8` 默认写 BOM，`serde_json` 不剥就解析失败；源头修 ＋ 接收端兜底，旧模板也能用。

### tooltip 挂在 body 上

设置里的 `?` 提示框挂到 `document.body`，`position: fixed` ＋ 按视口算坐标。

**为什么**：祖先有 `transform` 时，`position: fixed` 的包含块从视口变成那个祖先，坐标就不再是视口坐标，提示框会跑出屏幕。

### 日志：tracing 在 Builder 之前初始化

`logging::init` 必须在 `tauri::Builder::default()` 之前调用（全局 dispatcher 只能装一次）：文件层按天滚动、非阻塞写；`EnvFilter` 可热改级别，不重启就生效；ERROR 级经 `logging.rs::install_error_emitter` 注入的回调发 `monitor-error`，前端弹提示（限流防风暴）。日志目录建不出来就退化成只写 stdout，monitor 照样起。后端子进程的 stderr 接进 monitor 的滚动日志，脱离起的后端写自己那份日志。

**为什么**：release 版是 GUI 子系统，没有 stderr；日志不落盘，一条解析失败的 warn 就没人看见。

### Win32 同步调用走 spawn_blocking

`bring_terminal_to_front` 等 Win32 同步调用放进 `spawn_blocking`，前端再加超时兜底。

**为什么**：`EnumWindows` / `SetForegroundWindow` 这类调用可能阻塞数十毫秒到秒级，放在 Tauri 主 runtime 上会卡住 IPC 派发（INVARIANTS §10）。

### bring_monitor_to_front 三层 hack

用户在终端里敲回车、monitor 自动跟到那个 tab 并可选拉到前台时，monitor 不是前台进程，直接 `SetForegroundWindow` 会被 Windows 拒绝（防偷焦点）。`lib.rs::bring_monitor_to_front` 叠三层：

1. `keybd_event(VK_MENU)` 模拟一次 Alt，让系统视本进程为「刚有用户输入」；
2. `AttachThreadInput` 附加到前台线程的输入队列，借它的拉前权限；
3. `SetWindowPos(TOPMOST → NOTOPMOST)` 强制拉到 Z 序顶部，前两层都失败时至少视觉上浮顶。

单层在 Win10 1903+ 上都不够，三层叠加才稳。

---

## 6. 刻意不做的

- **按系统前台窗口切 tab**：Windows Terminal 一个进程多个窗口、多个 tab，前台窗口只拿得到 WT 主进程的把手，分不出是哪个 tab。改为看 jsonl：用户在 claude 里敲回车，claude 写一行 `type=user`，monitor 切到那个 tab（INVARIANTS §20）。
- **子运行的记录走主会话的行**：子运行（子 agent）的记录数量大，全量推会把重放缓冲撑大数倍、还会混进主时间线。那台后端照样盯着它们
  （与主记录同一条文件事件管线），只出成品：运行表帧 `session_runs`（列在 agent 面板里，不进主 tab 的消息流）；点面板那一行 / 派出它的那张卡的卡头
  开它自己的窗口（agent 窗口，查看窗那个入口带 `run=`），窗口里才按运行读它的记录（帧命令 `history-run`），运行表一变就续读。流那一侧由后端归位（`tap` 帧带 `run`），子运行的流不上主 tab 的活卡。
- **从 claude 的祖先链猜终端窗口**：explorer 起 PowerShell ＋ WT 接管控制台的常见架构下，claude 的祖先链与 WT 窗口完全脱节，启发式在主流环境下都不可靠。本机改为终端主动告诉 monitor 它是哪个窗口（PowerShell 接入块的 marker 握手）。远端会话从此刻连着它的终端出发：那台读出那个终端的 `SSH_CONNECTION`（在 tmux 里就问 tmux 此刻哪些客户端连着），本机后端按这四元组在连接表里找到这台电脑上开着那条连接的进程，往上数父进程到 `explorer.exe` 为止，monitor 取链上第一个有可见窗口的进程。链从连接的拥有者起、不从 claude 起；找不到就照实说原因（在 tmux 后台没人连着 · 不在这台电脑上 · 经跳板或端口转换对不上 · 没有窗口或不止一个）。
- **换掉 webview**：这个 app 的核心是渲染会话记录（Markdown · 代码高亮 · LaTeX · 可折叠工具卡 · 流式追加 · 上万条记录的虚拟化），正是 HTML 最擅长、原生 GUI 工具箱最不擅长的那一类。文件管理器窗口不渲染会话记录，所以它是原生（egui）的。

---

## 7. 入门读图

- 整体数据流：本篇 §1 ＋ §5
- 一件事该放在哪一层：本篇 §2
- 加 jsonl 记录类型、加帧命令、加设置项 / 快捷键：[CONTRIBUTING.md](CONTRIBUTING.md)
- 改跨进程文件或帧协议：[IPC-PROTOCOL.md](IPC-PROTOCOL.md)
- 改某个具体模块：对应目录的 README

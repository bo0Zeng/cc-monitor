# 架构总览

cc-monitor 是 Claude Code 会话的**观察者和启动器**：`claude` 跑在用户自己的终端里，cc-monitor 读它写下的会话记录来显示，替用户起会话、管机器，不接管 `claude` 本身。

本篇回答：由哪几个进程组成、会话内容从哪儿来、经过谁、停在哪儿；一件事该放进哪一层，以及为什么这么分。新贡献者先读这一篇。

相关文档：

- 全局不变量 → [INVARIANTS.md](INVARIANTS.md)
- 帧协议、一次性命令与跨进程文件 → [IPC-PROTOCOL.md](IPC-PROTOCOL.md)（总述）· [IPC-COMMANDS.md](IPC-COMMANDS.md)（逐格，从代码生成）
- 加东西 / 撤东西的做法 → [CONTRIBUTING.md](CONTRIBUTING.md)
- 逐文件清单 → 前端 [`src/README.md`](../README.md) · 壳 [`src/frontend/shell/README.md`](../frontend/shell/README.md) · 后端 [`src/backend/README.md`](../backend/README.md)

---

## 1. 进程与数据流

### 1.1 进程

| 可执行文件 | 是什么 | 谁起它 |
|---|---|---|
| `monitor` | 界面进程：webview 窗口 ＋ 通信层面 A 的客户端。零 SSH，不写用户文件 | 用户 |
| `ccm`（开发树里叫 `cc-monitor-backend`） | 唯一的后端。每台机器上 `~/.cc-monitor/bin/ccm` 就是那台的后端，也是用户敲的 `claude` 的壳（argv 打头是 `--` 加后端词才进后端，其余交给 `claude`） | 本机：monitor 连上来时起；远端：monitor 接那台时经一次 exec `--resident-ensure` 起（已在跑就用那一个） |
| `cc-monitor-filewin` | 文件管理器窗口，独立前端，一个窗口一个进程 | monitor（父子管道：通道就是窗口进程的 stdin / stdout，不监听） |

`claude` 不是我们起的进程：后端渲好一条命令串，monitor 交给用户自己的终端去 exec。

```
本机                                                          远端（每台）
┌ monitor（界面，零 SSH）─────┐   父子管道   ┌ 文件窗口 ×N ┐
│ webview 窗口 · 通道客户端   │◀─────────────▶│ call /      │
└──────┬─────────────────────┘               │ subscribe   │
       │ 本人 Unix 套接字 / 管道              └─────────────┘
┌──────▼──────────────────────────┐  SSH（链路 · exec · SFTP）  ┌ 常驻后端（与本机同形）┐
│ 本机常驻后端                      │ ──────────────────────────▶│ ＋ 中转（进程内）      │
│ 持有到各远端的全部 SSH（含 SFTP） │                            └───────────────────────┘
│ 中转 ＋ 上游选择                  │ ◀── agent 经中转口连进来
└─────────────────────────────────┘
```

- 本机固定两个进程：monitor ＋ 本机常驻后端。Linux 上后端脱离起，只听家里的本人 Unix 套接字 `<家>/run/backend.sock`（`run/` 只给本人、收连接时核对端 uid、目录独占锁；没有钥匙），monitor 接上它；monitor 关了，后端按那台机器的「退出行为」留下或退出，下次 monitor 起来接回。Windows 上后端是 monitor 监护的子进程（管道），随 monitor 退出。
- 远端每台一个常驻后端，与本机同形。本机后端在那条 SSH 连接上开一条链路，在那台跑 `ccm -- --resident-attach` 小中继去连那台的套接字、原样对拷；不另开任何口。那台后端比 monitor 旧就换一份。远端只支持 Linux（x86_64 / aarch64），别的系统连上时显式拒绝。
- 门与载体的细则（attach 行、拒绝理由）在 [IPC-PROTOCOL.md](IPC-PROTOCOL.md) §1；为什么没有钥匙在 INVARIANTS §48.1。
- 一个常驻后端可以同时接多个客户，连接数归零那一刻按「退出行为」当场决定退不退。

### 1.2 会话内容流：本机与远端同一条帧路

```
   <claude_dir>/projects/<编码后的 cwd>/<sid>.jsonl        （claude 写）
                 │  那台后端的 observe/watcher 盯文件，逐行出 line 帧（JSONL）
                 ▼
   本机：本人套接字 / 管道  ·  远端：本机后端持有的那条 SSH 长连接上的链路
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
- **启动序**：主窗口先经通道订好每台机器的会话流，再发 `frontend-ready`（带优先会话）——那就是这些订阅的就绪点。monitor 在 `event_replay.rs::ready_point` 里按会话分组切块、优先会话先交，并按活跃集补发 `session-ended`，归档落在全部重放行之后（INVARIANTS §24）。本机活会话的骨架 tab 也由这条流的起停帧给出。
- **冷读也问那台后端**：历史清单、整页正文、按偏移读、按行号读、子 agent、全文搜索都是帧命令（`history-*`）。记录解释（一行 jsonl → 通用记录 `agents/record.rs`，各家的翻译表在 `agents/<名>/`）只在后端，界面按 `t` 与格排版；主线外清单（ESC 回退掉的那几条）也是后端给的成品（实时帧 `session_branch` · 冷读 `history-branch`）；多台的搜索结果由本机后端合并排序。
- **Task 面板**：那台后端盯 `<agent 家>/tasks/`，一批事件按 sid 去重发 `tasks_changed`，界面订 `session-tasks`，收到就重问 `tasks-list`。

### 1.3 每条线的源头

按「源头是谁」切，是不会切错的那种切法：

| 线 | 源头 |
|---|---|
| 会话内容 · 历史 | `<claude_dir>/projects/**/*.jsonl`（冷读与实时读同一份） |
| 判活 | `<claude_dir>/sessions/<PID>.json` ＋ tmux 会话上的 `@ccm_sid` |
| 流量 | HTTP 请求本身（中转看得见的那一份） |
| 起会话 · 控制 | 用户意图 |
| 窗口绑定 · 拉前 | 点 ↗ 那一刻现查一串进程：本机从 agent 进程往上；远端从此刻连着那个会话的终端（那台报出它的 `SSH_CONNECTION`，本机后端在连接表里认出这台电脑上开着那条连接的进程链）。每一级「显示在哪个窗口」：Windows 问它的控制台；Linux 问 bash / zsh 接入块认下的那一份 |
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

- 不产观测帧，读都问后端（本机那几问也走 `<local>` 长连接）；
- 调后端控制面只经通信层那一个分流器 `src/comms/inward/backend_route.rs`（`Done` / `Refused` / `NoChannel` 三态，被门拒绝不另找一条路）；
- 与后端共用的只放在共享 crate `src/common/` 里；
- 壳自己的 `platform/` 住壳要的平台原语：文件原语（不覆盖改名 · 置可执行位 · 只给本人的目录）与控制台输出的解码（Windows 按那台的 OEM 代码页）。

monitor 里仍直读本机 agent 目录的地方逐处登记，条数以 `local_read_surface_registry` 的机检为准。

**搬不走的那条边界**：最后那次 exec 必须发生在用户自己的终端进程里——pid 要等于 pidfile 名，tty 与 Ctrl-C 要落在 agent 上，`tmux attach` 要占住调用者的终端。所以起一个会话拆成三个平面：

1. 计划面「跑什么命令」→ 那台后端出成品，**永远只是一行 `ccm …`**（本机帧命令 `launch-local`，远端 `launch-render-cli`；开终端那一行 `terminal-ssh` 只包它）。环境、中转地址、身份标记由那台的 `ccm` 在最终 exec 那一处定；
2. 执行面「在那台真的建 tmux」→ 后端 `control/launch`，argv 直传、不过 shell；
3. 开窗面 → 只能是 monitor（`open_terminal_window`）：后端在远端，开不了你面前的窗。平面 ③ 永远搬不走。

平面 ③ 在 POSIX 上挑一个终端开窗：设置里指定的 → 系统出口（`xdg-terminal-exec` · `x-terminal-emulator`）→ 常见终端逐个探；都没有就不开窗、说清楚去设置里指定（`launch_tests.rs::the_terminal_is_picked_by_setting_then_system_exits_then_common_ones` 钉着）。

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
- **文件管理**：`cc-monitor-filewin` 是独立前端，经父子管道（它的 stdin / stdout）只说 `call` / `subscribe`。写用户的文件只经那台后端的文件管理面（`files-peek` · `files-put` 带期望值 · `files-delete` …），本机远端同一条路；monitor 碰用户文件只有一个开口，只差 `origin`。
- **部署**：后端的字节随 monitor 内嵌，全仓只有壳的字节表一个取字节口，按目标机器的 (OS, arch) 选，没覆盖的格子写第一个字节前拒绝。「换不换、换成什么」由后端帧命令 `deploy-plan` 出计划（只升不降，身份读字节里的戳、不跑它），monitor 只按计划放字节。
- **资产**：每台后端一份资产目录，本机后端按事件在各后端之间拉 / 合 / 推；装到别的机器要用户点，先看差异。
- **退出行为**：住那台机器自己的 `~/.cc-monitor/backend.json`，只有那台后端读写（`control/exit_policy`），决定那一刻现读。

### 2.9 核心与适配层：成品 · 格目录

后端里「判定与写字」只有一处，叫**核心**；它两边各是一层适配。手机、CLI、桌面界面、对端后端都是出口；任何出口要改形，只要核心里已经有那一格，就只改出口自己。

```
 输入适配层（每家 agent 一个）        核心（后端，一处）                     输出适配层（每个出口一个）
 claudecode · codex · fake    ──▶  通用会话 / 记录模型                ──▶  桌面：帧 → 壳（只转运）→ 界面（只排版）
 盘上原始格式 ⇒ 通用模型              所有判定、所有成品只算一次              CLI：--json / --text
                                     成品 ＝ 值 ＋ 写好的字 ＋ 语气            手机：经 CLI 面或流
                                                                            对端后端：扇出时问远端
```

| 层 | 做 | 不做 |
|---|---|---|
| 输入适配层（`agents/<家>/`） | 把那一家的盘上格式翻成通用模型（`agents/record.rs::Record` · 会话活动 · 步骤结果的数）；申报那一家用了哪些文件 | 判定、写给人看的句子（给码 ＋ 原话，句子由核心写） |
| 核心（`observe/` · `control/` · `accounts/` … 与出成品的 `faces/`） | 一切判定；每件成品出全量格：值 ＋ 写好的字 ＋ 语气 ＋ 时刻字 | 认「是哪个出口在问」；为某一个出口加开关 |
| 输出适配层（壳 ＋ 界面 · CLI 面 · 手机仓 · 扇出那一处） | 挑格（要哪几格）、按核心算好的格筛 / 排、按预算截、装运（帧 / 一行 JSON / 文本） | 判定、写句子、格式化（时长 · 大小 · 百分比 · 按码取字） |

**成品约定**（每件成品都是这几种格的组合，出口不需要认业务）：

| 格 | 是什么 | Rust 类型 |
|---|---|---|
| 值 | 机器要的码、数、时刻（epoch / ISO 原样）、id | 普通类型；闭集的词是枚举的单元变体 |
| 写好的字（`text` · `…Text`） | 核心按文案表与这台的语言、本地钟写好的一句 | `common::cells::Words` |
| 语气（`tone` · `…Tone`） | 闭集：`plain` · `fail` · `now` · `need` · `warn`（该留意了，还没出错：上下文快满）· `busy`（后台有事在跑、不要人）；要用到再加 `ok` | `common::cells::Tone` |
| 活钟 `{dur}` | 要随时间走的那一截：`{text: "…{dur}…", from, to}`，出口用登记过的那一个读口填 | 过程行 `span` |
| `detail` · `raw` | 复制详情（只在失败时）· 原文透传（受认证约束的可选格） | — |

「这一格是值还是写好的字」由它的 Rust 类型说，不按格名猜：`blocks[].text` 是原文（值），`cost.text` 是核心写的字。界面自己的骨架字（按钮、菜单、标题、空状态）留在界面；凡是描述数据或状态的字都由核心写。

**会走的钟**：核心写的时刻字是发出那一刻的字。手机与 CLI 要它跟着走就按节拍重问（出口零格式化）；桌面秒级走的那几处（过程行 `{dur}` · 「已等」）只留一个时长读口（TS `duration-format.ts::fmtDur`，与 Rust `copy_core::short_duration` 对同一份金样 `short-duration.golden.json`）。

#### 格目录 `cells-catalog`

只读帧命令 `cells-catalog`（本体 `faces/cells_catalog.rs`）列出每件成品有哪些格（路径 · `value` / `text` / `tone` · 类型），出口据它就知道有没有那一格，不必读核心代码。目录不手写：每件成品登记一组用自己的 Rust 类型造的样本，经同一份 `Serialize` 走查出格路径。哪些成品已登记、格的路径写法、`pending`（判了要补、还没落地的格）与冻结规则，以命令本身的输出与 `tests/backend/faces/cells_catalog_tests.rs` 的头注为准；冻结的成品（`record` · `read_row`）格只许加。

#### 出口怎么挑格：请求信封里的声明

出口在请求信封里交自己的声明 `view`（帧面那一格 · CLI 面 `--view`，同一份）：今天的词是 `cells`（只要这几格）与 `omit`（这几格不要），路径照格目录写。
核心只一个通用投影（`faces/project.rs::project`）：声明先按格目录校验（认不出的词 · 成品 · 格 ⇒ `bad_args`，不静默放过），每条命令的应答里哪几处住着哪件成品登记一次
（`stream/inbound/views.rs::PLACES`），成功的应答照声明裁好再装运；各命令不再自己认「要不要正文」一类的开关（`summaryOnly` 删了）。
还靠开关定形的：流旗标 `--tail-only` · `--with-bg` · `--with-pid` · `--with-raw`，入参 `whole` · `--index` · `raw`，`quota-read --text` —— 收进声明是后面几刀。
新的出口需求不再往核心里加开关，照下表判。

#### 一个出口的新需求：只改出口，还是动核心

| 问 | 是 ⇒ | 否 ⇒ |
|---|---|---|
| ① 格目录里已经有这一格（值或写好的字）？ | 下一问 | **动核心**：补这一格，对所有出口一次补齐 |
| ② 只是「要 / 不要某几格」「按已有的格筛 / 排」「换预算」「换装运」？ | **只改出口** | 下一问 |
| ③ 要的是新判定、新句子、新的跨会话 / 跨机器汇总、新的推送时机？ | **动核心**：新判定进核心、新格进目录；之后同类需求回到 ② | — |

格已有 ⇒ 只改出口；格没有或要新判定 ⇒ 动核心一次，补给所有出口。出口永远不自己算一格。动核心的每一刀走全套：命令登记 → `IPC-COMMANDS.md` 重生成 → 冻结表 / 金样同拍 → `BUILD_ID`。

---

## 3. Tauri State：只有一个家

State 都在 `lib.rs` 的 `setup()` 里 `app.manage`；漏一次 `cargo check` 抓不住、运行时第一次调用才 panic。规矩与找全消费者的做法见 INVARIANTS §8 · [CONTRIBUTING.md § 3.1](CONTRIBUTING.md)。

---

## 4. 数据放在哪

monitor 自己的文件在 `~/.cc-monitor/`：

| 路径 | 写入方 | 读取方 | 用途 |
|---|---|---|---|
| `config.json` | monitor 设置 | monitor | 主题 · 字体 · 行为开关 · 机器表 · 诊断 |
| `ps-await/<PID>.tty` | Linux bash / zsh 接入块（本机桌面上开的 shell） | monitor `bind.rs` | 那个 shell 的进程号 · 起始时刻 · 终端设备，monitor 据它认窗口（认上 / 认不出就删） |
| `ps-registry/<PID>.json` | monitor（Linux） | monitor（↗ 沿进程链时先查它） | 那个 shell 显示在哪个窗口（与 shell 进程同寿） |
| `auto-launch.json` | monitor 设置 | PowerShell 接入块（`__ccm_bind`） | 「用 `cc` 起 claude 时自动开 monitor」开关 ＋ monitor 路径 |
| `history-metadata.json` | 本机后端 | 本机后端 | 历史注解（星标 · 改名 · 隐藏 · 上次账号） |
| `logs/monitor/` | monitor | 用户 | 按天滚动的诊断日志 |

后端自己的状态（`backend.json` · 中转根钥匙 `relay-key` · 资产目录 · skill 装记录 · API 号凭据 · 后端日志 · `bin/`）由那台后端写，写者逐文件登记在 `readonly_guard.rs` 的 `OWN_STATE_WRITERS`。

只读的外部数据源：`<claude_dir>/projects/**/*.jsonl`（会话内容）· `<claude_dir>/sessions/<PID>.json`（活跃会话，PID ＋ `procStart` 双校验）· `<claude_dir>/tasks/<sid>/`（Task 面板），都由那台后端读。

字段定义、编码约束（UTF-8 无 BOM）、原子性与切到终端那一套 → [IPC-PROTOCOL.md](IPC-PROTOCOL.md)。每个文件的完整路径在设置的「数据位置」页。

---

## 5. 关键设计选择与理由

每条都是「为什么不能用别的方案」。规矩本身由判据钉着的那几条只写在 INVARIANTS，这里不重复：顺序靠 `seq` 不靠后端保序（§5 · §9）· 判活要 PID ＋ `procStart`（§6）· 拉前三重校验（§7）· 长耗时同步调用走 `spawn_blocking`（§10）· 浮层真挂 `document.body`（§13）· 启动重放贴底不抖（§21）。

### 零侵入：只读 Claude Code 的数据源

后端只读 `projects/` 与 `sessions/`。写入一律是用户显式触发，而且只经那台后端的文件管理面：删历史会话只收 sid（`files-delete-session`）；从某一轮分叉只新增一份 `<new-sid>.jsonl`，`O_EXCL` 新建、绝不覆盖，原会话零改动（`control/fork_write`）。按 sid 找那份会话文件只有一份实现，两处共用。装别名块、skill、MCP 是用户点名的写，别名块只动 BEGIN / END 块内。

**为什么**：用户对「数据源就是我自己的命令痕迹」的认知不能破；写是必要时的可选副作用，就得是显式的、可见的（足迹页逐条列出）。

### 起子进程只有一个出口

monitor 起子进程一律经 `spawn_managed.rs`，三个策略都是必填参数、都没有 `Default`：控制台（`ConsolePolicy`：`Hidden` · `NewVisible` · `Inherit`）· 生命周期（`Lifetime`：`JobKillOnClose` · `Detached`）· stderr 去向（`StderrSink`：`ToLog` · `Null` · `Inherit` · `Captured`）。还自造 `Command` 的地方逐处登记。给用户开真终端那一处刻意是 `NewVisible`。

**为什么**：Windows 上 GUI 进程起一个控制台程序而不给 flag，系统会新配一个带窗口的控制台——用户看到就会关，关掉就杀死子进程（退出码 `0xC000013A`）。分进程之后，原来免费的东西都要显式管：谁杀谁、控制台策略、错误怎么跨进程传、两边对版、起不起得来。三个必填参数让坏默认值无法被表达；子进程的 stderr 接进 monitor 日志，死亡码说人话（「被控制台事件杀死」而不是裸退出码）。

### 成批交付

重放按块投递，一次 Tauri 事件里一串格，前端推进同一条队列走批量调度。

**为什么**：每次 emit 都有序列化与派发开销，逐条交付几千行时主线程累计阻塞明显，启动可见卡顿。

### 视口外不渲染：content-visibility ＋ 估高 ＋ 骨架

- 顶层卡片带 `content-visibility: auto`，视口外与隐藏 tab 的卡片跳过布局与绘制；`contain-intrinsic-size` 初值由 `height-estimate.ts` 按块类型估（正文用 pretext 测宽、代码块按行数），估值只是初值。估高路径绝不许抛，失败就降级。
- 拿得到会话索引的 tab 与历史查看器接上 `SkeletonView`（`skeleton-view.ts`）：没物化的 seq 区间由占位顶住，一接上滚动条就是全会话的；每次滚动只物化与视口相交的那一段，正文按偏移向那台后端取回。
- 没接骨架的 tab 由 `TailWindow`（`live-window.ts`）收纳：只物化尾部，更老的记录只进账本不建卡，滚到顶部附近再按批补。

**为什么**：建卡（markdown · DOMPurify · 高亮）是重放期最大的成本；不建看不见的卡，上万条记录的会话也能秒开。

### 每个窗口一个入口

`index.html` · `settings.html` · `viewer.html` 各有自己的入口（`entry-main.ts` · `entry-settings.ts` · `entry-viewer.ts`）。设置窗的模块图里没有高亮、数学排版与 tab 管理；设置窗关窗是隐藏、复用时重跑取值，主窗销毁时连带销毁它（`lib.rs::windows_to_destroy_after`）。独立只读窗口复用 `TabManager` 过滤到那个会话，订一条 `session-lines/<sid>`，不发 `frontend-ready`。

**为什么**：webview 之间的 JS realm 互不共享，一个入口顶层同步 import 了全部视图，运行期分派到哪一支都要把整张模块图执行一遍——修法只有多入口。viewer 不另写渲染器：再写一套就会与主管线漂移。

### 账号：隔离又同步

一个账号 ＝ 一个 `CLAUDE_CONFIG_DIR`：各自一份登录凭据，两个号可以同时跑、互不踢；skill、记忆、历史、设置经符号链接共享同一份（由后端 `accounts/manage/` 建立和维护）。账号分订阅号与 API 号；API 号的 key 只留在它所在的那台机器，经那台的中转注入，不进命令行、不进环境变量。界面只交意图：建库 · 添加 · 删除 · 设默认 · 修复 · 回滚由后端执行，每次改动前先备份；订阅号的登录走真实终端窗口，由 claude 自己完成。

换号重启直接结束旧会话再 resume 同一个 sid；compact 失败不阻断，kill 失败必须中止——绝不在旧进程还活着时续 resume，否则新旧两个进程抢同一份会话。选不了原账号时不静默换号，拒绝并给出「用当前账号」的显式选择。

### 按控制台认窗口（Windows）

点 ↗ 时沿进程链每一级借它的控制台问一句：`AttachConsole` → `GetConsoleWindow` → 那个窗口的属主（`platform/console.rs::console_window`，壳里借控制台只此一处）。
Windows Terminal 里每个标签的伪控制台窗口的属主就是承载它的那个终端窗口（拖到别的窗口时由 Windows Terminal 改挂）；经典控制台就是控制台窗口自己；看不见的不算。

**为什么不按进程找窗口**：Windows Terminal 一个进程开几个窗口，按进程只分得出「这个程序有两个窗口」。**为什么不靠接入块登记**：登记要用户装 PowerShell 接入块、
还要那个标签当时在前台、标题没被别人改，cc-monitor 自己从标签栏起的会话（命令里 ssh 用全路径，不带窗口标签）一样落空。控制台的属主由 Windows Terminal 自己维护，
谁起的、装没装接入块都一样认得准。只认到窗口：Windows Terminal 没有从外部选中别人标签的接口。

### 日志：tracing 在 Builder 之前初始化

`logging::init` 必须在 `tauri::Builder::default()` 之前调用（全局 dispatcher 只能装一次）：文件层按天滚动、非阻塞写；`EnvFilter` 可热改级别，不重启就生效；日志行不上屏：要让用户知道的出错只走 `ui_error::tell`（码 ＋ 文案键 ＋ 那句话 ＋ 复制详情，事件 `monitor-error`，出口由 `logging.rs::install_error_emitter` 装上，限流防风暴），前端照那一句弹提示；其余 ERROR 只进日志。日志目录建不出来就退化成只写 stdout，monitor 照样起。后端子进程的 stderr 接进 monitor 的滚动日志，脱离起的后端写自己那份日志。

**为什么**：release 版是 GUI 子系统，没有 stderr；日志不落盘，一条解析失败的 warn 就没人看见。

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
- **从 claude 的祖先链猜终端窗口**：explorer 起 PowerShell ＋ WT 接管控制台的常见架构下，claude 的祖先链与 WT 窗口完全脱节，启发式在主流环境下都不可靠。本机改为点 ↗ 那一刻沿 agent 进程往上，每一级问它的控制台显示在哪个窗口（Windows）。远端会话从此刻连着它的终端出发：那台读出那个终端的 `SSH_CONNECTION`（在 tmux 里就问 tmux 此刻哪些客户端连着），本机后端按这四元组在连接表里找到这台电脑上开着那条连接的进程，往上数父进程到 `explorer.exe` 为止，monitor 沿链每一级先问它的控制台显示在哪个窗口，再取第一个有可见窗口的进程。链从连接的拥有者起、不从 claude 起；找不到就照实说原因（在 tmux 后台没人连着 · 不在这台电脑上 · 经跳板或端口转换对不上 · 没有窗口或不止一个）。
- **换掉 webview**：这个 app 的核心是渲染会话记录（Markdown · 代码高亮 · LaTeX · 可折叠工具卡 · 流式追加 · 上万条记录的虚拟化），正是 HTML 最擅长、原生 GUI 工具箱最不擅长的那一类。文件管理器窗口不渲染会话记录，所以它是原生（egui）的。

---

## 7. 入门读图

- 整体数据流：本篇 §1 ＋ §5
- 一件事该放在哪一层：本篇 §2
- 加 jsonl 记录类型、加帧命令、加设置项 / 快捷键：[CONTRIBUTING.md](CONTRIBUTING.md)
- 改跨进程文件或帧协议：[IPC-PROTOCOL.md](IPC-PROTOCOL.md)
- 改某个具体模块：对应目录的 README

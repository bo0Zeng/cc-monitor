# 架构总览

cc-monitor 是 AI 编程 agent（Claude Code · Codex）会话的**观察者和启动器**：agent 跑在用户自己的终端里，cc-monitor 读它写下的会话记录来显示，替用户起会话、管机器，不接管 agent 本身。

本篇回答：由哪几个进程组成、一件事住在哪一层、各子系统谁调谁、为什么这么分。只写现在是什么样。新贡献者先读这一篇。

相关文档：

- 全局不变量 → [INVARIANTS.md](INVARIANTS.md)
- 帧协议、一次性命令与跨进程文件 → [IPC-PROTOCOL.md](IPC-PROTOCOL.md)（总述）· [IPC-COMMANDS.md](IPC-COMMANDS.md)（逐格，从代码生成）
- 加东西 / 撤东西的做法 → [CONTRIBUTING.md](CONTRIBUTING.md)
- 逐文件清单 → 界面 [`src/README.md`](../README.md) · 壳 [`src/frontend/shell/README.md`](../frontend/shell/README.md) · 后端 [`src/backend/README.md`](../backend/README.md) · 手机 [`src/mobile/README.md`](../mobile/README.md)

---

## 0. 一个核心，两层适配

```
 输入适配层（每家 agent 一个）       核心（后端，一处）                    输出适配层（每个出口一个）
 claudecode · codex · fake    ──▶  通用会话 / 记录模型              ──▶  桌面界面：帧 → 壳（只转运）→ webview（只排版）
 盘上原始格式 ⇒ 通用模型             一切判定、一切成品只算一次           壳的原生窗：文件窗口（egui，一窗一进程）
                                    成品 ＝ 值 ＋ 写好的字 ＋ 语气         手机（src/mobile）：经 SSH 连常驻后端
                                                                        CLI：ccm -- --子命令（--json / --text）
                                                                        对端后端：扇出时本机问远端
```

- **核心**＝后端（`src/backend/`）里「判定与写字」的那一处：读会话、判活、划段、挑号、渲起会话那一行、写用户文件、出每件成品的全部格。
- **输入适配层**＝`src/backend/agents/<家>/`：把那一家的盘上格式翻成通用模型（`agents/record.rs::Record` · 会话活动 · 步骤结果的数），申报那一家用了哪些文件；只给码 ＋ 原话，句子由核心写。
- **输出适配层**＝每个出口：挑格、按核心算好的格筛 / 排、按预算截、装运。不判定、不写句子、不格式化（时长 · 大小 · 百分比 · 按码取字）。

能力全进核心，实现只一处：一个出口要改形，只要核心里已有那一格，就只改出口自己；核心里没有，就动核心一次，补给所有出口（判法见 §2.9）。

---

## 1. 进程与数据流

### 1.1 进程

| 可执行文件 | 是什么 | 谁起它 |
|---|---|---|
| `cc-monitor`（壳的包名 `monitor`） | 桌面界面进程：webview 窗口 ＋ 通信层面 A 的客户端。零 SSH，不写用户文件 | 用户 |
| `ccm`（开发树里叫 `cc-monitor-backend`） | 唯一的后端。每台机器上 `~/.cc-monitor/bin/ccm` 就是那台的后端，也是用户敲的 agent 的壳（argv 打头是 `--` 加后端词才进后端，其余交给 agent） | 本机：monitor 连上来时起；远端：monitor 接那台时经一次 exec `--resident-ensure` 起（已在跑就用那一个）；手机：同一条 exec |
| `cc-monitor-filewin` | 文件窗口，壳的第二个二进制，独立前端，一个窗口一个进程 | monitor（父子管道：通道就是窗口进程的 stdin / stdout，不监听） |
| 手机 app（`com.ccmonitor.mobile`） | Android 前端，Kotlin | 用户 |

agent 不是我们起的进程：后端渲好一行 `ccm …`，交给用户的终端去 exec（§3.5）。

```
本机                                                          远端（每台）
┌ cc-monitor（界面，零 SSH）──┐   父子管道   ┌ 文件窗口 ×N ┐
│ webview 窗口 · 通道客户端   │◀─────────────▶│ call /      │
└──────┬─────────────────────┘               │ subscribe   │
       │ 本人 Unix 套接字 / 管道              └─────────────┘
┌──────▼──────────────────────────┐  SSH（链路 · exec · SFTP）  ┌ 常驻后端（与本机同形）┐
│ 本机常驻后端                      │ ──────────────────────────▶│ ＋ 中转（进程内）      │
│ 持有到各远端的全部 SSH（含 SFTP） │                            └──────────▲────────────┘
│ 中转 ＋ 上游选择                  │ ◀── agent 经中转口连进来               │ SSH（手机自己那条）
└─────────────────────────────────┘                             ┌─────────┴────────┐
                                                                 │ 手机 app          │
                                                                 └──────────────────┘
```

- 本机固定两个进程：monitor ＋ 本机常驻后端。Linux 上后端脱离起，只听家里的本人 Unix 套接字 `<家>/run/backend.sock`（`run/` 只给本人、收连接时核对端 uid、目录独占锁；没有钥匙），monitor 接上它；monitor 关了，后端按那台机器的「退出行为」留下或退出，下次 monitor 起来接回。Windows 上后端是 monitor 监护的子进程（管道），随 monitor 退出。
- 远端每台一个常驻后端，与本机同形。本机后端在那条 SSH 连接上开一条链路，在那台跑 `ccm -- --resident-attach` 小中继去连那台的套接字、原样对拷；不另开任何口。那台后端比 monitor 旧就换一份。远端只支持 Linux（x86_64 / aarch64），别的系统连上时显式拒绝。
- 手机走同一个门：自己拨 SSH，`--backend-probe` 核 `BUILD_ID` → `--resident-ensure` → `--resident-attach` 一条长连接（§3.9）。
- 门与载体的细则（attach 行、拒绝理由）在 [IPC-PROTOCOL.md](IPC-PROTOCOL.md) §1；为什么没有钥匙在 INVARIANTS §48.1。
- 一个常驻后端可以同时接多个客户（桌面 · 手机 · 小中继），连接数归零那一刻按「退出行为」当场决定退不退。

### 1.2 会话内容流：本机与远端同一条帧路

```
   <agent 家>/projects/<编码后的 cwd>/<sid>.jsonl        （agent 写）
                 │  那台后端的 observe/watcher 盯文件，逐行出 line 帧
                 ▼
   本机：本人套接字 / 管道  ·  远端：本机后端持有的那条 SSH 长连接上的链路
                 ▼
   monitor stream_source/batch.rs 的 LineIntake：攒批 · 静默窗 · 续点
   （本机那条经 local_lines.rs 进同一个收口）
                 ▼
   event_replay.rs：重放缓冲（每个会话只留尾巴）· 大小分流 · 就绪点
                 ▼  通道订阅 session-lines，按 credit 交格（窗口作用域事件 chan-items）
   界面 events.ts → tabs.ts（TabManager）→ render-stream-record.ts
                → record-timeline.ts（按 seq 插入）→ stream.ts → render.ts → DOM
```

- **同一个收口**：远端那台后端的 `line` 帧沿 SSH 长连接回来，交给 `stream_source/batch.rs` 的 `LineIntake`；本机后端的帧走管道，进的是同一个收口。换的是源，不是管线。
- **流只发要看的会话**：每条长连接经 `stream-watch {sids}` 报「在看哪几个会话」（壳 `stream_watch.rs`，名单 ＝ 主窗口在看的 tab ∪ 有 `session-lines/<sid>` 订阅的会话）；刚进名单的会话从第几行起上流由那台回，界面据它补上没上流的那段。手机按同一条命令报它屏上的会话。
- **seq 是后端给的行号**，界面 `RecordTimeline` 按 seq 二分插入，后端交格的先后不影响画面。一条记录至多一张卡：纯工具的记录画成一张「几步」卡（每步一行），界面不把相邻的并组 —— 哪几条收进「过程」只由后端划段（`observe/turns.rs`）。
- **起停与状态也在这条流里**：会话账本整本在后端（`observe/session_ledger`），帧 `session_added` · `session_state` · `sessions_replayed` 随 `session-lines` 一起来，起停帧不吃 credit、不许丢。monitor 只留一份「那台说过的成品」缓存（`session_book.rs`），它自己唯一知道的事实是「到那台的连接断了」，那时那台的成品作废、界面说「说不清」。
- **背压**：界面给 credit；实时行没有 credit 就丢，并在原位报 gap，界面按行号向那台后端补（帧命令 `history-lines`）。
- **大小分流**（`event_replay.rs::on_line_batch_awaited`）：小批逐行一格；大批（`--resume` 灌历史、重放）按 `CHUNK_SIZE = 600` 切块、末块先发，每块带 batch 边界，界面进 batch 模式（代码高亮延后）。
- **启动序**：主窗口先经通道订好每台机器的会话流，再发 `frontend-ready`（带优先会话）——那就是这些订阅的就绪点。monitor 在 `event_replay.rs::ready_point` 里按会话分组切块、优先会话先交，并按活跃集补发 `session-ended`，归档落在全部重放行之后（INVARIANTS §24）。本机活会话的骨架 tab 也由这条流的起停帧给出。
- **冷读也问那台后端**：历史清单、整页正文、按偏移读、按行号读、子 agent、全文搜索都是帧命令（`history-*`）。记录解释（一行 jsonl → 通用记录，各家的翻译表在 `agents/<名>/`）只在后端，界面按 `t` 与格排版；主线外清单（ESC 回退掉的那几条）也是后端给的成品（实时帧 `session_branch` · 冷读 `history-branch`）；多台的搜索结果由本机后端合并排序。
- **「X 变了 ⇒ 重读」只一种帧** `changed {topic, key?, rev?, body?}`：主题表（名字 · 可不可丢 · 带不带小成品、上限）只住后端 `stream/topic.rs`，协议见 IPC-PROTOCOL「`changed` 主题表」。壳不认主题，按 `topic` 原样扇给订了 `changed/<topic>` 的界面订阅；界面按生成的 `Topic` 订、读，带了 `body` 就不重问。
- **Task 面板**：那台后端盯 `<agent 家>/tasks/`，一批事件按 sid 去重发 `changed {tasks, key: sid}`，界面订 `changed/tasks`，收到就重问 `tasks-list`。

### 1.3 每条线的源头

按「源头是谁」切，是不会切错的那种切法：

| 线 | 源头 |
|---|---|
| 会话内容 · 历史 | `<agent 家>/projects/**/*.jsonl`（冷读与实时读同一份；Codex 是它自己的 rollout 文件，由适配层申报） |
| 判活 | `<agent 家>/sessions/<PID>.json` ＋ tmux 会话上的 `@ccm_sid` |
| 用量 | 中转经手的回包头 ＋ 官方客户端报的用量（`quota-probe`），都记进额度账 |
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
- webview 那一侧的通道客户端是 `chan.ts`（通信层面 A 成员）；握手时那台在 hello 里交出它接受的命令（`commands`）与这台做不到的（`unavailable`），做不到的命令界面事前置灰。
- Tauri 命令只剩「monitor 自己的事」：窗口 · 拉前 · 本机 monitor 配置 · 日志与数据位置 · 重放缓冲 · 本机后端起停与引导 · 通道本身 · 放字节 · 足迹里 monitor 自己的事实。它们逐条登记在 `command_home_registry`，碰后端的新 Tauri 命令进不了这张表；全仓只有 `src/frontend/ui/ipc/commands.ts` 直接调 `invoke`。

---

## 2. 层边界

### 2.1 三层

- **前端**：把后端给的东西排版成像素、收手势，并在用户面前开终端窗口。不做判定、不拼命令串、不缓存业务数据。桌面界面状态只有一份 pub-sub：`app-store.ts` 的 `Slice`（当前机器 · 账号快照 · 当前 tab），tab 那一层是 `tab-store.ts` ＋ `tab-router.ts`，overlay 视图的开关收在 `overlay-router.ts`。
- **通信层**：搬字节，零业务判断。面 A 是前端 ↔ 各处后端（按 `origin` 寻址，住 `src/comms/inward/`）；面 B 是 agent ↔ 上游 API，也就是中转（按路径前缀寻址，住 `src/comms/outward/`）。它不读盘、不起进程、不绑端口，凭据与端口由后端交给它。
- **后端**：其余一切干活的——读会话、起进程、动 tmux、拨 SSH、管资产、管账号、写用户的文件。

### 2.2 后端：一份代码、两种承载

后端是一份代码、两种承载：本机常驻后端与每台远端的后端是同一个二进制，本机与远端的唯一差别是通道（管道 / SSH）。本机就是不走 ssh 的远端。

进后端的口有两个、平级：帧面（`stream/`，monitor · 手机 · 外部前端经通信层面 A 走这条）与 CLI 面（`main.rs` 的一次性分派，用户与脚本敲 `ccm` 走这条；帧命令派生出同名的 CLI 子命令，§3.2）。argv 只在 `main.rs` 里取一次。

模块地图（`src/backend/`，逐文件清单见它的 README，每个目录的 `mod.rs` 头注是那一块的说明）：

- `platform/`：平台原语与平台 cfg 的唯一住处（路径 · 进程 · 判活的读法 · 信号 · 文件原语 · 脱离）；`platform/shell/` 是 shell 方言知识的唯一住处（引号 · 定义函数 · 导出环境 · rc / `$PROFILE` 在哪）。
- `observe/`：产出观测帧的读（会话文件 watcher · 会话账本 · 划段 · tmux 观测 · 历史 / 搜索 / 任务 / 账号查询）。
- `control/`：改状态的动作，以及只喂控制决策的只读查询（起会话与命令渲染 · kill · 送键 · 抓屏 · 分叉落盘 · 常驻与退出行为 · 部署计划 · 写用户文件的那一族 · 传输台）。
- `files/`：文件管理的读面（常驻文件名索引 · 列 · 读 · 按内容搜）。
- `accounts/`：账号域。账号库管理（`manage/`）· 登录令牌（`oauth/`）· 额度账与轮换（`quota/`）· 上游选择（`upstream_select/`：这一发走哪个上游、注入什么凭据、钉在哪个号）。
- `agents/`：用户的 AI CLI 的适配面（claudecode · codex · 测试用的 fake），一家一行注册表，含记录解释与用量读法。
- `plugin/`：调外部程序的口（找 · 问 · 起 · 收），cc-bus 那一族命令经它起。
- `relay/`：中转的宿主（门 · 监听 · 根钥匙）；中转本体是通信层面 B。
- `dial/`：SSH（连接池 · 链路 · SFTP · 问远端后端 · `~/.ssh/config` 解读 · 端口转发 · 开终端那一行）。
- `assets/`：skill / MCP / 别名的计算、判定与写，资产目录与同步，两台之间的装由本机后端当枢纽。
- `history/`：历史跨机 join 与注解（星标 / 改名 / 隐藏 / 上次账号）。
- `plan/`：计划页的读面（这台机器上的计划工作区、每片的图与状态）。
- `faces/`：只读面与功能侧的帧面宿主（本体在 `observe/` / `control/` / `accounts/`），格目录与通用投影也在这里；`stream/`：帧定义 · 入方向命令信封与命令表 · 监听（本机后端问远端后端的唯一一处住 `dial/remote_ask.rs`）。
- `footprint/`：足迹（这台机器上 cc-monitor 写过什么、能不能撤）；`common/`：共用判定与成品的格类型；`lineage.rs`：会话血缘（谁起的谁）。

后端分两块、零互相依赖：**原生后端**（会话 · tmux · 账号 · SSH 与两台之间的传输 · 资产 · 历史 · 中转宿主）与**文件管理后端**（只管本机的读和写：列 · 读 · 搜 · 写用户的文件 · 上传块 · 解压 · 提交，即 `files/` 与 `control/files_write.rs` · `files_upload_chunks.rs` · `files_extract.rs` · `files_commit.rs`），两块只共用 `platform/` · `common/` 与 `lib.rs` 的能力声明轴；原生那一块够到文件管理那一块只经帧面分派（命令表文件管理那一族）。传输台 `control/transfer.rs` 归原生那一块（拨号拿会话、搬字节），它要的路径解析 · 暂存件命名 · 摘要住 `common/`。判据 `tests/backend/files/module_boundary_guard.rs`。

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
- 壳自己的 `platform/` 住壳要的平台原语：文件原语（不覆盖改名 · 置可执行位 · 只给本人的目录）、控制台输出的解码（Windows 按那台的 OEM 代码页）、窗口与单实例。

monitor 里仍直读本机 agent 目录的地方逐处登记，条数以 `local_read_surface_registry` 的机检为准。

Tauri State 都在 `lib.rs` 的 `setup()` 里 `app.manage`；漏一次 `cargo check` 抓不住、运行时第一次调用才 panic。规矩与找全消费者的做法见 INVARIANTS §8 · [CONTRIBUTING.md § 3.1](CONTRIBUTING.md)。

### 2.5 `platform/`：判据是跨 target 编译

`platform/` 是唯一允许平台原语与平台 cfg 的地方。判据不是「cfg 出现在哪」，是跨 target 编译：后端 CI 跑 `cargo check --all-targets --target x86_64-pc-windows-msvc`，本地门禁的 `winchk` / `winchk-backend` / `winlink` 三格在 Linux 上对 Windows target 编一遍；`tests/backend/platform/cfgless_guard.rs` 补它的盲区（不带 cfg 的平台代码）。

「怎么读到这个事实」下沉 `platform/`（Linux 读 `/proc`，Windows 读 Win32），「什么叫同一个活进程」留通用层、两平台共用一张判定（`liveness::is_same_live_process`）。连规则也下沉，就成了每个平台一套规则。

### 2.6 零轮询：一律事件驱动，四张账本

零轮询由四张登记表覆盖，口径是「每一处周期唤醒都说清事件源在哪、谁退役它」：

| 账本 | 管哪一块 |
|---|---|
| `no_timer_guard`（后端） | 后端里不许有自己醒过来的构件，零容忍 |
| `polling_registry` | 界面 TS 与 `src/shared/` 下的 shell |
| `rust_timer_registry` 的 `REGISTERED` | monitor Rust 里的 `sleep` / `interval` |
| `rust_timer_registry` 的 `SHELL_WAKES` | monitor Rust 拼出来的 shell 循环（前三张都看不见它） |

后端也不产出自带节拍的 shell 串（`no_timer_guard` 的 `f09_external_beat` 零命中）。期限一律由发起那件事的一方给一个绝对时刻，下游只收紧，通信层自己没有期限常量。

### 2.7 共享 crate 与 workspace

`src/common/` 住 monitor 与后端共同 link 的共享 crate。它们只放两边必须对上的**契约**（路径 · 端口 · 文件格式 · 文案表 · 令牌形状 · 字节表键），不放**判定**；判定只在后端。monitor 生产段只许依赖契约类 crate，由 `tests/frontend/shell/contract_crate_guard_tests.rs` 两向钉住。共享常量让两侧「想不一致」得先把 import 删掉，漂移变成不可表示。

后端 crate 刻意不是 workspace 成员：它要能在目标机上原生构建（发版交叉编成 musl 静态二进制）。壳的 workspace 成员是 `monitor` ＋ 共享 crate ＋ 通信层 ＋ 文件窗口包，个数以 `src/frontend/shell/Cargo.toml` 现量为准。

代价：在 `src/frontend/shell` 里跑 `cargo test --workspace` / `cargo fmt --all` 覆不到后端 ⇒ 测试、格式、clippy 要两处分别跑（壳的 workspace · `src/backend`），门禁的 `cargo` · `backend` 两格就是这两处。

### 2.8 其余几块

- **部署**：后端的字节随 monitor 内嵌，全仓只有壳的字节表一个取字节口，按目标机器的 (OS, arch) 选，没覆盖的格子写第一个字节前拒绝。「换不换、换成什么」由后端帧命令 `deploy-plan` 出计划（只升不降，身份读字节里的戳、不跑它），monitor 只按计划放字节。手机今天只核 `BUILD_ID`、对不上就说那台要更新，不自己装后端。
- **资产**：每台后端一份资产目录，本机后端按事件在各后端之间拉 / 合 / 推；装到别的机器要用户点，先看差异。
- **退出行为**：住那台机器自己的 `~/.cc-monitor/backend.json`，只有那台后端读写（`control/exit_policy`），决定那一刻现读。

### 2.9 核心与适配层：成品 · 格目录

后端里「判定与写字」只有一处，叫**核心**；它两边各是一层适配（§0）。手机、CLI、桌面界面、对端后端都是出口；任何出口要改形，只要核心里已经有那一格，就只改出口自己。

| 层 | 做 | 不做 |
|---|---|---|
| 输入适配层（`agents/<家>/`） | 把那一家的盘上格式翻成通用模型（`agents/record.rs::Record` · 会话活动 · 步骤结果的数）；申报那一家用了哪些文件 | 判定、写给人看的句子（给码 ＋ 原话，句子由核心写） |
| 核心（`observe/` · `control/` · `accounts/` … 与出成品的 `faces/`） | 一切判定；每件成品出全量格：值 ＋ 写好的字 ＋ 语气 ＋ 时刻字 | 认「是哪个出口在问」；为某一个出口加开关 |
| 输出适配层（壳 ＋ 界面 · 文件窗口 · CLI 面 · 手机端 · 扇出那一处） | 挑格（要哪几格）、按核心算好的格筛 / 排、按预算截、装运（帧 / 一行 JSON / 文本） | 判定、写句子、格式化（时长 · 大小 · 百分比 · 按码取字） |

**成品约定**（每件成品都是这几种格的组合，出口不需要认业务）：

| 格 | 是什么 | Rust 类型 |
|---|---|---|
| 值 | 机器要的码、数、时刻（epoch / ISO 原样）、id | 普通类型；闭集的词是枚举的单元变体 |
| 写好的字（`text` · `…Text`） | 核心按文案表与看的那一台的语言、时区写好的一句 | `common::cells::Words` |
| 语气（`tone` · `…Tone`） | 闭集：`plain` · `fail` · `now` · `need` · `warn`（该留意了，还没出错：上下文快满）· `busy`（后台有事在跑、不要人）；要用到再加 `ok` | `common::cells::Tone` |
| 活钟 `{dur}` | 要随时间走的那一截：`{text: "…{dur}…", from, to}`，出口用登记过的那一个读口填 | 过程行 `span` |
| `detail` · `raw` | 复制详情（只在失败时）· 原文透传（受认证约束的可选格） | — |

「这一格是值还是写好的字」由它的 Rust 类型说，不按格名猜：`blocks[].text` 是原文（值），`cost.text` 是核心写的字。界面自己的骨架字（按钮、菜单、标题、空状态）留在界面、也从文案表取；凡是描述数据或状态的字都由核心写。

**出口扫描判据**：`tests/frontend/ui/exit-judgment-scan.vitest.ts` 按写法指纹（`tests/frontend/ui/exit-rules.ts`：按码取字 · 按状态排序 · 拼百分比 · 写时长单位 · 换算时刻 · 换算大小）扫桌面界面、壳与手机 Kotlin 的生产代码；今天的存量登在欠账名单 `tests/frontend/ui/exit-debt.json`，逐格恒等、只许删不许加（每格写明凭什么留）。

**会走的钟**：核心写的时刻字是发出那一刻的字。手机与 CLI 要它跟着走就按节拍重问（出口零格式化）；桌面秒级走的那几处（过程行 `{dur}` · 「已等」）只留一个时长读口（TS `duration-format.ts::fmtDur`，与 Rust `copy_core::short_duration` 对同一份金样 `short-duration.golden.json`）。

#### 格目录

格目录（本体 `faces/cells_catalog.rs`，落盘一份金样 `tests/__fixtures__/cells-catalog.golden.json`）列出每件成品有哪些格（路径 · `value` / `text` / `tone` · 类型），出口照金样就知道有没有那一格，不必读核心代码；`view` 投影按它认成品名与路径。它不是帧命令（原先那条没有调用方，第 8 批删了），整份目录只在测试档现算。目录不手写：每件成品登记一组用自己的 Rust 类型造的样本，经同一份 `Serialize` 走查出格路径。哪些成品已登记、格的路径写法、`pending`（判了要补、还没落地的格）与冻结规则，以金样与 `tests/backend/faces/cells_catalog_tests.rs` 的头注为准；冻结的格（每格带 `frozen`：`record` · `read_row` 整件，会话三帧 `session_added` · `session_status` · `session_removed` 只冻手机读的那几格）不删、不改名、不换类型。

#### 出口怎么挑格：请求信封里的声明

出口在请求信封里交自己的声明 `view`（帧面那一格 · CLI 面 `--view`，同一份）：今天的词是 `cells`（只要这几格）与 `omit`（这几格不要），路径照格目录写。
核心只一个通用投影（`faces/project.rs::project`）：声明先按格目录校验（认不出的词 · 成品 · 格 ⇒ `bad_args`，不静默放过），每条命令的应答里哪几处住着哪件成品登记一次（`stream/inbound/views.rs::PLACES`），成功的应答照声明裁好再装运；各命令不自己认「要不要正文」一类的开关。
流要什么由 attach 行 / `--view` 交的声明说（`IPC-PROTOCOL.md` §2），没有流旗标。还靠开关定形的：入参 `raw`。CLI 面的 `--text`（给人看那一形）是所有命令通用的一个出法（`control/ship_text.rs` 把核心写好的 `text` · `rows` 拼成字），不按命令写。

#### 一个出口的新需求：只改出口，还是动核心

| 问 | 是 ⇒ | 否 ⇒ |
|---|---|---|
| ① 格目录里已经有这一格（值或写好的字）？ | 下一问 | **动核心**：补这一格，对所有出口一次补齐 |
| ② 只是「要 / 不要某几格」「按已有的格筛 / 排」「换预算」「换装运」？ | **只改出口** | 下一问 |
| ③ 要的是新判定、新句子、新的跨会话 / 跨机器汇总、新的推送时机？ | **动核心**：新判定进核心、新格进目录；之后同类需求回到 ② | — |

格已有 ⇒ 只改出口；格没有或要新判定 ⇒ 动核心一次，补给所有出口。出口永远不自己算一格。动核心的每一刀走全套：命令登记 → `IPC-COMMANDS.md` 重生成 → 冻结表 / 金样同拍 → `BUILD_ID`（§3.8）。

---

## 3. 各子系统：住在哪、谁调谁

### 3.1 会话流

| 段 | 住址 |
|---|---|
| 盯文件 · 出 `line` 帧 | 后端 `observe/watcher.rs`（每条连接一份） |
| 会话账本（活 / 可重连 / 已结束，单写者） | `observe/session_ledger.rs` |
| 记录解释（一行 → 通用记录） | `agents/<家>/`，Claude 的链与主线在 `agents/claudecode/` · `agents/mainline.rs` |
| 划段（一轮里哪几条收进「过程」） | `observe/turns.rs` |
| 子运行（子 agent）表 | `observe/runs.rs`，帧 `session_runs` |
| 帧定义与发帧 | `stream/wire.rs` · `stream/tap.rs` · 活卡 `stream/live.rs` · 主题表 `stream/topic.rs` |
| 收口 · 重放 · 就绪点 | 壳 `stream_source/` · `event_replay.rs` · `session_book.rs` · `stream_watch.rs` |
| 排版 | 界面 `events.ts` → `tabs.ts` → `render-stream-record.ts` → `cards/` |
| 手机 | `core-claude` 的 `link/`（常驻流客户端 · 帧解码）→ `app` 的聊天屏 |

谁调谁：watcher 写帧 → 本机套接字 / 远端链路 → 壳收口 → 通道 → 界面。冷读（`history-*`）是界面经通道直接问那台后端，壳只转运。

### 3.2 命令层

- **命令表**：后端每条入方向命令一行 `CommandSpec`（名字 · 一句说明 · 错误码 · 入参与回包的每一格 · 跑在哪一档），按域分文件住 `src/backend/stream/inbound/registry/`；信封、取消、期限在 `stream/inbound/`。
- **一张表两个面**：帧面（长连接上的 `{op, args}`）与 CLI 面（`ccm -- --<命令>`，入参从 stdin 读一段 JSON 或 `--args-b64`）从同一张表派生；只上帧面的登记为 `STREAM_ONLY`。CLI 面的子命令全表是 `lib.rs::SUBCOMMANDS`，其中不从命令表派生、只在 CLI 面的那几条说明在 `stream/inbound/cli_only.rs`。
- **协议参考从代码生成**：`IPC-COMMANDS.md` 由 `tests/backend/protocol_doc_gen.rs` 从命令表与帧类型写出，判据是「重新生成 == 仓里那份」；改了命令或帧之后在 `src/backend` 下跑 `CCM_REGEN_PROTOCOL_DOC=yes cargo test --lib -- protocol_doc_gen`。
- **握手**：hello 带 `commands`（这台接受的命令）· `unavailable`（接得下、这台做不到的，带码）· `uncancellable`（开跑之后撤不动的）；出口据它置灰，不猜。
- **调用方**：桌面界面经 `chan.call(origin, op, payload)`，回包解码写在各 `*-reads.ts`；手机经 `core-claude` 的 `FrameClient`；脚本与 skill 走 CLI 面。

### 3.3 中转与账号轮换

**中转**（通信层面 B，`src/comms/outward/`）：本机远端同形，是那台常驻后端进程里的一条线程，对外端口由宿主 `relay/` 绑，进门要钥匙。凡经 `ccm` 起的会话都注入中转地址（`ccm` 在最终 exec 那一处问上游选择）；用户自己设了 `ANTHROPIC_BASE_URL` 时不注入并说一句。中转只切流：路由键是两段不透明的路径前缀（agent · 账号），读成「哪一家、哪个号」、这一发走哪个上游、注入什么凭据都是上游选择（`accounts/upstream_select/`）的事；表里没有那一行就拒，不回落到任何一家。monitor 对凭据文件零读零写。

**账号**：一个账号 ＝ 一个配置目录（`CLAUDE_CONFIG_DIR`），账号库由 `accounts/manage/` 建立和维护；订阅号的登录令牌由 `accounts/oauth/` 读与续；API 号的 key 只在它所在那台的凭据表里（§5「账号：隔离又同步」）。

**额度与轮换（今天的实现）**：

| 块 | 住址 | 做什么 |
|---|---|---|
| 额度账 | `accounts/quota/ledger.rs`（`~/.cc-monitor/quota.json`） | 每号每窗口的用量、重置时刻、被拒 · 超额。两个来源：中转经手的回包头（读法在适配层 `agents::quota_read_of`）· `quota-probe` 起官方客户端报的用量（`agents::usage_of`） |
| 轮换存储 | `accounts/quota/rotation.rs`（`~/.cc-monitor/rotation.json` 第三版）· `accounts/quota/rules/shelf.rs`（`~/.cc-monitor/strategies/`） | 每个会话按会话 id 存：用哪份策略（空 ⇒ 不轮换）· 改过的设置 · 拖出来的顺序 · 策略约的叫醒 · 换号记录；新会话用哪份（`forNew`，默认空）。策略文件都是用户自己的，没有随包策略 |
| 事实 | `accounts/quota/facts.rs`（拼装在 `upstream_select/rotate.rs` 的 `Hop::facts`） | 每个号的用量 · 重置 · 用速 · 被拒 · 超额 · 登录；只读额度账 ＋ 账号库 ＋ 登录，换一份策略不变 |
| 节点 | `accounts/quota/nodes.rs` | 事实或设置变了的那一刻（读数 · 被拒 · 续期失败 · `quota.json` · 账号库 · 策略目录 · 会话那一条）；全进程一条通道，判的缓存与推送只订它。重置这类已知时刻不走它，判的那一侧记成下一个节点时刻 |
| 判 | `accounts/quota/rules/decide.rs`（`turn`，纯函数）· `rules/standing.rs` · 策略文件里的 `decide(ctx)` · 积木库 `src/shared/rotation/lib.rhai` | 「换不换、换谁」全在策略里；后端只照策略交的先后取第一个事实上发得出去的号（`standing`）、停发、兜底 |
| 推演 | `accounts/quota/rules/simulate.rs` | 离散事件推演，每个事件点调的是同一个 `turn`；每号照自己量出来的用速涨，量不到不编数 |
| 策略文件 | `accounts/quota/rules/shelf.rs`（目录）· `rules/script.rs`（JSON 头 ＋ Rhai 代码）· 沙箱 `accounts/quota/strategy/` | 一份策略一个 `.rhai`，文件名即 id；没有随包策略；第三版的旧脚本升级时挪进 `strategies/v3/`、不加载 |
| 换 | `accounts/upstream_select/turn.rs` · `rotate.rs` | 中转那一发里只在节点上判（结论缓存到下一个节点）、备料（订阅号：鉴权头换成那个号的令牌，请求体里的身份格一起换；按量号：照它在 key 表里那一行）、记一条。不重启进程；要重启换就走 `session-restart` |
| 血缘 | `src/backend/lineage.rs`（`~/.cc-monitor/lineage.json`） | 谁起的谁，只在中转那一路认 |
| 帧面 | `faces/rotation_face.rs` · `faces/rotation_view_face.rs` · `faces/rotation_edit_face.rs` · `faces/rotation_try_face.rs` · `faces/quota_rows.rs` | `quota-read` · `rotation-*`（策略目录 · 一个会话或一份策略的四块 `rotation-view` · 设置 · 拖出来的顺序 · 现在用 · 试跑）；quota-warm 读的「该不该发一句」也在 `quota_rows.rs` 判 |

谁调谁：agent → 中转 → 上游选择（问轮换、拿令牌）→ 上游；回包头 → 额度账；界面 / 手机 / skill 经 `quota-read` · `rotation-*` 读写，不自己判。

**去向**：后端已是第五版的分层（上表：事实 · 节点 · 判 · 推演 · 策略文件 · `rotation.json` 第三版，第一版 / 第二版读到时一次升到第三版写回、原文留 `.v1` / `.v2`）。还没进主线的是界面那一刀：设置页与会话面板照第五版的 `rotation-*` 命令读写。

### 3.4 文件

- **窗口**：`cc-monitor-filewin`（独立包 `src/frontend/filewin/`，egui，一窗一进程）经父子管道只说 `call` / `subscribe`；开窗那一侧在壳 `filewin/`，契约在共享 crate `src/common/filewin-contract/`。配色读同一份界面令牌。
- **读**：后端 `files/`（常驻文件名索引 · 列目录 · 元数据 · 按块读 · 按内容搜）。
- **写**：只经那台后端的文件管理面 `control/files_write.rs`（`files-peek` · `files-put` 带期望值 · `files-delete` …，规矩见 INVARIANTS §4），本机远端同一条路；上传块 · 解压 · 提交在 `control/files_upload_chunks.rs` · `control/files_extract.rs` · `control/files_commit.rs`。monitor 碰用户文件只有这一个开口，只差 `origin`。
- **传输**：两台之间搬字节归原生那一块的传输台 `control/transfer.rs`（经 `dial/` 的 SFTP 写暂存区，远端那台的 `files-commit-upload` 提交）；下载的本机落点也由它写（过同一套路径解析，经帧面分派交来的写句柄）。
- **手机**：文件屏（`app` 的 `ui/sftp/`）今天经手机自己那条 SSH 走 SFTP，不经那台后端的文件管理面。

### 3.5 终端与 tmux

起一个会话拆成三个平面，最后那次 exec 必须发生在用户自己的终端进程里——pid 要等于 pidfile 名，tty 与 Ctrl-C 要落在 agent 上，`tmux attach` 要占住调用者的终端：

1. **计划面「跑什么命令」** → 那台后端出成品，**永远只是一行 `ccm …`**（`control/launch_render/`：本机帧命令 `launch-local`，远端 `launch-render-cli`）。环境、中转地址、身份标记由那台的 `ccm` 在最终 exec 那一处定（`control/ccm/`）。
2. **执行面「在那台真的建 tmux」** → 后端 `control/launch.rs`，argv 直传、不过 shell。后台起（`session-new` 的 `place: tmux`）整个在这一面做完，不开窗。
3. **开窗面** → 只能是用户面前那台的前端：后端在远端，开不了用户面前的窗。

**在终端里打开**（桌面）：

- 远端：那台后端 `launch-render-cli` 渲出那一行（接回一个 tmux 会话就是 `ccm -- --attach <名>`）→ 本机后端 `terminal-ssh` 把它包成这台终端方言的一行 `ssh -t … -- 'bash -lic …'`（本体 `dial/terminal.rs`；Windows 上是 PowerShell 那一形，ssh 客户端用全路径）→ monitor `open_terminal_window` 开窗跑它。
- 本机：本机后端 `launch-local` 渲出那一行 → monitor 开窗。
- 界面只经 `src/frontend/ui/terminal-open.ts` 走这条，monitor 只开窗、不拼命令。开窗挑终端：设置里指定的 → 系统出口（`xdg-terminal-exec` · `x-terminal-emulator`）→ 常见终端逐个探；都没有就不开窗、说清楚去设置里指定（`launch_tests.rs::the_terminal_is_picked_by_setting_then_system_exits_then_common_ones` 钉着）。

**在终端里打开**（手机）：手机开自己的终端（`core-terminal` ＋ 自己那条 SSH），在里面附着会话。附着那一行问那台后端 `launch-render-cli` 渲（接回一个 tmux 会话就是 `ccm -- --attach <名>`，与桌面「命令已复制」那一条同一个出处），手机原样敲进自己的终端（`src/mobile/core-claude/src/main/kotlin/com/ccmonitor/mobile/core/claude/link/Terminals.kt` 的 `AttachLine`）；列会话 · 结束 · 抓屏 · 送字也都问那台核心（`terminals-list` · `kill` · `terminal-preview` · `terminal-input`）。手机自己只留用户在 ssh 里也会敲的两条（主机设置里「连上就进某个会话」与 tmux 管理窗里的「新建」，`ssh/TmuxCommands.kt`）。

**tmux 这一面**（都在后端）：

| 事 | 住址 |
|---|---|
| 送键 · 杀会话的三道门 | `control/gate.rs` · `control/gate_rules.rs` · `control/kill.rs`（INVARIANTS §34） |
| 会话打标 `@ccm_sid` | `control/identity_tag.rs`（INVARIANTS §30） |
| 终端名单 | `control/terminals.rs`（`terminals-list`） |
| 往会话里送字 · 送键 | `terminal-input`（桌面终端管理页 · 手机聊天屏都走它） |
| 看画面 · 跟随 | `control/capture_pane.rs` · `control/terminal_follow.rs`，帧 `terminal_screen`，壳 `terminal_screen_relay.rs` 转交 |
| tmux 观测 | `observe/tmux_observe.rs` · `control/tmux_hook.rs`（事件，不轮询） |

### 3.6 文案表

- 对外的每一句话都从一张表来：`src/shared/copy/table.json`（键 → 每种语言一句 ＋ 具名插值点；`rules.json` · `terms.json` 是写法规矩与用词表）。
- 取文口各一份、读同一张表：Rust `src/common/copy-core/`（后端 · monitor · `creds-core` 共用，编译期内嵌）· 界面 `copy-table.ts::copyText`（Vite 读）· 手机 `core-copy`（构建时只抄手机代码引到的那几条，`src/mobile/core-copy/build.gradle.kts`）。
- 键必须是字面量，参数名集合等于表里那一条的 `args`；判据 `tests/copy/copy-table.vitest.ts`，手机生产源码不写死界面中文由 `tests/copy/mobile-literals.vitest.ts` 钉。
- 描述数据与状态的句子由核心写（§2.9），出口只取自己骨架上的字。

### 3.7 生成类型

线上类型只在 Rust 写一份，经 ts-rs 导出成 TS：界面那份在 `src/frontend/ui/generated/`，通信层那份在 `src/comms/ui/generated/`。`npm run gen:types` 重生成（两棵 Rust 树各跑一次 `export_bindings`），`npm run check:types` 要求重生成后零差异；生成物勿手改。手机的帧解码今天是手写的，按冻结格与金样（`tests/__fixtures__/`）对拍。

### 3.8 格目录与 `BUILD_ID`

- **身份**：`src/backend/lib.rs` 的 `BUILD_ID` 是这份后端二进制的身份，编进字节里的身份戳（`CC_MONITOR_BUILD_STAMP`）。换不换后端只按它判：桌面按内嵌字节自报的那个 id，手机按自己带的那个，远端那台自报的 `build_id` 对不上就要换。`PROTO_VERSION` 只在破坏性线上变更时加。
- **什么时候打**：加 / 删命令、改帧形、改冻结格时，合进主线那一刻打新号（`tests/scripts/bump-build-id.sh`：改 `BUILD_ID`，子命令集变了就把指纹历史表里最老那行换掉），再 `tests/scripts/re-embed.sh` 重铺内嵌字节。
- **格目录**（§2.9）与 `BUILD_ID` 同拍：新格进目录、金样重写、冻结格只许加，判据 `tests/backend/faces/cells_catalog_tests.rs`。

### 3.9 手机（`src/mobile/`）

| 模块 | 管什么 |
|---|---|
| `app` | 屏与导航、注入、前台保活；聊天屏（`ui/chat/`）起会话 · 送字 · 正文都走那台核心；终端屏用拨号网关与 tmux 网关 |
| `core-claude` | `link/`：常驻流的帧客户端（门槛只核 `BUILD_ID` · 起常驻 · attach · 一问一答 · 断了接回）与核心成品的解码；`model/` 把通用记录映射成渲染单元 |
| `core-remote` | 中立的远端抽象：一次性 exec · 流式 exec · 双向长 exec（`RemoteDuplex`）· shell 引号 |
| `core-ssh` | 连接管理（单飞 · 探活 · 退避重连）与 `RustTransport`：SSH 本身走 `core-rs` |
| `core-rs` | 手机嵌的 Rust 核心 `libmobcore.so`：后端那一份拨号核心（`src/backend/ssh-dial`）的 FFI 外壳（手写 JNI · 走消息）——竞速 · 跳板 · host key · 鉴权 · SFTP · 钥匙派生与生成 · `~/.ssh/config` 解析都在这一份里 |
| `core-terminal` | 终端引擎（termlib） |
| `core-data` | 本地库（Room） |
| `core-ui` | 共用主题与反馈 |
| `core-copy` | 取文口与大小 · 时长格式（同一张文案表） |

谁调谁：`core-ssh` 拨号 → `core-remote` 开长 exec 跑 `ccm -- --resident-attach` → `core-claude` 在上面说帧（与桌面连远端同一条路、同一份后端）→ `app` 排版。手机与桌面共吃同一批冻结格与金样（IPC-PROTOCOL §7）；手机的界面令牌今天是它自己一套，不与桌面共用。

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

后端自己的状态（`backend.json` · 中转根钥匙 `relay-key` · 额度账 `quota.json` · 轮换 `rotation.json` · 血缘 `lineage.json` · 资产目录 · skill 装记录 · API 号凭据 · 后端日志 · `bin/`）由那台后端写，都经写句柄 `platform::fs::Own`（订阅号凭据与诊断文件各一个窄句柄）；每一项是「删了会丢」还是「可随手删」见 INVARIANTS §2.1。

只读的外部数据源：`<agent 家>/projects/**/*.jsonl`（会话内容）· `<agent 家>/sessions/<PID>.json`（活跃会话，PID ＋ `procStart` 双校验）· `<agent 家>/tasks/<sid>/`（Task 面板），都由那台后端读。

字段定义、编码约束（UTF-8 无 BOM）、原子性与切到终端那一套 → [IPC-PROTOCOL.md](IPC-PROTOCOL.md)。每个文件的完整路径在设置的「数据位置」页。

---

## 5. 关键设计选择与理由

每条都是「为什么不能用别的方案」。规矩本身由判据钉着的那几条只写在 INVARIANTS，这里不重复：顺序靠 `seq` 不靠后端保序（§5 · §9）· 判活要 PID ＋ `procStart`（§6）· 拉前三重校验（§7）· 长耗时同步调用走 `spawn_blocking`（§10）· 浮层真挂 `document.body`（§13）· 启动重放贴底不抖（§21）。

### 零侵入：只读 agent 的数据源

后端只读 `projects/` 与 `sessions/`。写入一律是用户显式触发，而且只经那台后端的文件管理面：删历史会话只收 sid（`files-delete-session`）；从某一轮分叉只新增一份 `<new-sid>.jsonl`，`O_EXCL` 新建、绝不覆盖，原会话零改动（`control/fork_write`）。按 sid 找那份会话文件只有一份实现，两处共用。装别名块、skill、MCP 是用户点名的写，别名块只动 BEGIN / END 块内。

**为什么**：用户对「数据源就是我自己的命令痕迹」的认知不能破；写是必要时的可选副作用，就得是显式的、可见的（足迹页逐条列出）。

### 起子进程只有一个出口

monitor 起子进程一律经 `spawn_managed.rs`，三个策略都是必填参数、都没有 `Default`：控制台（`ConsolePolicy`：`Hidden` · `NewVisible` · `Inherit`）· 生命周期（`Lifetime`：`JobKillOnClose` · `Detached`）· stderr 去向（`StderrSink`：`ToLog` · `Null` · `Inherit` · `Captured`）。还自造 `Command` 的地方逐处登记。给用户开真终端那一处刻意是 `NewVisible`。

**为什么**：Windows 上 GUI 进程起一个控制台程序而不给 flag，系统会新配一个带窗口的控制台——用户看到就会关，关掉就杀死子进程（退出码 `0xC000013A`）。分进程之后，原来免费的东西都要显式管：谁杀谁、控制台策略、错误怎么跨进程传、两边对版、起不起得来。三个必填参数让坏默认值无法被表达；子进程的 stderr 接进 monitor 日志，死亡码说人话（「被控制台事件杀死」而不是裸退出码）。

### 成批交付

重放按块投递，一次 Tauri 事件里一串格，界面推进同一条队列走批量调度。

**为什么**：每次 emit 都有序列化与派发开销，逐条交付几千行时主线程累计阻塞明显，启动可见卡顿。

### 视口外不渲染：content-visibility ＋ 估高 ＋ 骨架

- 顶层卡片带 `content-visibility: auto`，视口外与隐藏 tab 的卡片跳过布局与绘制；`contain-intrinsic-size` 初值由 `height-estimate.ts` 按块类型估（正文用 pretext 测宽、代码块按行数），估值只是初值。估高路径绝不许抛，失败就降级。
- 拿得到会话索引的 tab 与历史查看器接上 `SkeletonView`（`skeleton-view.ts`）：没物化的 seq 区间由占位顶住，一接上滚动条就是全会话的；每次滚动只物化与视口相交的那一段，正文按偏移向那台后端取回，取回途中占位留着高度。
- 没接骨架的 tab 由 `TailWindow`（`live-window.ts`）收纳：只物化尾部，更老的记录只进账本不建卡，滚到顶部附近再按批补。

**为什么**：建卡（markdown · DOMPurify · 高亮）是重放期最大的成本；不建看不见的卡，上万条记录的会话也能秒开。

### 每个窗口一个入口

`index.html` · `settings.html` · `viewer.html` 各有自己的入口（`entry-main.ts` · `entry-settings.ts` · `entry-viewer.ts`）。设置窗的模块图里没有高亮、数学排版与 tab 管理；设置窗关窗是隐藏、复用时重跑取值，主窗销毁时连带销毁它（`lib.rs::windows_to_destroy_after`）。独立只读窗口复用 `TabManager` 过滤到那个会话，订一条 `session-lines/<sid>`，不发 `frontend-ready`。

**为什么**：webview 之间的 JS realm 互不共享，一个入口顶层同步 import 了全部视图，运行期分派到哪一支都要把整张模块图执行一遍——修法只有多入口。viewer 不另写渲染器：再写一套就会与主管线漂移。

### 账号：隔离又同步

一个账号 ＝ 一个 `CLAUDE_CONFIG_DIR`：各自一份登录凭据，两个号可以同时跑、互不踢；skill、记忆、历史、设置经符号链接共享同一份（由后端 `accounts/manage/` 建立和维护）。用户级 MCP 的定义与 MCP 登录在各号之间同步（同一个开关）：定义走三方对照（`mcp_share`）；登录（各号凭据文件里 `mcpOAuth` 那一节）任一号一登录或一续期就把最新那一份写进所有号、在一个号里退出则全部撤（`mcp_logins`），写经订阅号凭据的窄句柄、在那一家续这一条的锁与整份写锁里重读比对、只换那一条（`oauth/store::rewrite`，续期写回也只经它；续期写回碰上那一家正在存盘 ⇒ 新令牌先在内存里照用，写锁放开的文件事件来了再写回，有总期限，`oauth/writeback`）。账号分订阅号与 API 号；API 号的 key 只留在它所在的那台机器，经那台的中转注入，不进命令行、不进环境变量。界面只交意图：建库 · 添加 · 删除 · 设默认 · 修复 · 回滚由后端执行，每次改动前先备份；在 app 里登录（一个订阅号 · 一个号里的一个 MCP 服务器，`accounts/sign_in.rs`）：后端在后台起那一家的官方登录子命令（MCP 那一种要终端，起在伪终端里，`platform/child.rs`），只把登录链接交给界面、把粘回来的码或回跳地址写进它的 stdin，令牌由 agent 自己换、自己写；在一个号里登了 MCP，经上面那条同步到所有号。终端窗口只留在「⋯」里当逃生口。

换号重启直接结束旧会话再 resume 同一个 sid；compact 失败不阻断，kill 失败必须中止——绝不在旧进程还活着时续 resume，否则新旧两个进程抢同一份会话。选不了原账号时不静默换号，拒绝并给出「用当前账号」的显式选择。不重启的换号在中转那一发里换凭据（§3.3），进程不知道。

### 按控制台认窗口（Windows）

点 ↗ 时沿进程链每一级借它的控制台问一句：`AttachConsole` → `GetConsoleWindow` → 那个窗口的属主。借控制台交给本二进制的一次性小助手（隐藏子命令 `--ccm-console`，
`platform/console.rs::helper_main`）：挂控制台是整个进程的状态，monitor 自己挂着控制台时恒被拒（开发构建是控制台程序），小助手先摘掉自己的再借；monitor 一次 ↗ 起它一趟问整条链（`console_looks`）。
Windows Terminal 里每个标签的伪控制台窗口的属主就是承载它的那个终端窗口（拖到别的窗口时由 Windows Terminal 改挂）；经典控制台就是控制台窗口自己；看不见的不算。

**为什么不按进程找窗口**：Windows Terminal 一个进程开几个窗口，按进程只分得出「这个程序有两个窗口」。**为什么不靠接入块登记**：登记要用户装 PowerShell 接入块、
还要那个标签当时在前台、标题没被别人改，cc-monitor 自己从标签栏起的会话（命令里 ssh 用全路径，不带窗口标签）一样落空。控制台的属主由 Windows Terminal 自己维护，
谁起的、装没装接入块都一样认得准。

**切到那个标签页**：Windows Terminal 没有从外部选中别人标签的接口。小助手借那个会话的控制台挂一个记号标题（`ccm-front-` ＋ 8 位），
UI Automation 在那个窗口的标签页里找名字带记号的那个、选中，原标题写回（`bind.rs::pick_tab`，Win11 虚拟机上整趟 100–400 ms）。
标签名不随程序标题变化（手动改过名 · 设置了「禁止更改标题」）、读不到标签页、终端提权时只切窗口，照实说为什么。
「切过去了」要核过前台：系统答应了、前台也确实是它才算（真机见过答应了前台却没换）。

### 日志：tracing 在 Builder 之前初始化

`logging::init` 必须在 `tauri::Builder::default()` 之前调用（全局 dispatcher 只能装一次）：文件层按天滚动、非阻塞写；`EnvFilter` 可热改级别，不重启就生效；日志行不上屏：要让用户知道的出错只走 `ui_error::tell`（码 ＋ 文案键 ＋ 那句话 ＋ 复制详情，事件 `monitor-error`，出口由 `logging.rs::install_error_emitter` 装上，限流防风暴），界面照那一句弹提示；其余 ERROR 只进日志。日志目录建不出来就退化成只写 stdout，monitor 照样起。后端子进程的 stderr 接进 monitor 的滚动日志，脱离起的后端写自己那份日志。

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
  开它自己的窗口（agent 窗口，查看窗那个入口带 `run=`），窗口里才按运行读它的记录（帧命令 `history-run`），运行表一变就续读。流那一侧由后端归位、拼成活卡（`live` 帧带 `run`，`stream/live.rs`），子运行的卡不上主 tab。
- **从 agent 的祖先链猜终端窗口**：explorer 起 PowerShell ＋ WT 接管控制台的常见架构下，agent 的祖先链与 WT 窗口完全脱节，启发式在主流环境下都不可靠。本机改为点 ↗ 那一刻沿 agent 进程往上，每一级问它的控制台显示在哪个窗口（Windows）。远端会话从此刻连着它的终端出发：那台读出那个终端的 `SSH_CONNECTION`（在 tmux 里就问 tmux 此刻哪些客户端连着），本机后端按这四元组在连接表里找到这台电脑上开着那条连接的进程，往上数父进程到 `explorer.exe` 为止，monitor 沿链每一级先问它的控制台显示在哪个窗口，再取第一个有可见窗口的进程。链从连接的拥有者起、不从 agent 起；找不到就照实说原因（在 tmux 后台没人连着 · 不在这台电脑上 · 经跳板或端口转换对不上 · 没有窗口或不止一个）。
- **换掉 webview**：这个 app 的核心是渲染会话记录（Markdown · 代码高亮 · LaTeX · 可折叠工具卡 · 流式追加 · 上万条记录的虚拟化），正是 HTML 最擅长、原生 GUI 工具箱最不擅长的那一类。文件窗口不渲染会话记录，所以它是原生（egui）的。

---

## 7. 判据与门禁的分层

**判据**是 `tests/` 下的测试：单测、登记表（`*_registry` · `*_guard` · `*_ledger`，从盘上全集派生人群、默认拒绝，INVARIANTS §46）、金样（`tests/__fixtures__/`）、e2e 套件（`tests/e2e/`）。Rust 判据经 `#[path]` 挂进各自的 crate，跟那棵树的单测一起跑。

**入口分四层**（脚本都在 `tests/scripts/`，跑法见 [`tests/scripts/README.md`](../../tests/scripts/README.md)）：

| 层 | 脚本 | 什么时候 | 跑什么 |
|---|---|---|---|
| 写的时候 | — | 每改一处 | 增量编 ＋ 只跑改到的那几块（`cargo test --lib -- <模块>` · `npx vitest run <文件>`） |
| 交回前 | `quick-check.sh` | 一份改动做完 | 两棵 Rust 树的排版 · tsc · 后端整套单测 ＋ 壳那棵 `cargo test --workspace --lib` · 整套 vitest，在 `.build/` 下的一次性沙箱里跑、不拿锁；改到手机读的东西才加跑 ktlint · detekt · 手机 JVM 单测。「加了子命令要打版本号」单列成待办、不算红 |
| 推候选分支前 | `gate-affected.sh --trust-ci` | 合进主线之前 | 先 `quick-check.sh`；再按改动挑门禁的格（`gate-affected/map` 路径 → 格组，`gate-affected/plan.py` 挑），只跑恒跑的便宜格 ＋ 被改到的本机才有的格 ＋ 被改到被测对象的 e2e 套件，CI 会跑的交给 CI；拿门禁锁 |
| 打发版标签前 | `gate-clean.sh` | 发版 | 清掉会话变量、排门禁锁，跑全量 `gate.sh`（各格一遍，`GATE: OK` 才打标签） |

- `gate.sh` 是格的唯一名单（`run_gate` · `run_e2e` …），`GATE_ONLY=<格>` 只跑点名的；CI 的 Linux job 就是这么调它。
- 推送：`push-candidate.sh` 先把合并好的分支推到候选分支 `next`，`ci.yml` 在上面跑与 main 同一套 job，全绿才快进 `main`；`release.yml` 只认标签，第一个 job `ci-gate` 要求这个提交上有一条绿的 CI。
- 重活（编译 · 测试 · 门禁 · gradle）经 `heavy.sh`：单独一个封顶内存的 scope ＋ 槽位排队，没有 `systemd-run --user` 的机器直接跑。
- 门禁、挑格、推送这几份脚本自己也有判据（`tests/backend/flow_scripts_guard.rs`）。

---

## 8. 入门读图

- 整体：本篇 §0 ＋ §1
- 一件事该放在哪一层：本篇 §2
- 某个子系统谁调谁：本篇 §3
- 加 jsonl 记录类型、加帧命令、加设置项 / 快捷键：[CONTRIBUTING.md](CONTRIBUTING.md)
- 改跨进程文件或帧协议：[IPC-PROTOCOL.md](IPC-PROTOCOL.md)
- 改某个具体模块：对应目录的 README

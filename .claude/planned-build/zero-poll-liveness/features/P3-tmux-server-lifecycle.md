# P3 — tmux server 生 / 死 / 复活事件化

> 主计划：`../MASTERPLAN.md` §1 P3 · 前置：P0（socket 与 cgroup 事实）· P1（四值枚举）· P2（pidfd + 统一 channel）
>
> **本功能消掉用户那份调研里唯一的 ⚠ 盲区**（「server 复活只有 inotify 能做、未装 inotify-tools」），
> 并把 P1 那处刻意保守的判据收紧。

## 1. DoD

- [x] **死**：pidfd 绑 tmux server pid ⇒ server 退出立刻等价"零会话"，不等 8s 节拍
- [x] **复活**：socket 目录 inotify `IN_CREATE` ⇒ 重挂 pidfd + 重探，不等 8s 节拍
- [x] 复用 P2 的 `pidfd_open`/`spawn_pid_watcher`，**全 crate 仍只有一处 pidfd 的 `unsafe`**
- [x] 收紧 P1 的 `rc=1` 判据，**不改帧契约**
- [x] **刻意不删 `TMUX_EMIT_INTERVAL`**（P5 的事）
- [x] 只加 `WatchEvent` 变体 + 发送方，**没有新挂任何独立定时器**（P2 立的硬约束）
- [x] pidfd 用在**真 tmux server 进程**上的双向验收（P2 只测了会话进程）
- [x] **跨 cgroup 存活**用真 daemon 自测（P0-③ 只测了拓扑）
- [x] 全门禁绿且数字不降

**明确不做**：不给 inotify 溢出（E39）补定时器 · 多 server / 多 socket 只管 daemon 观测的那个 ·
不碰 `~/.cc-monitor/bin/` 那个用户实况在跑的 daemon

## 2. 实现要点

### 2.1 `TmuxObservation` 细分，而帧契约逐字节不变

```
Sessions(String) | ServerEmpty | NoServer | NoTmux | Unobservable
                   ↑ P3 新增的两个细分（原来合成一个 ZeroSessions）
```

`ServerEmpty`（rc=0 + 空 stdout，`exit-empty off` 那格）与 `NoServer`（rc=1）
**在 wire 上映射到同一个取值** `zero_sessions` ⇒ **帧契约与 P1 逐字节一致**。
这正是 P0/P1 预判的「P3 加细分时不必改帧契约」，并且有测试机器化钉住
（`observation_frame_keeps_raw_payload_backward_compatible` 里两个细分都断言映射到 `OBS_ZERO_SESSIONS`）。

### 2.2 server pid / socket 路径从哪来

`tmux display-message -p '#{pid}\t#{socket_path}'`（P3 实测两个格式串在 tmux 3.6 上都存在）。

**和 `tmux ls` 放同一个探测线程里**（`run_tmux_probe`）：`display-message` 同样是无超时
subprocess，`run_tmux_ls` 头注那条约束对它一样适用 ⇒ 不新增线程、不阻塞主循环
（P2 立的硬约束）。只在"可能有 server"时才问——`NoTmux`/`Unobservable` 下问了白问；
**`ServerEmpty` 也要问**（`exit-empty off` 下 server 活着）。

### 2.3 pidfd 目标泛化：一处 `unsafe` 服务两种目标

```rust
enum PidWatchTarget { Session { key: PathBuf }, TmuxServer }
```
`spawn_pid_watcher(target, pid, expected_start, tx)`，醒了按 `target.death_event(pid)`
发 `PidDied` 或 `TmuxServerGone`。⇒ P3 **一行 unsafe 都没新增**。

server 的 procStart 基线我们没有（不是从 pidfile 读的）⇒ 传 `None`，判据 2 退化成存在性
（`is_same_live_process` 的 `_ => true` 臂）。

### 2.4 收紧 P1 的 `rc=1`：**刻意不依赖"pidfd 是否醒过"**

P1 把 `tmux ls` rc=1 一律判"确证零会话"，并留注释说 P3 可以收紧。落地在
`classify_with_server_state(obs, server)`：

| 观测 | server 状态 | 结果 |
|---|---|---|
| `NoServer` | `Alive(pid)` **且 `pid_alive(pid)`** | **`Unobservable`**（server 明明活着而 `tmux ls` 连不上 = 真异常，保守跳过） |
| `NoServer` | `Alive(pid)` 但 pid 已不在 | `NoServer` 原样通过 |
| `NoServer` | `Unknown` / `Gone` | `NoServer` 原样通过 |
| 其他任何观测 | 任何 | **原样穿过**（守卫范围 = 性质范围） |

**为什么不写成「pidfd 还没醒 ⇒ 压成 Unobservable」**——那有个危险失效模式：
pidfd 路万一没醒（线程起失败、poll 出错放弃看守），状态永远停在 `Alive`，
`rc=1` 被**永久**压成 `Unobservable` ⇒ **永不 retire**。改成直接查 `/proc` 里那个 pid
还在不在：一次存在性读，无定时器、无状态耦合、**无挂死风险**。
变异验收专门钉了这一条（§3 变异 A）。

### 2.5 复活：inotify 监视 socket **所在目录**

P0 实测三条支撑这个设计：
- server 死后 **socket 文件仍留着** ⇒ **「文件存在」≠「server 活」**，只当"该重探一次"的
  触发器，**绝不拿存在性判活**
- 复活时 **inode 会变**（tmux unlink+create）⇒ `IN_CREATE` 能感知
- 死 socket 上调用不会把 server 拉活 ⇒ 重探安全

实现：**挂进既有的 debouncer**（不新建第二个 inotify 实例），消费侧按**精确 socket 路径**
过滤（`watched_socket.as_deref() == Some(p)`）。

⚠ **必须监视目录而不是文件**：要感知的是"文件被重新 create"，watch 文件本身在它被
unlink 后就失效了。代价：同目录下**其他** socket 的变化也会进 debouncer（本机
`/tmp/tmux-1000/` 有上百个静态残留文件，不产生事件；活动只来自真实 server 启停，量极小），
按精确路径过滤后不会误触发。

**生产含义（如实记录）**：daemon 会对 `/tmp/tmux-<uid>/` 加一个**只读** inotify watch。

### 2.6 死亡路为什么要自己发帧

`TmuxServerGone` 的处理**直接 `sink.send(observation_to_frame(NoServer))`**，不等下一次探测。
这同时解决一个竞态：若 rc=1 的探测结果先到（那时状态还是 `Alive`）被 §2.4 压成
`Unobservable`，随后 `TmuxServerGone` 到达仍会补上这一帧 ⇒ **信号不丢**。

## 3. 变异验收 + 端到端实测（Phase D）

**强度：中高风险**（新事件源 + 状态机 + 改判据）⇒ 变异 + 全门禁 + **两次隔离端到端**。

### 3.1 变异

| 变异 | 做了什么 | 结果 |
|---|---|---|
| **A** | 去掉收紧判据里的 `pid_alive(pid)` 复核（= 退化成"只要状态 Alive 就压成 Unobservable"，正是 §2.4 刻意避开的危险写法） | **成立**。红在「server 真没了就该照常判零会话」那格（`left: Unobservable / right: NoServer`）⇒ 钉住了不许退化成永久压制 |

**一次无效判色，如实记录**：变异 A 的初版我把绑定改成了 `_pid`，导致 `warn!` 里的 `{pid}`
找不到 ⇒ **编译失败**。那时的"测试红"是无效信号（判色三步②：判据是运行时行为时必须先
确认编译过）。重做成保留 `pid` 绑定的版本、看到 `Finished test profile` 之后才判色。

### 3.2 单元测试（4 条新增，共 140）

- `no_server_is_tightened_only_when_the_pid_is_really_still_alive`：四个方向
  （自己的 pid=铁定活 ⇒ 收紧 · 已回收的 pid ⇒ 不收紧 · `Unknown`/`Gone` ⇒ 不收紧 ·
  **其他四种观测原样穿过** ← 守卫范围 = 性质范围）
- `pid_watch_target_maps_to_the_right_death_event`
- `pidfd_watches_a_real_tmux_server_and_stays_silent_while_it_lives`：**真 tmux server** 上的
  双向验收，隔离 socket（`-L ccmP3-<pid>`）+ `-f /dev/null`。
  **无 tmux 就硬失败而不是静默跳过**——静默 SKIP 正是 gate-integrity 在治的那个病
- `tmux_server_query_yields_nothing_without_a_server`：拿 PATH 前置的假 tmux（rc=1）跑
  **与生产同一段脚本**，断言 rc≠0 且无 stdout

### 3.3 端到端冒烟一：server 死 + 复活（隔离 socket）

做法：给 daemon 一个 PATH 前置的 `tmux` 包装脚本，`exec /usr/bin/tmux -L p3e2e "$@"`
⇒ daemon 全程只看得见**隔离 socket**，**绝不碰默认 socket**。

```
启动     : tmux_sessions raw 含 "only" 会话
           日志：已给 tmux server pid 1021303 挂 pidfd 看守
           日志：已监视 tmux socket 目录 /tmp/tmux-1000
kill-server → {"kind":"tmux_sessions","raw":"","observation":"zero_sessions"}
           **27ms**（日志同时有「tmux server pid 1021303 已退出（pidfd）」）
复活     → tmux_sessions raw 含 "revived" 会话，**153ms**
           日志：已给 tmux server pid 1021426 挂 pidfd 看守（新 pid）
```

对比 P3 之前：这两格都得等 8s 节拍（且"变灰"还要 × threshold 2 ≈ 16s）。
复活那 153ms 里含 `DEBOUNCE_MS = 100`，基本就是"去抖 + 一次 `tmux ls`"。

**测法说明**：每 50ms `grep` 一次 stdout 落盘文件、首次命中即停并计时 ⇒ 含轮询与落盘开销，
是**上界**。

### 3.4 端到端冒烟二：跨 cgroup 整锅 SIGKILL（用真 daemon）

P0-③ 只测了 cgroup **拓扑**，P2 只测了会话进程。这一格补上：

```
tmux server : /user.slice/user-1000.slice/user@1000.service/app.slice/ccmP3cg-1024879.service
              （systemd-run --user --property=KillMode=control-group，transient、不落盘）
daemon      : /user.slice/user-1000.slice/user@1000.service/tmux-spawn-6a9f05c0-….scope
              ⇒ **不同锅**
systemctl --user kill --signal=SIGKILL ccmP3cg-1024879.service
  → {"kind":"tmux_sessions","raw":"","observation":"zero_sessions"}  **30ms**
  → 日志：tmux server pid 1024915 已退出（pidfd）
回滚        : stop + reset-failed，实测 0 残留
```

⇒ **daemon 自持的 pidfd 探针确实扛得住 tmux 整锅 SIGKILL**，用的是我方生产代码路径
（不是 python 探针）。

### 3.5 清场核对

| 项 | 结果 |
|---|---|
| 测试 socket（`p3srv`/`p3e2e`/`p3cg`/`ccmP3-*`） | 全清 ✓ |
| 默认 socket 会话清单 | 逐字未变 ✓ |
| 默认 socket 已设 hook 数 | 仍 **0** ✓ |
| systemd 用户单元 | 0 残留 ✓ |
| 孤儿进程 | 无 ✓ |
| 装的软件包 / 家目录改动 | 0 ✓ |

## 4. 工程审计结果（Phase E）

### 4.1 账本对账

| 账本行 | 本功能做了什么 | 状态 |
|---|---|---|
| 1 `watch_loop` | **只加了两个 `WatchEvent` 变体 + 两个发送方**（server pidfd 看守、socket 事件路由到重探）。**零新定时器**、循环结构未动 | ✅ P2 立的硬约束守住了 |
| 2 `run_tmux_ls` 契约 | 细分为五值，但 **wire 映射不变** | ✅ 契约层面未破（P1 的承诺兑现） |
| 3 wire + EMITS | **未触及** | ✅ |
| 4 `ssh_source` 收帧臂 | **未触及**（monitor 侧零改动） | ✅ |

**这一行值得单独说**：P3 是本工作区第一个"纯加事件源"的功能，而它确实只加了变体和发送方
——**P2 把结构做对了，P3 就便宜**。这是账本机制起作用的直接证据。

### 4.2 用户调研的 ⚠ 盲区已消

调研原文：「**server 复活** | 只有 inotify 能做；未装 inotify-tools | ⚠ **盲区**」，
并建议"接受盲区"。在 daemon 里它不是盲区——`notify` crate 就是 inotify、早就链着，
本功能只是**多加一个 watch 目标 + 一条精确路径过滤**。实测 153ms 感知复活。

### 4.3 对后续的影响

- **P4**（装 hook）：`ServerState::Alive(pid)` 这个状态点正是"该（重）装 hook"的时机
  ——hook 活在 server 内存里、每次 server 起来都要重装，而 P3 刚好把"server 起来了"
  变成了一个**事件**。P4 只要在那个 `match` 臂里多调一个装 hook 的函数。
- **P5**：删 ticker 的硬前置（接 `WatchEvent::Shutdown`）不变。另外 P5 删掉 8s 之后，
  「多个中杀一个」靠 hook、「杀到没了 / server 被端」靠 P3 的 pidfd、「复活」靠 P3 的
  inotify ⇒ **三格合起来无缺口**，这是删轮询 B 的前提条件已经齐了的证据。
- **P6**：零定时器守卫届时应断言 `watcher.rs` 里除 `DEBOUNCE_MS` 外无 `Duration::from_secs`。

### 4.4 范围外，如实标注

- **多 tmux server / 多 socket**：只管 daemon 观测的那一个（本机实测有 2 个 server）。
  daemon 的 `tmux` 调用走默认 socket 解析，所以它天然只看一个。
- **inotify 溢出（E39）**仍是盲区，本轮**没补定时器**。注意 P3 的复活路**依赖 inotify** ⇒
  溢出时会漏掉一次复活感知，但 8s ticker 仍在（P5 之前）⇒ 现阶段有兜底。
  **P5 删 ticker 时要重新评估这一格**（已写进 §4.3 与 STATUS）。
- **`/tmp/tmux-<uid>/` 会被加一个只读 inotify watch**——这是本功能的固有代价。

## 5. 签收

- [x] 通过代码审计（中高风险档：变异 A 成立 + 4 条新单测 + 两次隔离端到端 + 一次无效判色如实记录）
- [x] 通过工程审计（账本第 1 行只加事件源零新定时器；调研 ⚠ 盲区已消；三条对后续的影响已回写）
- [x] 主计划已据此更新（§7 变更记录 06）

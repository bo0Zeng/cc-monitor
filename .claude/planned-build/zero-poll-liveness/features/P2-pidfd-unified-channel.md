# P2 — pidfd 替掉判活轮询 A + 建统一事件 channel

> 主计划：`../MASTERPLAN.md` §1 P2 · 前置：P0（cgroup 事实）· 后继：P3/P4 往本轮建的 channel 里加事件源
>
> **本功能建立账本第 1 行的最终形态**，并交付本工作区唯一一条**正确性**改进
> （PID 复用从"检测得更准"变成"无从发生"）。

## 1. DoD

- [x] `watch_loop` 阻塞在**无超时 `recv()`**、消费**单一** `mpsc<WatchEvent>`（账本第 1 行）
- [x] 轮询 A（2s tick 驱动的判活扫描）**消失**；`recv_timeout` 全仓零命中
- [x] 轮询 B（8s `tmux ls`）**刻意保留**，但从"主循环里的节流判断"搬进独立 ticker 线程
      ⇒ P5 删 ticker 即可，不必再动循环结构
- [x] pidfd 绑**进程实例**⇒ PID 复用在机制上不存在（主计划成功标准 2）
- [x] **pidfd 双向变异验收**：杀 → 事件到；**不杀 → 事件不到**（后者才是钉住"由目标退出驱动"的那条）
- [x] 不动 wire（纯 daemon 内部改进，可独立验收）
- [x] 全门禁绿且数字不降

**明确不做**：不动 wire · 不删 `TMUX_EMIT_INTERVAL`（P5）· 不给 inotify 溢出（E39）补定时器 ·
不起真实已认证的 `claude`/`codex`（测试靶子用 `sleep`）

## 2. 实现要点

### 2.1 统一事件 channel

```rust
enum WatchEvent {
    Notify(DebounceEventResult),        // notify-debouncer-mini 经 DebouncerSink 转投
    PidDied { key: PathBuf, pid: u32 }, // pidfd 醒了
    TmuxObserved(TmuxObservation),      // 一次性 tmux ls 探测线程的结果
    TmuxProbeDue,                       // ticker 节拍（唯一剩下的定时器，P5 删）
    Shutdown,                           // 预留给 P5（见 §2.5）
}
```

`DebouncerSink` 实现 `notify_debouncer_mini::DebounceEventHandler`，把 notify 事件转投进
统一 channel ⇒ **零额外线程**（notify 本来就在自己线程里回调 handler，只是换个投递目标）。

**给 P3/P4 的硬约束**（已写进源码头注）：往里**加变体 + 加发送方**，
**不许**各自再挂独立线程 + 定时器。

### 2.2 pidfd

`pidfd_open(2)` 经 `libc::syscall(SYS_pidfd_open, …)`。三条判据按顺序：

1. `pidfd_open` 失败（`ESRCH` 等）⇒ 目标已不在 ⇒ 立刻发 `PidDied`
2. open 成功后**再校验一次身份**（`session_alive(pid, expected_start)`，复用既有纯函数）
   ⇒ 不符 = 在"读 pidfile"与"开 pidfd"之间发生了 PID 复用、我们开到了冒名者 ⇒ 发 `PidDied`。
   **这就是原先那套 procStart 启发式的全部去处**：从"每 2s 复查一遍"降成"挂看守时校验一次"，
   之后身份由**内核**保证
3. 起线程阻塞在 `poll(pidfd, POLLIN, -1)`；醒了发 `PidDied`

**`poll` 真出错（非 `EINTR`）时刻意不发 `PidDied`** ——宁可让会话留在 live 等 pidfile
删除或断连来收，也不因一次系统调用失败误归档。与本文件 `is_same_live_process` 头注那条
「瞬时读失败绝不误归档」同一条纪律。

**陈旧唤醒**：`PidDied` 带 `pid`，消费侧比对 `state.sessions[key].pid == pid` 才退休。
所以「pidfile 先被删、进程后死」那条看守线程发的迟到事件被自然挡掉、无副作用。
Batch6-F22-② 的引用计数语义原样保留（仍经 `retire_sid_if_unreferenced`）。

**线程数的界**：每个被追踪的 `(pidfile, pid)` 对最多一条，实际个位数。
`pid_watched` 按**对**存而不是按路径——同路径换 pid（`/clear` 原地换 sid、PID 复用写同路径）
要能重新挂；按对存则**任何移除路径都不必做清理**（daemon 生命周期 ⊆ 一次 SSH 连接）。

### 2.3 删掉的状态

`SessionEntry.start` 被删——**那就是 2s 轮询的 procStart 基线**（每轮拿它跟 `/proc` 现值比对）。
pidfd 之后基线只在挂看守那一刻用一次（`arm_pid_watcher` 的实参）。
删它是"轮询消失 ⇒ 它的状态也消失"，不是丢信息。

### 2.4 依赖

`libc = "0.2"`。**它本来就是 tokio 在 unix 上的传递依赖**（daemon 自己的 `Cargo.lock` 里
`libc 0.2.186` 早已解析），所以声明它：`Cargo.lock` 的 diff **只有一行**
（`cc-monitor-remote` 的 dependencies 加 `libc`），零版本变动，`cargo build --offline` 直接过。

### 2.5 一处留给 P5 的坑，已写在源码里

主循环改成无超时 `recv()` 后，`sink.is_closed()` 只在**有事件到达时**才被复查。
现在 ticker 每 8s 醒一次，所以"写端关了就停读"这个优化仍然生效。
**P5 删掉 ticker 时必须把写端关闭接到 `WatchEvent::Shutdown`**，否则 reader 线程会一直
阻塞在 `recv()`（进程退出时才随之消亡——不是泄漏，但不再"没人听就停读"）。
变体已建好 + 注释写在它上面，P5 会读到。

## 3. 变异验收 + 端到端实测（Phase D）

**强度：中高风险**（改主循环结构 + 新增 unsafe + 判活语义换机制）⇒ 主线程变异 + 全门禁 +
**一次隔离端到端冒烟**（`watch_loop` 要真文件系统 + notify + 多线程，任何单测都碰不到它）。

### 3.1 两条变异，都先确认编译过再判色

| 变异 | 做了什么 | 结果 |
|---|---|---|
| **A** | `poll` 的 timeout 从 `-1` 改成 `0`（立刻返回、不等内核唤醒） | **成立**。编译过，`pidfd_watcher_fires_on_death_and_stays_silent_while_alive` 红在**反方向那半边**：「目标活着时不该有事件，实得 true」。⇒ 钉住了「事件由**目标进程退出**驱动」，而不是"无条件发" |
| **B** | 把事件 channel 的创建挪回 Phase 1 **之后**（复现我自己犯过的那个缺陷） | **成立**。`events_channel_is_created_before_the_initial_scan` 红并打出实测偏移（`tx@23345 scan@22343`） |

### 3.2 我自己犯的一个静默缺陷，以及为它补的守卫

初版把 `state.events_tx = Some(...)` 写在 `// --- Phase 2: live watch. ---` 处
——**而 Phase 1 的初始扫描已经在调 `process_session_added` 了** ⇒ 那时 `events_tx` 是 `None`
⇒ `arm_pid_watcher` 直接 return ⇒ **daemon 启动时就活着的会话一个 pidfd 看守都没有**、
永远判不出死。而 P2 之前那条 2s 轮询**是覆盖它们的** ⇒ 这是回归。

**它不是被任何测试抓到的**，是被 clippy 的「field `start` is never read」**间接**暴露：
我去查"为什么这个字段没人读"，才发现 Phase 1 那条路根本没走到挂看守。

⇒ 补了 `events_channel_is_created_before_the_initial_scan`：扫源码断言注入点的偏移
**小于** Phase 1 锚点的偏移。反向自检断言的是「两个锚点都找到 + 源码真读进来」
（`src.len() > 1000`），**不是"命中数 < N"**——阈值不能挂在被检查的量上。

**教训**：把一个副作用挂到某个函数里时，要问"这个函数**所有**调用点都在我注入依赖之后吗"。
`process_session_added` 有两个调用点（Phase 1 扫描 + Phase 2 notify 臂），我只想到了后者。

### 3.3 端到端冒烟（隔离，零真实 socket 接触）

`watch_loop` 无单测覆盖，所以本轮跑了一次隔离端到端：
- `CLAUDE_CONFIG_DIR` 指向 scratchpad 里新建的 `cfg/{projects,sessions}`
- **PATH 前置一个假 `tmux`**（`exit 1` = server 不在）⇒ daemon 全程**不碰任何真实 tmux socket**，
  且结果确定
- 靶子是 `sleep 60`（**绝不起真实已认证的 claude**），按它真实的 `/proc/<pid>/stat` 第 22 字段
  写 `procStart` 进 pidfile ⇒ 走 `add_time_verdict` 的**主证据路**（procStart 相等），
  不依赖"cmdline 像不像 claude"的启发式

结果：

```
{"kind":"hello","v":1,"build_id":"p1q-accounts",...}
{"kind":"session_added","sid":"1111...5555","session_kind":"interactive","status":"idle"}
{"kind":"tmux_sessions","raw":"","observation":"zero_sessions"}      ← P1 那条路也活着
（kill 掉 sleep）
{"kind":"session_removed","sid":"1111...5555"}
从 kill 到 session_removed 落盘：0.0179 秒
```

三条结论：
1. **Phase 1 初始扫描发现的会话确实挂上了看守**（§3.2 那个修复端到端验证通过）
2. **P1 的 `observation` 字段在真 daemon 上确实发出来了**
3. **~18ms**（实测，非估算）。测法：`kill` 后每 50ms `grep` 一次 stdout 落盘文件，
   首次命中即停并计时 ⇒ 这个数**含 grep 轮询与落盘开销**，是"从 kill 到我能观测到"的上界。
   对比 P2 之前：那条 2s tick ⇒ 最坏 ~2s。**降了两个数量级。**

### 3.4 本轮没做的验证，如实标注

- **跨 cgroup 存活未复测**：pidfd 探针"扛得住整锅 SIGKILL"是**用户调研**测过的
  （其 §10 有 `systemd-run` 双单元 + SIGKILL 那组）。P0-③ 只测了 cgroup **拓扑**
  （daemon 经 SSH 起 ⇒ 落新 `session-<N>.scope`，与 tmux server 必然不同锅）。
  P2 的 pidfd 用在**会话进程**上而不是 tmux server 上，那格由 **P3** 用到时再自测。
- **inotify 溢出（E39）仍是盲区**，本轮**刻意不补定时器**。但 P2 把判活这一路
  **摘出了 inotify 依赖**（pidfd 是内核直通）⇒ 情况变好，不是变差。

## 4. 工程审计结果（Phase E）

### 4.1 账本对账

| 账本行 | 本功能做了什么 | 是否到最终形态 |
|---|---|---|
| **1 `watch_loop`** | **建立最终形态**：无超时 `recv()` + 单一 `mpsc<WatchEvent>`；轮询 A 消失；轮询 B 搬进 ticker 线程 | ✅ **到位**。P3/P4 只加变体+发送方；P5 删 ticker + 接 `Shutdown` |
| 2 `run_tmux_ls` 契约 | 未改（P1 建立的四值枚举原样） | ✅ 保持 |
| 3 wire | **未触及** | ✅ 本功能明确不动 wire |
| 4 `ssh_source` 收帧臂 | 未触及 | ✅ |

### 4.2 对后续的影响

**① P6 的载体问题，P2 顺手给出了答案。** P1 审计发现原计划的 graylight 一族不做 socket 隔离
（E41）。而 §3.3 那个冒烟用的模式——**隔离 `CLAUDE_CONFIG_DIR` + PATH 前置假 `tmux` +
读 daemon stdout 的帧**——**根本不需要任何 tmux socket**，天生隔离。
⇒ P6 的「daemon 侧延迟 e2e」可以直接照这个模式建，不必先去改那 6 套。
（真 tmux 那半边的延迟——P5 的 hook 路——仍需带 `-L` 的真 socket。两半边分开做。）

**② P3 的 pidfd 复用面已经铺好。** `pidfd_open` / `spawn_pid_watcher` 是通用的，
P3 给 tmux server 挂看守时**只是多一个调用点 + 一个 `WatchEvent` 变体**，不必再写 unsafe。

**③ P5 有一条硬前置**（§2.5）：删 ticker 前必须接 `WatchEvent::Shutdown`。

### 4.3 unsafe 的账

新增两处 `unsafe`，都写了 SAFETY 注释：
- `libc::syscall(SYS_pidfd_open, …)`：只读地为目标进程创建 fd，不解引用任何指针；
  `rc < 0` 时不构造 `OwnedFd`
- `libc::poll(&mut pfd, 1, -1)`：`pfd` 是栈上合法 `pollfd`、`nfds=1` 与之匹配；
  `fd` 的所有权在闭包里，poll 期间不会被 close

fd 由 `OwnedFd` 接管 ⇒ 线程结束时自动 close，无泄漏。

### 4.4 一处如实记录的观察（与本功能无关，但值得知道）

清场时看到本机有一个**活的生产 daemon**：`~/.cc-monitor/bin/cc-monitor-remote --with-bg
--tail-only`（`p1q-accounts` 构建，二进制日期 2026-07-28）。它不是本轮的孤儿
（我的冒烟跑的是 `remote-daemon-proto/target/debug/` 那个，已退出）。
**相关性**：P5 bump `BUILD_ID` 之后，这台机器上那个 daemon 会在下次连接时被判 stale 并重装。

## 5. 签收

- [x] 通过代码审计（中高风险档：两条变异双向成立 + 端到端冒烟 + 自查出并补掉一个静默回归）
- [x] 通过工程审计（账本第 1 行到最终形态；三条对后续的影响已回写）
- [x] 主计划已据此更新（§7 变更记录 05）

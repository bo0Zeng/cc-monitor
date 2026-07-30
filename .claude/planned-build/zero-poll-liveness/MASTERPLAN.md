# 主计划 / MASTERPLAN — zero-poll-liveness（判活从轮询改成内核事件）

> 所有功能宏观设计的**单一事实来源**。每次修订在末尾「§7 变更记录」追加一行。
>
> **状态：✅ 全区收官（2026-07-30）—— P0-P7 八个功能全部交付签收。**
> **主计划用户已批准 + P4 hook 授权已给。两条轮询都已删，生产段零定时器。**
> **成果落档：`doc/INVARIANTS.md` §41（四路事件 + 三盲区）· BACKLOG E34 已结案。**
> **来源：BACKLOG E34（用户 2026-07-29 点名「我要把轮询杀掉」）+ 用户 2026-07-30
> 追加要求「daemon 是能改的，我的要求就是性能最佳且不要轮询」。**

---

## §0.0 当前事实（2026-07-30 本轮实测，不是照抄旧文档）

### 事实 1：daemon 里有**两条**轮询，不是一条

E34 登记时只盯着 tmux 那条。实际 `watch_loop`（`remote-daemon-proto/src/watcher.rs:117`，
跑在**一条专用 OS 线程**上、`.spawn(move || watch_loop(...))`，同步阻塞在
`notify_rx.recv_timeout(Duration::from_secs(2))`）里有两个独立的定时动作：

| # | 轮询 | 常量 | 它检的是什么 | 延迟 |
|---|---|---|---|---|
| **A** | 判活扫描 | 循环 tick **2s**（`recv_timeout` 超时值本身） | pidfile 还在但 PID 已死（CC 被强杀，没人清 `sessions/<PID>.json`）+ PID 复用（`procStart` 不匹配，#34） | ≤2s |
| **B** | `tmux ls` | `TMUX_EMIT_INTERVAL = 8s`（`watcher.rs:65`） | tmux 会话被带外杀 | 8s × `RETIRE_MISS_THRESHOLD`(2) ≈ **16s** |

⇒ 「把轮询杀掉」的完整范围是 **A + B 都得换**。只换 B 的话那条 2s tick 还在，
`watch_loop` 依然是个定时器循环。

### 事实 2：daemon 的进程模型使这件事比调研设想的**便宜得多**

- daemon 是**我方仓内 Rust**（`remote-daemon-proto/`，standalone crate，**125** 个测试
  （此前记的 47 是 `watcher.rs` 单文件里 `#[test]` 的数量，已订正），
  CI 有独立 job 跑 `fmt --check` + `clippy --all-targets` + `test`）
- 它**已经依赖 `notify 6.1`**（Linux 上就是 inotify）+ `notify-debouncer-mini 0.4` + tokio
- 它由 monitor 经 SSH 直接 exec（`~/.cc-monitor/bin/cc-monitor-remote`）⇒
  **daemon 的生命周期 ⊆ monitor 的连接生命周期**，且**它不是 tmux 的子进程**

这三条各自消掉用户那份调研里的一个成本：

| 调研里的成本 | 在 daemon 里为什么不存在 |
|---|---|
| 「⚠ 盲区：server 复活只有 inotify 能做，未装 inotify-tools」 | `notify` crate **就是** inotify，早就链着了。对 socket 目录加一个 `IN_CREATE` watch 即可 ⇒ **盲区消失** |
| 「pidfd 探针要 1 个 systemd unit + 1 个 python 脚本」 | daemon 自己 `pidfd_open` + 一条阻塞 `poll` 线程 ⇒ **零外部单元、零 python** |
| 「hook 要写 `~/.tmux.conf` 才能持久」 | hook 活在 server 内存里、server 重启就没 —— 而「server 重启」正是我们**有事件可感知**的（socket 目录 `IN_CREATE`）⇒ daemon 每次感知到 server 就（重）装一次 hook ⇒ **不写任何家目录文件** |
| 「探针必须住在 tmux 之外的 cgroup 才扛得住整锅 SIGKILL」 | daemon 是 `sshd` 的子进程（`session-N.scope` 一族），tmux server 在 `user@1000.service`（调研 §12 实测）⇒ **本来就不同锅**。**仍需实测确认**（列为 P0-③） |

### 事实 3：`§24bis` 里有一条**预先登记、明确卡在「daemon 零改」上**的残留 bug

`doc/INVARIANTS.md:408` 原文：

> **已知残留（daemon-bound，记档待版本批次）**：收帧收割器对**空 backend 保守跳过**
> （`ssh_source.rs` `!backend.is_empty()` 门）……代价：当**被杀的是该 origin 最后一个
> tmux 会话**时，tmux server 退出→daemon `run_tmux_ls` 回空串→收割器整段跳过→该
> idle-tmux 灰灯**卡到断连 flush 才清**……干净修法=daemon 对「命令成功但零会话」回
> 确定性哨兵（如 `NO_SESSIONS`）区分于「exec 失败」，monitor 即可安全 retire——
> **但这要动 daemon（红线：daemon 零行为改动），留 daemon 版本批次**。

⇒ **这次松红线正好解锁它**，而且它是本工作区里唯一「不依赖任何新机制、独立、当天可交付、
立刻修掉一个真 bug」的功能。**排第一个做**（P1）。

### 事实 4：monitor 侧的 16s 不是「帧太慢」，是**对账机制本身是轮询式**

`tmux_reconcile.rs:31` 有编译期断言 `assert!(RETIRE_MISS_THRESHOLD >= 2)`，理由是
`/branch` 漂移有 ~1s 竞态窗（某轮旧 sid A 仍在 `announced_live` 但 backend 已是新 sid B，
threshold=1 会误 retire 还活着的 A）。

⇒ **光把帧推快不解决问题**：那只是把 8s×2 变成 新间隔×2，仍然是「连续缺席计数」这种
轮询式判据。要拿到 ~10ms 就必须换判据：**从「快照差分 + 缺席计数」换成「正向死亡事件」**
——tmux 明确说「叫 X 的这个会话关闭了」，没有抖动可言，不需要 debounce。
这是**唯一需要动 wire 协议**的一步（P5）。

### 事实 5：用户那份调研已经替我们踩掉的坑（直接吃，不重测）

`/home/zbl/文档/claudecode-frontend/2026-07-30-tmux会话存活实时监控-零轮询机制调研.md`：

1. **`session-closed` 里只有 `#{hook_session_name}` 是对的**——`#{session_name}` 报的是
   「当时的当前会话」（实测：死的是 `alpha`，`#{session_name}` 报 `gamma`）
2. **hook 是数组，不带下标会互相覆盖**——必须写 `session-closed[50]`。附带：
   `show-hooks -g` 列的是**槽位名**不是已设 hook（调研作者据此误以为 cc-monitor 装过东西）；
   实测本机已设 hook 数 = **0**，槽位 50 空着
3. **`session-closed` 在「最后一个 pane 自己退出导致会话消失」时也触发**——正是我们要的覆盖
4. **`run-shell` 里堆多层引号会解析坏**（调研第一次测探针根本没起来，被误判成机制失败）
   ⇒ hook 一律只负责调用一个独立可执行文件
5. **测「是否轮询」不能只看 strace 的 syscall 计数**——ptrace 停点会把 level-triggered fd
   反复唤醒造成假忙等（调研看到 39323 次 `epoll_wait`，摘掉 strace 后 `user=0 sys=0`）
6. **`tmux wait-for never` 不要用**：干净退出时 tmux 会先 SIGTERM 掉自己的 `run-shell`
   子进程，探针在返回前就被打死；加 trap 就等价于 PDEATHSIG，白绕
7. 已否决：systemd `.path` 单元（level-triggered、撞 start-limit）· `capture-pane` 轮询
   （13.5ms/轮、1s 间隔 48 CPU-秒/小时）· 控制模式与 `attach -r`（都得当 client，脏
   `tmux ls` / 影响尺寸协商）· `pipe-pane`（全屏 TUI 的原始字节流，ANSI 噪音）

---

## §0.1 目标与范围

**目标**：把 daemon 侧全部判活信号从「定时轮询」换成「内核/tmux 推的事件」，
在**不新增任何定时器**的前提下把带外杀 tmux 的变灰延迟从 ~16s 降到 ~10ms 量级。

### 成功标准（可验证，逐条可勾）

1. **`watch_loop` 里零定时器**：源码中不再出现 `recv_timeout` / `TMUX_EMIT_INTERVAL` /
   `Instant::elapsed()` 节流。终态是**一个无超时的 `recv()`**（见 §3 账本第 1 行）。
   由门禁扫源码钉死（P6），不靠人眼。
   **⚠ 本条有前置条件**：`TMUX_EMIT_INTERVAL`（轮询 B）**只能在 P5 落地后删**，
   因为「杀掉多个会话中的一个」这个场景**只有 hook 知道**（server 还活着 ⇒ 无 pidfd 事件、
   无 socket 事件）。P4 未获授权 ⇒ 本条降级为「只消掉轮询 A」，**如实标注未达成**，
   绝不为了让守卫变绿而先删掉唯一的信号源。
2. **PID 死亡判定从启发式升级为内核级**：不再靠「pid 存在 + procStart 匹配」推断，
   改由 `pidfd` 绑进程实例本身 ⇒ **PID 复用问题在机制上不存在**（不是"检测得更准"，
   是"无从发生"）。这是本工作区唯一一条**正确性**改进，不只是延迟改进。
3. **带外杀 tmux → 端到端变灰有实测数字**，且该数字写进文档时标明是实测还是估算。
   目标量级 ~10ms；**若实测远高于此，如实记录并说明瓶颈在哪一跳**，不许把估算写成实测。
4. **三个盲区如实分类**：① server 复活 → **本工作区解决**（inotify socket 目录）
   ② 「活着但卡死」→ **明确不做、且说清现在的轮询也没在做这个**（`tmux ls` 里卡死的
   CC 照样在，8s 轮询只检"会话不在了"）⇒ 删轮询在这格上零损失
   ③ user manager 整体挂掉 → 机器内部无解，靠 monitor 断连自愈（已有路径）。
5. **旧 daemon / 旧 monitor 混搭不炸**：新增帧走既有 `emits` 声明机制门控消费
   （`wire.rs:41-47` 已有此字段，正是为此设计），**不 bump `PROTO_VERSION`**
   （`main.rs:111` 明写「绝不为 additive 变更 bump，那会把每台旧 daemon 误判」）。
6. **§24 单写者不破**：所有新信号都汇进既有的 `SessionChange{removed}` → `remote_tx` →
   唯一写者 `remote-session-emitter`。**绝不**新增 `remote_active` / `REMOTE_IDLE` 写点
   （`ssh_source.rs::f032_idle_tests::remote_idle_single_writer_guard` 会测红）。
7. **`doc/INVARIANTS.md:408` 那条残留项被销掉**（不是搬走、不是改措辞）。

### 明确不做

- **不做「活着但卡死」检测**（无事件解；要做只能回轮询 pane 内容，方向相反）
- **不改 `TMUX_LS_FMT`**（`watcher.rs:69` ↔ monitor `tmux::TMUX_LS_FMT` 逐字节双写点，在册红线）
- **不改 `RETIRE_MISS_THRESHOLD` 那条 `>= 2` 断言**——快照路径继续用它；
  事件路径**绕过** miss 计数而不是把阈值调小（阈值调小会破坏快照路径的漂移防护）
- **不改 `shared/ccm` 本体**（在册红线）——hook 由 **daemon** 装，不由 ccm 装。
  这不只是为了守红线，也更对：daemon 才知道 server 什么时候（重）起
- **不写 `~/.tmux.conf`、不写任何家目录文件**
- **不装任何软件包**（inotify-tools 不需要，`notify` crate 就是 inotify）
- 不做 monitor 侧新轮询（在册红线，本工作区方向本来就相反）

---

## §1 功能清单

| # | 功能 | 规模 | 需外部授权？ | 状态 |
|---|---|---|---|---|
| **P0** | **五项机制实测**（隔离 socket + scratchpad，唯一可能推翻方向的一步） | 小，全是测 | 否（隔离 socket） | **✅ 完成，已签收** |
| **P1** | daemon `ZeroSessions` 观测分类 + monitor 侧安全 retire（销 `INVARIANTS:408` 残留） | 小 | 否 | **✅ 完成，已签收** |
| **P2** | pidfd 替掉判活轮询 A（含 PID 复用在机制上消失）+ **建统一事件 channel** | 中 | 否 | **✅ 完成，已签收** |
| **P3** | tmux server 生 / 死 / 复活事件化（**不删轮询 B**，见下） | 中 | 否 | **✅ 完成，已签收** |
| **P4** | daemon 装 tmux hook（会话生/死/改名）+ 事件通路 | 中 | **是：改活 tmux server 的 hook 状态** | 未开工 |
| **P5** | wire 正向死亡帧 + monitor 侧免 debounce 立即 retire + **删 `TMUX_EMIT_INTERVAL`** | 中 | 否（承 P4） | 未开工 |
| **P6** | 门禁：零定时器守卫 + 端到端延迟 e2e | 小 | 否 | 未开工 |
| **P7** | 文档：INVARIANTS 新节 + 销 §24bis:408 残留 + E34 结案 | 小 | 否 | 未开工 |

### P0 — 五项机制实测（**门禁：不许凭推测动手**）

全部在隔离 socket（走 tmuxshim，强制 `-L`）+ scratchpad 里做，产出
`features/P0-machine-facts.md`。五项：

**✅ 已完成（2026-07-30）。全部实测证据见 `features/P0-machine-facts.md` §3。
五项里三项是坏答案，其中一项若照直觉写会造成真 bug。**

| 项 | 问题 | **实测答案** |
|---|---|---|
| ① | `session-closed` 里 `#{@ccm_sid}` 能不能展开？ | **坏答案，且比"空"危险**：它解析到**当时"当前会话"**的值。杀 A 拿到 `SID-C`（两次独立复现）⇒ 照直觉写会把**还活着的 C 变灰**。`#{hook_session_name}` 是对的。⇒ 死亡帧**只带名字** |
| ② | per-session hook 可不可行？ | **坏答案**：per-session 机制本身可用（`session-renamed -t A` 对照触发 ✓），但 **`session-closed` 专门不触发** ⇒ 必须用**全局 `[50]`**（已获授权）。E34 期望的"最干净路线"不存在 |
| ③ | daemon 落哪个 cgroup？ | **好答案**：tmux server 在 `session-12881.scope`，每个新 SSH 登录得**新的** `session-<N>.scope` ⇒ 必然不同锅 ⇒ pidfd 扛得住。**⚠ 但这条只对「经 SSH 起」成立，`local-as-remote` L1 必须重判** |
| ④ | `tmux ls` 三态的确切 rc/stdout | **比设想复杂**：是**四**态。`exit-empty off` 下「server 活+零会话」**存在**且 **rc=0 + stdout 空**（Phase D 自审补测出来的）。判据 = **rc + stdout 空否**，不碰 stderr 文本 |
| ⑤ | `notify` 把 inotify 溢出报成什么 | **坏答案**：`notify` 报了（`Flag::Rescan`），但 **`notify-debouncer-mini` 静默吞掉**（`add_event` 只读 `event.paths`，而溢出事件 `paths` 为空 ⇒ 循环体一次不执行）。**但这是既有盲区不是本区引入**，且 **pidfd 对溢出免疫** ⇒ P2 让情况变好 |
| ③' | 附带 | **`run-shell -b` 与同步版的分界**（计划外撞出来的最有价值产出）：`-b` 在「杀掉最后一个会话」时**写不进去**（server 先 SIGTERM 掉自己的 run-shell 子进程）；同步版能写但**会阻塞用户实况 server** ⇒ 选 `-b`，那两格交给 pidfd。`kill-server` 两种都不触发 |
| ③'' | 附带 | **server 死后 socket 文件仍留着** ⇒「文件存在」≠「server 活」；**复活时 inode 变**（unlink+create）⇒ **`IN_CREATE` 能感知**，P3 的复活路成立 |

**⇒ 由 P0 定死的三条设计**（不再是选项）：
1. 死亡帧带 `#{hook_session_name}`，monitor 侧 name→sid 反查（§2 注 2 已论证对漂移安全）
2. hook 用**全局 `[50]` + `run-shell -b`**；「最后一个 / kill-server」两格由 pidfd 覆盖
   ——**hook 与 pidfd 的分界是实测的，不是设计的**，两者不重叠且合起来无缺口
3. `run_tmux_ls()` 返回**四值枚举**而非裸 `String`（见 §3 账本第 2 行）

### P1 — `ZeroSessions` 哨兵（独立、最小、修真 bug）

> **P0 已把本功能的形态定死**：哨兵语义是 `ZeroSessions`（不是 `NO_SESSIONS`），
> 判据是 **rc + stdout 空否**，覆盖三种实测到的零会话形态。

- daemon：`run_tmux_ls()` 现在的 `tmux ls … 2>/dev/null || true` **把 rc 丢掉了**，
  把五种观测压成"空串 / 有内容"两种。改成返回**四值枚举**：

  | 观测 | 分类 | monitor 行为 |
  |---|---|---|
  | `rc=0` + stdout 非空 | `Sessions(raw)` | 按 raw 对账 |
  | `rc=0` + stdout 空（`exit-empty off`） | `ZeroSessions` | **安全 retire** |
  | `rc=1`（server 不在 / socket 不存在） | `ZeroSessions` | **安全 retire** |
  | `command -v tmux` 失败 | `NoTmux` | 跳过（现有行为不变） |
  | 其他 rc | `Unobservable` | 保守跳过 |

- monitor：`ssh_source.rs:2380` 的 `!backend.is_empty()` 门放宽为「`ZeroSessions`
  也是有效观测（空集）」，保守跳过只留给 `NoTmux` / `Unobservable`
- **一处刻意的保守 + 它的升级路径**：把 `rc=1` 判成 `ZeroSessions` 意味着"socket 权限异常"
  也会被判成零会话（理论上可能误 retire）。socket 路径 uid 隔离 ⇒ 同 uid 下几乎不可能。
  **P3 落地后升级**：那时 daemon 持有 server 的 pidfd ⇒「server 活着但 `tmux ls` rc=1」
  = 真异常 ⇒ 归 `Unobservable`。**该升级不改帧契约**，所以 P1 现在就能安全落地
- **销掉 `INVARIANTS.md:408`**（改成"已修 + 修法"），不留"记档待批次"
- 回归纪律：**先写复现的失败测试**（"最后一个 tmux 会话被杀 → 灰灯该清"）再改

### P2 — pidfd 替掉判活轮询 A

- 每个被追踪的 session PID 开一个 `pidfd`，一条极小的阻塞线程 `poll(pidfd, POLLIN)`，
  醒了往**统一事件 channel** 发一条「PID X 实例死了」
- `session_alive(pid, start)` 那套 `procStart` 启发式**退成兜底**（首次开 pidfd 时的一次性校验），
  不再周期跑
- 线程数 = 被追踪 PID 数（实际个位数），在头注里写明这个界
- `pidfd_open` 需 Linux 5.3+ / `libc` 依赖：**新增依赖前先确认** `src-tauri/Cargo.lock`
  里已解析过 `libc`（daemon 的依赖策略是「版本钉在 lock 已解析过的，好离线」，
  `Cargo.toml` 注释明写）。若没有 → 用 `std::os::fd` + `syscall` 手写，零新依赖
- **这一步不动 wire**，纯 daemon 内部改进 ⇒ 可独立验收

### P3 — tmux server 生 / 死 / 复活

- **死**：`pidfd` 绑 tmux server pid（`tmux display-message -p '#{pid}'`）→ 醒了立刻发
  「server 没了」⇒ 等价于零会话
- **复活**：对 socket 目录（`/tmp/tmux-<uid>/`）加 inotify `IN_CREATE` → 感知到 socket
  重新出现 ⇒ 重挂 pidfd + （P4 后）重装 hook + 全量 `tmux ls` 重同步
- **本功能刻意不删 `TMUX_EMIT_INTERVAL`**（轮询 B 留到 P5）。理由：P3 只覆盖
  「server 整体生死」；**「多个会话里被杀掉一个」server 还活着 ⇒ 无 pidfd 事件、
  无 socket 事件，只有 hook（P4）知道**。此时删掉轮询 B 会把该场景从 16s 变成**永不**——
  那是回归，不是优化
- **但 P3 已经让最常见的那种情况变成即时**：杀掉该 origin 上**仅剩的**那个 tmux 会话
  ⇒ server 随之退出 ⇒ pidfd 立刻醒 ⇒ 配合 P1 的 `NO_SESSIONS` 语义安全 retire。
  这正是 `INVARIANTS:408` 记的那个卡到断连才清的场景
- `run_tmux_ls` 无超时那条约束不变——继续跑在一次性线程里
- 多 server / 多 socket（调研 §12 实测本机有 **2 个** tmux server）：本功能只管
  daemon 实际观测的那个 socket，其余如实标注为范围外

### P4 — daemon 装 tmux hook（**需授权**）

- daemon 在「感知到 server 存在」时装 `session-created[50]` / `session-closed[50]` /
  `session-renamed[50]`，hook 只调用**一个独立可执行文件**（不在配置里堆引号，吃调研坑 4）
- 事件通路 —— **2026-07-30 已推翻重定，见下方「§P4 设计修订」**。
  原方案（hook 追加日志文件 + daemon inotify 读增量）**违反红线 I7「daemon 只读」**，
  被 `readonly_guard` 当场拦下。新方案：hook → `<exe> --tmux-notify <pid> <starttime>`
  → 校验身份 → `SIGUSR1` → daemon 重探并与上一份快照差分
- **授权点**：这会改用户活着的 tmux server 的 hook 状态。槽位 50 调研实测空着；
  撤销 = `tmux set-hook -gu 'session-closed[50]'`（逐个 unset）。**要用户明确一句授权。**
- **未获授权时的准确后果**（不许含糊）：P4/P5 跳过 ⇒ 轮询 B（8s）**保留**，
  成功标准 1 只达成一半（轮询 A 消掉）。届时的实际延迟：

  | 场景 | 未获授权（P1-P3） | 获授权（P1-P5） |
  |---|---|---|
  | claude 进程正常退出 | ~0.1s（既有 pidfile inotify，本来就是事件） | 同 |
  | claude 被强杀留下陈旧 pidfile | **~0（P2 的 pidfd）**，原 ≤2s | 同（P2 实测 ~18ms） |
  | 杀掉该 origin **仅剩的** tmux 会话 | **~0（P3 pidfd + P1 哨兵）**，原卡到断连 | 同 |
  | 杀掉多个中的**一个** tmux 会话 | **仍 ~16s**（只有 hook 知道） | **实测 126 ms**（P5 删轮询 B 后复测；P4 后 136/137 ms） |
  | tmux server 复活 | ~0（P3 socket inotify） | 同 |

### P4 设计修订（2026-07-30，被 `readonly_guard` 拦下后重定）

**被拦的是什么**：原方案让 daemon 往 `$XDG_RUNTIME_DIR/cc-monitor/tmux-events.log` **追加**
事件行。`remote-daemon-proto/src/readonly_guard.rs` 扫 daemon 生产源码里的 `fs::` 变更调用，
当场红：「daemon 只读护栏违规（红线 I7）：生产代码 `tmux_hook.rs` 含文件系统写操作
`fs::create_dir`」。

**为什么不是守卫太严**：`doc/INVARIANTS.md` §A2 把这条定得很硬——新增的账号只读查询被明确
框成「**本约的「读」面延伸（澄清，非例外/非松动）**」，并写着「动凭据的部署操作**绝不经
daemon**——那会往只读组件里塞写权限」。追加日志确实是往只读组件里塞写权限。
**放宽它需要用户对红线表态**，不是我能自行决定的。

**重定后的方案（零红线改动，且在另外两点上更好）**：

```
tmux hook (全局 [50], run-shell -b)
   └─> <daemon exe> --tmux-notify <daemon_pid> <daemon_starttime>
          ├─ 读 /proc/<pid>/stat 校验 starttime 相符（挡 PID 复用误伤无关进程）
          └─ kill(pid, SIGUSR1)            ← 无文件系统写
   daemon: tokio SIGUSR1 流（`signal` feature 已启用、main 已在用）
          └─> 经 WatcherPoke 往统一 channel 发一拍 ⇒ 立刻重探
          └─> 与**上一份 tmux ls 快照差分** ⇒ 消失的会话名 = 关闭的那个
```

三条优势（不只是"绕开守卫"）：

| | 原方案 | 新方案 |
|---|---|---|
| daemon 文件系统写 | **有**（违反 I7） | **零** |
| 会话名经 shell 引号 | 有——`"#{hook_session_name}"` 里含 `"` 或 `$(...)` 会破坏命令串甚至注入（原方案只能"接受并登记"） | **名字根本不传** ⇒ 注入面消失 |
| 日志文件 | 要管增长 / 轮转 / 多消费者 | 不存在 |

**代价与处置**：
- 信号无载荷、会合并（多个会话同时关 → 可能只来一次）⇒ **靠"重探 + 差分"天然免疫**
  （差分一次能报出所有消失的会话，比逐条事件更稳）
- **P5 要的名字改由 daemon 自己算**：daemon 得留住上一份 `tmux ls` 快照（现在是发完就忘）。
  纯 daemon 内部状态，一个字段。
- 信号在 daemon 不在时会丢 ⇒ 无所谓：daemon 不在就没有 monitor 在听
- **不新增 `unsafe`**：走 `tokio::signal::unix`（`Cargo.toml` 的 `signal` feature 早已启用，
  `main.rs` 已在用它做停机）；`kill` 那一侧在 hook 子进程里，用 `libc::kill` 一处调用
- `spawn_watcher` 要**返回一个 poke 句柄**（把统一 channel 的发送端以一个窄类型暴露给 main），
  这是 P2 那条「只加事件源、不加定时器」约束的自然延伸

**如果用户更想保留原方案**：那需要给 I7 加一条**窄例外**——写只准落
`$XDG_RUNTIME_DIR`/`/tmp/cc-monitor-<uid>`、**绝不进 `claude_dir`**，并给 `readonly_guard`
加一条新断言把这个边界机器化（那样守卫反而更强：从"任何 fs 写"变成"任何落在观测树内的
fs 写"，范围等于性质）。原方案的完整实现已存成 patch
（`scratchpad/P4-work.patch`，485 行，含单测；当时 145 passed / 只有这条守卫红）。

### P5 — 正向死亡帧（wire）〔**已交付，2026-07-30**。上表「获授权」那列已换成实测值〕

- 新帧 `TmuxSessionClosed { name }`（若 P0-① 拿到好答案则同时带 `sid`），走 `emits`
  声明门控，**不 bump `PROTO_VERSION`**
- monitor：收到即按 name→sid 反查最新快照 → 直接当 `SessionChange{removed}` 送进
  `remote_tx`（唯一写者 emitter 照常分流 idle / archived）⇒ **绕过 miss 计数**
- 快照路径与 `RETIRE_MISS_THRESHOLD >= 2` **原样保留**（重同步 / 旧 daemon 降级都靠它）
- 同一 sid 两条路都可能到 ⇒ retire 必须幂等（`SidTrack.retired` 已是幂等设计，复用）

### P6 — 门禁〔**守卫已交付 2026-07-30**（features/P6-no-timer-gate.md）；e2e 延迟那半已登记未做〕

- **零定时器守卫**：扫 `remote-daemon-proto/src/watcher.rs` 断言无 `recv_timeout` /
  `from_secs` 节流常量。**阈值不能挂在被优化的量上**（rust-ts-boundary 的教训）⇒
  断言的是「扫到的文件数 > 0」+「命中数 == 0」，不是「命中数 < N」
- **端到端延迟 e2e**：隔离 socket 起会话 → 带外 `kill-session` → 量到「monitor 收到
  removed」的墙上时间。
  **⚠ P1 审计订正了载体**：原计划「落进既有 `graylight-daemon-frames` 一族」**行不通**——
  实测那 6 套（`graylight-*` / `restart-*` / `resume-*` + helper `gen-idle-tmux.sh`，正是
  BACKLOG E14 记的"不在 CI 的 6 套"）**一处 `-L` 都没有**，会在**默认 socket** 上建/杀会话。
  ⇒ P6 必须**先给它们做 socket 隔离**（顺带闭合 E41），或改用已隔离的 6 套作载体。
  ~~两条路都比"直接并入"贵，P6 开工时先定这个~~
  **〔2026-07-30 已过时〕**：`gate-integrity` **G-C**（`69e14c3`）已经给那批做了隔离
  （`unset TMUX` + 短 `TMUX_TMPDIR`），**E41 已销**，其中 5 套已进 CI 且自带断言数地板。
  ⇒ **「先做隔离」这条前置不存在了，直接并入 `*-daemon-frames` 一族即可。**
  （记录会过时：这条不是当初写错，是**别的功能把它解决了而计划没同步**。）
- 新套件断言条数进 CI 标签（gate-integrity G-A 的地板纪律，本区遵守不重复实现）

### P7 — 文档

- `doc/INVARIANTS.md`：新增一节「判活信号全部事件驱动」，写清四路事件 + 三个盲区分类
- 销 §24bis:408 残留项；§24 单写者那节补一句「事件路同样只经 emitter」
- `BACKLOG.md` E34 结案（含**对 E34 原措辞的订正**：原文承诺「daemon 零改」+「轮询→事件」，
  实际是「daemon 可改」+「A/B 两条轮询都换」+「『杀掉轮询』字面版会留 2 个盲区」）

---

## §2 架构概览

```
       tmux server                      内核                     文件系统
            │                            │                          │
  hook session-closed[50]         pidfd(session PID)          inotify:
  hook session-created[50]        pidfd(tmux server pid)       - projects/ (递归, 既有)
  hook session-renamed[50]              │                      - sessions/ (平, 既有)
            │                            │                      - /tmp/tmux-<uid>/ (新, IN_CREATE)
            ▼                            ▼                      - tmux-events.log (新)
   独立可执行文件追加一行                阻塞 poll 线程                    │
   $XDG_RUNTIME_DIR/.../tmux-events.log │                              │
            └──────────────┬─────────────┴──────────────────────────────┘
                           ▼
              ┌────────────────────────────┐
              │  统一事件 channel（mpsc）    │   ← 终态：watch_loop 阻塞在无超时 recv()
              └────────────┬───────────────┘
                           ▼
                     watch_loop（一条 OS 线程，零定时器）
                           │
                           ├─ 事件触发 run_tmux_ls（仍在一次性线程，无超时约束不变）
                           ▼
                 Frame::TmuxSessions{raw}      （快照，重同步用，含 NO_SESSIONS 哨兵）
                 Frame::TmuxSessionClosed{..}  （新，正向死亡，免 debounce）
                           │  SSH stdout
                           ▼
                 monitor ssh_source stream_loop
                           │
                           ├─ 快照 → reconcile_step（miss 计数 + threshold≥2，原样）
                           └─ 死亡帧 → 直接 SessionChange{removed}
                                          │
                                          ▼
                          remote_tx → remote-session-emitter（§24 唯一写者，不变）
```

**注 1：为什么不用 unix socket / FIFO 当事件通路。**
FIFO 在无读者时写会阻塞（`run-shell -b` 虽是后台、但会攒住进程）；unix socket 要 daemon
先在、否则事件丢。追加日志 + inotify 是调研**已实测 0% 空闲 CPU** 的那条，而且天然多消费者
（同主机多个 monitor 各连一个 daemon 时不互相抢）。

**注 2：为什么「死亡帧只带 tmux 会话名」也安全。**
若 P0-① 证明 `#{@ccm_sid}` 在 `session-closed` 里取不到，就带名字，monitor 用最新快照反查。
`/branch` 漂移改的是「同一个 tmux 会话内的 sid」——A 漂到 B 之后这个 tmux 会话关闭时，
**A 和 B 都已经死了**，按名字 retire 不会误杀活着的会话。这正是名字路径比"缺席计数"更强的地方。

**注 3：为什么 hook 由 daemon 装而不是 ccm 装。**
守「不改 `shared/ccm` 本体」红线是其一；更实质的是**只有 daemon 有「server 重启」这个事件**
（socket 目录 inotify），而 hook 活在 server 内存里、每次 server 起来都要重装。
ccm 只在建会话时被调用一次，根本没有那个时机。

---

## §3 ★共享面账本

| # | 共享面 | 谁改 | **最终形态**（所有功能做完后应长成的样子） |
|---|---|---|---|
| **1** | **`remote-daemon-proto/src/watcher.rs::watch_loop`** | **P1 P2 P3 P4** | **一个阻塞在无超时 `recv()` 上的循环，消费单一 `mpsc<WatchEvent>` channel。** 零 `recv_timeout`、零 `Instant::elapsed()` 节流、零 `Duration::from_secs` 常量。所有事件源（notify / pidfd 线程 / tmux 事件日志）都往这一个 channel 发。**禁止**任何功能自己再挂一条独立线程+定时器 —— 那正是"补丁叠补丁"。P2 是第一个改它的，**P2 就要把 channel 统一起来**，后面三个功能只往里加事件源 |
| **2** | **`run_tmux_ls()` 的返回契约** | **P1 P3** | **四值枚举（P0 定死，不是裸 `String`）**：`Sessions(raw)` / `ZeroSessions` / `NoTmux` / `Unobservable`。**P1 建立它**，P3 只加调用时机与内部判据（pidfd 收紧 `Unobservable`），**不改这个契约** ⇒ 后面不会有人为新场景再往里塞第五种哨兵字符串。`TMUX_LS_FMT` 逐字节不动（红线） |
| **3** | **`wire.rs::Frame` + `main.rs::EMITS`** | **P1**（哨兵在 `TmuxSessions.raw` 里，不新增帧） **P5**（新增帧） | additive-only：新帧必须 `skip_serializing_if` 或走 `emits` 声明，**永不 bump `PROTO_VERSION`**。EMITS 登记 = 承诺真发（`main.rs:133` 已有此纪律） |
| **4** | **`ssh_source.rs` 收帧臂（`stream_loop` ~2367）** | **P1 P5** | 快照臂与死亡帧臂**并存且正交**：快照走 `reconcile_step`（threshold≥2 原样），死亡帧走直接 removed。两条都只往 `remote_tx` 送 `SessionChange{removed}` ⇒ §24 单写者不变。**不得**为死亡帧新增 `remote_active`/`REMOTE_IDLE` 写点 |
| **5** | **`doc/INVARIANTS.md` §24 / §24bis** | **P1 P5 P7** | §24bis:408 残留项**销掉**（P1）；新增一节描述四路事件 + 三盲区（P7）。P5 在 §24 那节补一句"事件路同样只经 emitter"。**不重写既有条款**，只加与销 |
| **6** | **`.github/workflows/ci.yml`** | **P6** | 只**追加**（不重排他人步骤）——§3 跨工作区协议，C05 与 gate-integrity G-B 已在遵守 |
| **7** | **`e2e/` graylight 一族** | **P6** | 延迟测量并入既有套件，**不新建套件**；断言条数进 CI 标签 |

### 跨工作区冲突协议

- 本区与 `gate-integrity` 同时会碰 `ci.yml` ⇒ 都只追加，后到者不重排先到者
- 本区与 `local-as-remote`（L1 本地=不走 ssh 的远端）会碰同一个 daemon ⇒
  **本区先落地**（L1 尚未开工），L1 届时继承事件模型而不是继承轮询
- 本区**不碰** `tabs.ts` / `accounts.ts` / `shared/ccm` / `src-tauri/vendor/`

---

## §4 依赖图与实现顺序

```
P0（实测）─────┬──► P1（NO_SESSIONS，独立可先做，但要 P0-④ 的 rc 事实）
              ├──► P2（pidfd 判活；建统一 channel）──┐
              ├──► P3（server 生死复活；删 8s）──────┼──► P6（门禁 + 延迟 e2e）──► P7（文档）
              └──► P4（装 hook，需授权）──► P5（死亡帧）┘
```

| 顺序 | 功能 | 为什么在这个位置 |
|---|---|---|
| 1 | **P0** | 唯一可能推翻设计的一步。五项里有三项（①②⑤）会改后面的形态 |
| 2 | **P1** | 独立、最小、修一个**预先登记的真 bug**、不依赖任何新机制 ⇒ 先拿一个干净检查点 |
| 3 | **P2** | **它建 §3 账本第 1 行的统一 channel**。必须在 P3/P4 之前，否则那两个各挂一条线程就是补丁叠补丁 |
| 4 | **P3** | server 级事件。依赖 P2 的 channel + P0-③ 的 cgroup 事实。**刻意不删轮询 B** |
| 5 | **P4** | 需授权。未获授权则跳过（后果见 §1 P4 那张延迟表） |
| 6 | **P5** | 承 P4（没有 hook 就没有死亡事件）。唯一动 wire 的一步。**删轮询 B 在这里**——只有到这一步才存在覆盖「多个中杀一个」的事件源 |
| 7 | **P6** | 门禁必须在机制稳定后钉，否则钉的是中间态 |
| 8 | **P7** | 文档最后写，写的是实际落成的样子（不是计划的样子） |

---

## §5 横切关注点与约定

**测试**：沿用既有门禁，不新搭。daemon 侧 `cargo fmt --check` + `clippy --all-targets` +
`cargo test`（CI 有独立 job，现 47 个测试）；monitor 侧 `cargo test --all` + `tsc` + `npm` +
8 套真机 152 条。**门禁一律 `set -o pipefail` + 输出落文件后 grep 核实**（裸管道曾掩盖
cargo 编译失败、误报全绿）。

**判色**：「我这次判色依赖的那个东西，是不是本次变异产的」。① diff 确认落位
② 判据是运行时行为时必须先确认编译过 ③ 判据是扫源码的守卫时编译状态无关。

**变异验收**是每个功能 DoD 的硬项：机制类改动尤其容易"看起来对了"——
pidfd 那步必须做「杀掉被追踪进程 → 事件真的到了」和「不杀 → 事件不到」双向。

**真机测试纪律（本区特别重）**：本区**必须**碰 tmux。
- tmux 一律走 shim（`/tmp/claude-1000/-home-zbl----claudecode-frontend/9d66c46d-bf88-4f99-877e-459680e35a8e/scratchpad/tmuxshim`，无 `-L`/`-S` 一律 rc=97）
- **裸 `tmux kill-server` 是禁用词**
- 起飞前 canary 双向自检 + 跑完逐字核对默认 socket 会话清单未变
- 默认 socket 上住着 `cc-9d66c46d`（我自己）/ `cc-d7692cdf` / `cc-7c2a26d6`
- **绝不启动真实已认证的 `claude`/`codex` 子进程**；要"像 claude 的进程"就用 fake claude
- P4 装 hook 那步（唯一会碰真实况的）：先备份 `tmux show-hooks -g` 全文，改完复核，
  测完 `set-hook -gu` 逐个撤销并再次核对

**commit**：一功能一 commit；message 走 heredoc（`-F -` + `<<'EOF'`），**绝不 `-m` 带反引号**；
不加 `Co-Authored-By`；显式 `git add` 文件清单，绝不 `git add -A`。

---

## §6 风险与开放问题

| # | 风险 | 处置 |
|---|---|---|
| 1 | **P0 拿到坏答案**（cgroup 同锅 / `notify` 吞溢出 / `@ccm_sid` 取不到） | 每项都已预写降级路径（见 P0 表最后一列）。**P0 后强制回 Phase F 修计划** |
| 2 | **hook 装在用户活着的 tmux server 上** | 需用户明确授权；槽位 50 实测空；备份 `show-hooks -g` + 测完撤销 + 复核 |
| 3 | **inotify 队列溢出 = 静默漏事件** | **P0 实测：`notify` 报了但 `notify-debouncer-mini` 静默吞掉**（`add_event` 只读 `event.paths`，溢出事件 `paths` 为空）。**范围订正——这是既有盲区，不是本区引入**：今天那条 2s tick 只扫内存里的 `state.sessions`、不重扫目录；目录发现本来就只靠 notify 事件、本来就没有周期兜底。且 **pidfd 对溢出免疫**（内核直通）⇒ **P2 让情况变好**。处置：登记独立 BACKLOG 项；P3 可选加 raw-`notify` 溢出哨兵。**绝不为它补定时器** |
| 4 | **多 tmux server / 多 socket**（本机实测 2 个） | 明确范围：只管 daemon 观测的那个 socket，其余标为范围外 |
| 5 | **pidfd 线程数随会话数增长** | 实际个位数；在头注写明界；若将来爆量再改 epoll 多路复用（不在本区） |
| 6 | **旧 daemon × 新 monitor / 新 daemon × 旧 monitor** | `emits` 门控 + additive-only + 不 bump `PROTO_VERSION`；两个方向各写一个测试 |
| 7 | **「性能最佳」被理解成"没有兜底"** | 事件驱动的兜底不是定时器，是**重同步触发器**（连接建立 / server 复活 / inotify 溢出）。这条要写进 INVARIANTS，否则下一个人会补定时器 |
| 8 | 用户那份调研的 `~/.tmux.conf` / systemd unit / python 探针**一个都不落地** | 本区全部改动在仓内。调研文档在 P7 里被引为一次性事实来源，不作为落地方案 |
| 9 | **「为了让零定时器守卫变绿而删掉唯一信号源」**——这是本区最像会自己犯的错（Phase A 初稿就把删轮询 B 排在 P3，会让「多个中杀一个」从 16s 变永不） | 删轮询 B 的前置条件写死在成功标准 1 + §1 P3 + §4 顺序表**三处**。P6 的守卫**必须**在 P5 之后才钉；P4 未获授权时守卫要按「只消掉轮询 A」的形态钉，**不许把断言写松**去迁就 |

---

## §7 变更记录

| # | 日期 | 改了什么 / 为什么 |
|---|---|---|
| 03 | 2026-07-30 | **P0 交付，四处设计被实测收紧**：① P0-① 的"好答案分支（帧带 sid）"**被证伪**——`#{@ccm_sid}` 在 `session-closed` 里解析到**别的会话**，照直觉写会把活着的会话变灰 ⇒ 只能带名字 ② P0-② 的"per-session 干净路线"**被证伪**（对照实验：per-session 机制可用，但 `session-closed` 专门不触发）⇒ 必须全局 `[50]`，用户已授权的正是需要的那条 ③ §6 风险 3 **范围订正**：inotify 溢出是**既有盲区**（debouncer 吞掉），且 pidfd 免疫 ⇒ P2 让情况变好，不是本区引入的风险 ④ P1 哨兵从 `NO_SESSIONS` 改为 **`ZeroSessions`**、判据改为 **rc + stdout 空否**（Phase D 自审补测发现 `exit-empty off` 下"server 活+零会话"存在且 rc=0）。另**计划外最有价值的产出**：`run-shell -b` 在「杀掉最后一个会话」时写不进去 ⇒ **hook 与 pidfd 的分界从此有实测依据**（hook 管"多个中杀一个"，pidfd 管"杀到没了/server 被端"），并给 `local-as-remote` L1 留下"cgroup 隔离只对经 SSH 起成立"的提醒 |
| 07 | 2026-07-30 | **P4 设计被 `readonly_guard` 当场推翻并重定（未落代码）**。原方案让 daemon 追加事件日志文件 ⇒ 撞红线 I7「daemon 只读」，守卫报「生产代码 `tmux_hook.rs` 含 `fs::create_dir`」。`INVARIANTS §A2` 把这条定得很硬（新增只读查询被明确框成「读面延伸，非例外非松动」；「动凭据的部署操作绝不经 daemon——那会往只读组件里塞写权限」）⇒ **放宽它需要用户对红线表态，不是我能自行决定的**。重定为 **hook → `<exe> --tmux-notify <pid> <starttime>` → 校验 /proc 身份 → `SIGUSR1` → daemon 重探 + 与上一份快照差分**：daemon 文件系统写**归零**、**会话名不再经 shell 引号**（原方案那个 `"` / `$(...)` 注入面直接消失、不必再「接受并登记」）、不需要管日志增长。代价：信号无载荷且会合并 ⇒ 靠「重探+差分」天然免疫，且 P5 要的名字改由 daemon 自己算（留一份上次快照，纯内部状态）。**不新增 unsafe**（`tokio::signal` 的 `signal` feature 早已启用）。原方案完整实现（485 行、含单测、当时 145 passed 仅这条守卫红）已存 `scratchpad/P4-work.patch`，**若用户宁愿给 I7 加窄例外**（写只准落运行时目录、绝不进 `claude_dir`，并把这个边界加进守卫 ⇒ 守卫从「任何 fs 写」变成「任何落在观测树内的 fs 写」、范围等于性质）则可换回 |
| 06 | 2026-07-30 | **P3 交付：用户调研那个 ⚠ 盲区（server 复活）已消**——在 daemon 里 `notify` 就是 inotify，本功能只是多一个 watch 目标 + 精确路径过滤。**实测**：`kill-server` → `zero_sessions` 帧 **27ms** · 复活 → 含新会话的帧 **153ms**（含 100ms DEBOUNCE_MS）· **跨 cgroup 整锅 SIGKILL → 30ms**（用真 daemon：tmux server 在 `app.slice/…`、daemon 在 `tmux-spawn-….scope`，transient unit 已回滚零残留）。三条设计要点：① `TmuxObservation` 细分为 `ServerEmpty`/`NoServer`，但**两者映射到同一 wire 取值** ⇒ 帧契约逐字节不变（P0/P1 的预判兑现，有测试钉住）② 收紧 P1 的 rc=1 判据时**刻意不依赖「pidfd 是否醒过」**——那会在 pidfd 路失效时把 rc=1 永久压成 `Unobservable` ⇒ 永不 retire；改成直接查 `/proc` 里 server pid 还在不在（一次存在性读，无挂死风险），变异 A 专门钉住这条 ③ 复活必须监视 socket **所在目录**而非文件（要感知的是「被重新 create」）。**账本第 1 行的证据**：P3 只加了两个 `WatchEvent` 变体 + 两个发送方，**零新定时器、循环结构未动** ⇒ P2 把结构做对了、P3 就便宜。给 P4 的现成时机：`ServerState::Alive(pid)` 那个臂正是「该（重）装 hook」的点（hook 活在 server 内存里、每次起来都要重装，而 P3 把「server 起来了」变成了事件）|
| 05 | 2026-07-30 | **P2 交付：账本第 1 行到最终形态**（无超时 `recv()` + 单一 `mpsc<WatchEvent>`；轮询 A 消失、轮询 B 从主循环搬进独立 ticker 线程 ⇒ P5 删 ticker 即可、不必再动循环结构）。**端到端实测：杀掉会话进程 → `session_removed` ~18ms**（原 2s tick ⇒ 降两个数量级；测法含 grep 轮询开销、是上界）。三条回写：① **P6 的载体问题 P2 顺手解决了**——冒烟用的「隔离 `CLAUDE_CONFIG_DIR` + PATH 前置假 tmux + 读 daemon stdout 帧」模式**根本不需要任何 tmux socket**、天生隔离 ⇒ daemon 侧延迟 e2e 照这个建，不必先改那 6 套（E41 只剩真 tmux 那半边要管）② P3 给 tmux server 挂 pidfd 只是多一个调用点 + 一个变体，不必再写 unsafe ③ **P5 硬前置**：删 ticker 前必须把写端关闭接到 `WatchEvent::Shutdown`，否则 reader 不再「没人听就停读」（变体与注释已备好）。**自查出并补掉一个静默回归**：初版把事件 channel 建在 Phase 2，而 Phase 1 初始扫描已在调 `process_session_added` ⇒ 启动时就活着的会话一个 pidfd 看守都没有（原 2s 轮询覆盖它们 ⇒ 是回归）。它不是被测试抓到的、是被 clippy 的「field `start` is never read」间接暴露 ⇒ 补了一条扫源码守卫钉住注入点必须早于扫描锚点 |
| 04 | 2026-07-30 | **P1 交付**（销掉 `INVARIANTS:408` 那条 2026-07-25 就登记、明文卡在「daemon 零改」上的真 bug）。两条实质影响回写：① **P6 的载体作废重定**——实测那 6 套非 CI 套件（`graylight-*`/`restart-*`/`resume-*` + helper）**一处 `-L` 都没有**、会在默认 socket 上建/杀会话 ⇒ 原计划「并入既有 graylight 一族」行不通，P6 得先做 socket 隔离（登记 E41）或换已隔离的载体。旁证：`graylight-daemon-frames.sh:30` 那个 keepalive 会话的存在理由**就是**绕开 P1 修的这个 bug ⇒ 该 bug 当年是被测试侧绕过而非被发现 ② **P3 有了明确的收紧对象**（`rc=1` 判 `ZeroSessions` 是刻意保守，P3 持 pidfd 后可把「server 活着但 rc=1」归 `Unobservable`，且不改帧契约）。另：`BUILD_ID` **刻意不在 P1 bump**（推到 P5 一次重部署覆盖整个工作区，避免让用户每台远端被强制重装两次）⇒ 本修复在远端**休眠**，已记进 STATUS；基线订正 daemon 测试数 47→125 |
| 02 | 2026-07-30 | **交审前自查改掉一处顺序缺陷**：初稿把「删 `TMUX_EMIT_INTERVAL`」排在 P3，但 P3 只覆盖 server 级生死——「多个会话里被杀掉一个」server 还活着 ⇒ 无 pidfd/无 socket 事件，**只有 hook（P4）知道**。那样删会把该场景从 16s 变成**永不**。删轮询 B 移到 P5，并在成功标准 1 / §1 P3 / §4 顺序表三处写死前置条件，另加 §6 风险 9 |
| 01 | 2026-07-30 | Phase A 落盘。范围比 E34 登记的**大一格**（发现 daemon 有 A/B **两条**轮询，E34 只盯了 tmux 那条）；架构比 E34 登记的**不同**（E34 写"daemon 零改"，用户已松该红线，且零改做不到正向死亡帧 ⇒ 16s 只能降到「新间隔×2」）；比用户调研的落地成本**低**（`notify` crate 自带 inotify ⇒ 调研的 ⚠ 盲区消失；daemon 自持 pidfd ⇒ 不要 systemd unit 与 python）。顺带把 `INVARIANTS:408` 那条"卡在 daemon 零改上"的预登记残留纳为 P1 |
| 08 | 2026-07-30 | **P4b→P7 交付，本区收官**。P4b 装 hook + SIGUSR1 通路（真机探针法实测通路打通 + PID 复用防御成立）；**同一个自指陷阱连踩七次**，其中一次让守卫成了安慰剂——是变异揪出来的，并定下「不许为自己方便去改红线守卫」。P5 三步全交付：快照差分（三态，**观测失败绝不当「都没了」**）+ `TmuxSessionClosed` additive 帧 + 删轮询 B 并接 `Shutdown`；**差点顺手删掉「首轮立即发一拍」**（ticker 还兼着初探职责）⇒ 换成一次性 `initial_tmux_probe()`。P6 两半：护栏（**判据落在「周期性唤醒」而非「出现 `Duration`」**，登记表不是豁免清单）+ 延迟 e2e 并进 `graylight-daemon-frames`（5→8），**并撞出一个 P5 留下的真回归**（对照组确认非新引入：第 2 段靠 ticker 重发快照，ticker 删了它就红；那 6 套是 CI-only，P5 的门禁扫不到）⇒ 新纪律「删周期性信号时还要跑依赖那个节拍的 e2e」。P7 文档收口 + E34 结案，**并抓出 P5 漏掉的 `BUILD_ID` bump**（改不改都全绿，漏掉则整轮工作在已部署远端休眠）⇒ `p1r-event-liveness`。**三条红线全程未破**：`TMUX_LS_FMT` / `RETIRE_MISS_THRESHOLD >= 2` / `shared/ccm` 一字未动 |

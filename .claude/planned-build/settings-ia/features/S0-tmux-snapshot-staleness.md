# S0 — 修「/branch 后原 tab 永久灰点、杀不掉」

用户 2026-07-30 实测报障：「执行 branch 后，原本的 tab 不会变灰，而是变成灰点也杀不掉；
而且进去终端后还会莫名其妙变绿」。

## §0 根因（读码坐实，非猜测）

`/branch` 的机制（用户原话）：「输入 branch 之后直接进入 branch，等于说在 tmux 内的 claude
resume 到了另外一个历史记录」。落到实现上：

1. claude **进程不变**（`shared/ccm` 末尾是 `exec`，pid 不变），只是重写
   `~/.claude/sessions/<pid>.json` 里的 `sessionId`。
2. daemon 的 inotify 命中该文件 → `process_session_added` 走「同 pidfile 原地换 sid」分支
   （`watcher.rs:1187`，注释里逐字写着这条是给 `/clear` 等重写 sessionId 用的）
   → `retire_sid_if_unreferenced(旧sid)` → 发 `SessionRemoved{旧sid}`。
3. monitor 的 emitter 收到 removed，调
   `classify_removed(find_tmux_origin_for_sid(旧sid))`（`lib.rs:620`）。
4. `find_tmux_origin_for_sid` 查的是 **`tmux_raw_registry` 里缓存的那份 `tmux ls` 原文**，
   按 `@ccm_sid` 列匹配。命中 ⇒ `Idle{origin}` ⇒ **灰点**（`mark_idle` + `SESSION_IDLE`，
   **不 forget**）。

第 4 步为什么会命中一个已经不存在的 sid —— **两条独立原因，各自都足以致命**：

- **(a) 快照永久陈旧。** `tmux_raw_registry` 只在收到 `TmuxSessions` 帧时刷新。
  P5（zero-poll-liveness）删掉 8s ticker 后，该帧只由**四条事件路径**触发：
  pidfile inotify+pidfd · socket 目录 inotify · tmux hook(`session-created` /
  `session-closed` / `session-renamed`) → `--tmux-notify` → SIGUSR1。
  **`/branch` 一条都不碰**：会话没建、没关、没改名，socket 没动。
  ⇒ 缓存停留在 branch 之前那一刻，里面的 `@ccm_sid` 还是旧 sid，**永远不会被纠正**。
  **这是我 P5 自己的回归，与 P6 抓到的那条同类。**
- **(b) 即使刷新了也可能仍旧。** `@ccm_sid` 由 `shared/ccm` 里那个 **1 秒 poller** 回填
  （`shared/ccm:612` 注释自陈它就是为了「随 /branch 漂移」而存在）。pidfile 变化与
  tag 更新之间有**最长 1s 的窗口**。所以主计划原本写的「pidfile 变化时顺带重探 tmux」
  **单独不成立** —— 它很可能正好探在窗口里、又抓回旧 tag。

「杀不掉」：灰点 tab 走 idle 分支后**不 forget 绑定**，其 kill/attach 路径按旧 sid 去
`@ccm_sid` 命中 tmux 会话，而真实 tmux 上那一格现在挂的是新 sid ⇒ 永远匹配不上。

「进终端后又变绿」：**尚未解释清楚，不当作已知**。attach 会触发 tmux hook（可能是
`session-renamed`，ccm 有 `set-titles`）⇒ 来一帧新 `TmuxSessions` ⇒ 状态被重算。
本功能不声称修它；DoD 里单列一条观察项。

## §1 DoD

- [x] `/branch`（同 pidfile 原地换 sid）后，**旧 sid 归档而不是变灰点**
      —— e2e `graylight-daemon-frames.sh` 1bis 节，真 inotify 链路，实测帧带 `cause=superseded`。
- [x] 真「claude 死了但 tmux 还在」的场景**仍然是灰点**
      —— 同套件第 2 节新增对照断言：真死的帧**不带任何 cause**；
      单测 `superseded_always_archives_…` 里也有一条同输入 `Gone` 仍判 Idle 的反向对照。
- [x] 判据不依赖 `@ccm_sid` 何时被 ccm 的 poller 更新 —— `Superseded` 分支**根本不读快照**。
      e2e fixture 里压根没有那个 poller（标签一直是旧 sid），断言照样绿，正说明这一点。
- [x] 不新增任何定时器（`no_timer_guard.rs` 绿，daemon 176 测全绿）。
- [x] 双写点同步（`removal_cause_wire_literal_stays_in_sync` 读 daemon 源文件锚定）
      + `BUILD_ID` bump 到 `p1t-removal-cause`。
- [x] 旧 daemon × 新 monitor 向后兼容 —— `parses_session_removed` 钉住「缺字段 ⇒ Gone」。
- [x] 变异验证：6 条（见 §6），逐条改坏见红、还原见绿。
- [x] 观察项已登记 —— BACKLOG **E43**（「进终端变绿」成因未查清，S0 可能顺带消除但不声称已修）。

**不做**：不改 `shared/ccm`（它已经在正确地做它该做的事，1s 延迟是它的设计）；
不给 daemon 补任何轮询；不动 `TMUX_LS_FMT`；不改 `RETIRE_MISS_THRESHOLD`。

## §2 方案

**核心洞察：daemon 本来就分得清「死了」和「换了 sid」——它们是两个不同的调用点。**
今天这个信息在发帧时被丢掉了，monitor 只好去猜（猜的方式就是查那份会陈旧的快照）。
把信息补上，monitor 就不用猜。

### 2.1 daemon：`SessionRemoved` 带上 cause

```rust
pub enum RemovalCause { Gone, Superseded }   // 线上：缺省 / "superseded"
Frame::SessionRemoved { sid, cause }
```

| 调用点 | 语义 | cause |
|---|---|---|
| `process_session_added` 「原地换 sid」（`watcher.rs:1187`） | /branch、/clear | **`Superseded`** |
| `process_session_added` 「kind 翻成非 interactive」（`:1143`） | 交互会话没了 | `Gone` |
| `process_session_removed`（pidfile 被删） | 真死 | `Gone` |
| `WatchEvent::PidDied` | 真死 | `Gone` |

线格式 additive：`Gone` 不写字段（旧 monitor 原样工作）。

### 2.2 monitor：`Superseded` 直接归档，不查快照

`SessionChange.removed: Vec<String>` → `Vec<RemovedSid{sid, cause}>`。
选它而不是加第 4 个字段：`removed: vec![]` 的构造点（多数）**一字不用改**，
只有真正塞 sid 的那几处要动；也不引入新的全局可变注册表。

`classify_removed(tmux_origin, cause)`：`Superseded` ⇒ `Archive`（**不看快照**）；
`Gone` ⇒ 维持今天的 `Some→Idle / None→Archive`。

### 2.3 顺带：pidfile 的 sid 变化时重探 tmux

单独不足以修本 bug（见 §0(b)），但**能让快照更新鲜**、且是纯事件驱动零定时器。
去重复用现有 `tmux_inflight`。触发条件收紧到「**这个 key 的 sid 真的变了/新增/消失**」，
不是「任何 .json 事件」——后者会变成变相轮询。

## §3 共享面账本对照

| 共享面 | 本功能怎么动 | 最终形态 |
|---|---|---|
| daemon↔monitor 线协议 | `session_removed` 加可选 `cause` | additive，缺省=旧语义 |
| `BUILD_ID` | bump | 每轮改 daemon 必 bump（P5 漏过一次，P7 pre-flight 抓到） |
| `SessionChange` | `removed` 换元素类型 | 带 cause 的结构体，不再是裸 sid |

## §4 步骤

1. daemon：`RemovalCause` + `Frame::SessionRemoved.cause` + 四个调用点分派 + wire 序列化。
2. daemon：sid 真变化时触发 tmux 重探（复用 `tmux_inflight` 去重）。
3. daemon：`BUILD_ID` bump。
4. monitor：解析可选 `cause`（缺省 `Gone`）。
5. monitor：`SessionChange.removed` 换类型，改动波及点逐个过。
6. monitor：`classify_removed` 加参数 + 纯函数单测（含「Superseded 不看快照」）。
7. 跨语言双写点守卫：cause 字面量两侧一致（照 `TMUX_LS_FMT` 那条纪律）。
8. 变异验证 + 全门禁 + daemon frames e2e。

## §5 测试策略

- 纯函数：`classify_removed` 四象限（cause × tmux_origin）。
- daemon 单测：原地换 sid ⇒ 发出的帧带 `Superseded`；pidfile 删除 ⇒ 不带。
- 线协议：round-trip + 缺字段解析成 `Gone`（向后兼容）。
- e2e：照 `graylight-daemon-frames.sh` 的做法造 pidfile、原地改 sessionId，看帧。

## §6 代码审计结果（Phase D）

**做法如实记**：本轮 **未开并行审计 agent**（本会话的运行约束禁止主动起 agent），
改为主线程逐点自审 + 变异验证兜底。强度低于 planned-build 对高风险功能的规定，**记在这里**。

自审发现并当场修掉一处真问题：

- **`sid_drifted` 在探测在途时被清掉 ⇒ 丢信号。** 初版写成
  `if sid_drifted { sid_drifted = false; if !tmux_inflight { …起探测… } }`。
  在途的那次探测是在漂移**之前**发起的，它带回来的快照照样是旧的 ⇒ 这一拍白丢。
  改成**只在真的起了探测时才清标志**。

逐点复核过、判定无问题的：

- 引用计数语义未变（`retire_sid_if_unreferenced` 仍是「没有别的 pidfile 持有它」才发帧）。
- `/clear` 与 `/branch` 走同一分支、同样是 `Superseded`，语义正确（旧 sid 都是被顶替）。
- 新 daemon × 旧 monitor：旧 monitor 只读 `sid`，多出来的 `cause` 字段被忽略 ⇒ 兼容。
- 旧 daemon × 新 monitor：缺字段 ⇒ `Gone` ⇒ 与今天行为一字不差。
- 未知 `cause` 取值 ⇒ 降级到 `Gone`，**方向是保守判活**：归档是破坏性的（forget 绑定 + 关 tab），
  宁可多留一个灰点，也不能凭一个不认识的词把还活着的会话关掉。
- 本地路径（`session_map::diff_sessions`）一律 `Gone`：本地没有 idle-tmux 灰点
  （`SESSION_IDLE` 是远端专有），cause 在那边无分支意义。

**变异验证（5 条单测 + 1 条 e2e，逐条改坏见红、还原见绿）**：
① daemon 漂移点误发 `Gone` · ② `Gone` 也上线（破坏 additive）· ③ daemon 变体改名（双写点漂）·
④ monitor 丢掉 `Superseded` 早返回 · ⑤ 重探触发退化成无条件 · ⑥ e2e：①的端到端形态。

## §7 工程审计结果（Phase E）

- **文档-代码漂移，已修**：`doc/INVARIANTS.md` §41.3 原本只列三个盲区，而
  「已存在会话上 `@ccm_sid` 变了」**四路事件一个都不响** —— 这是个真盲区，P5 删掉 ticker
  之后从「8s 兜住」变成「永久」。已补为盲区 ④，连同修法（不补事件路径，改成让 monitor
  不再需要这份快照）一起写清。§24bis 第 2 条补了 `cause` 先于快照裁决这道前置。
  §41.1 补了第五个触发条件（sid 漂移 ⇒ 顺带重探），并写明它**不是新事件源**。
- **CI 断言地板**：`graylight-frames` 8→12。顺带补上 **E42 那轮漏改的** `usage-probe` 7→9
  ——地板是 `>=`，不改不会红，但那两条新断言就此没人管着（正是那条守卫要防的病）。
- **对后续功能的影响**：`SessionChange.removed` 换了元素类型，S1-S10 都不碰这条路，无牵连。
- **部署面**：bump 了 `BUILD_ID`（`p1t-removal-cause`）⇒ `build.rs` 会警告内嵌 daemon 比源码旧。
  发版走 CI 交叉编译重出内嵌二进制，本地不需要处理；**但真机上的 daemon 必须重新部署**，
  否则 monitor 侧的新逻辑永远收不到 `cause`（退回 `Gone` = 灰点 bug 依旧）。

## §8 签收

- [x] 过代码审计（自审 + 变异；强度不足之处已在 §6 声明）
- [x] 过工程审计
- [x] 主计划已更新

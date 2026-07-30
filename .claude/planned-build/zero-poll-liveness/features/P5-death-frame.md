# P5 — 正向死亡帧 + 删轮询 B（**三步全部交付**）

> 主计划：`../MASTERPLAN.md` §1 P5 · §3 账本 · §4 顺序表第 6 位 · 铁律 9
> 前置：**P4**（`3ccb6d6`）+ P5 前置实测（`d9f464c`：136ms vs 5042ms，带对照组）

## 1. P5 是三步，全部交付

| 步 | 内容 | 状态 |
|---|---|---|
| 1 | daemon 留住上一份 `tmux ls` 快照 + 差分 | **✅ 本轮** |
| 2 | 新帧 `TmuxSessionClosed { name }` + monitor 消费 | **✅ 本轮** |
| 3 | 删 `TMUX_EMIT_INTERVAL`（轮询 B）+ 接 `Shutdown` | **✅**（单独走了一圈，见 §6） |

## 2. 为什么是「差分」而不是「逐事件」

SIGUSR1 **无载荷且会合并**：一串 hook 同时打进来，daemon 可能只醒一次。
逐事件必漏；差分则一次能报出**所有**消失的会话。

三态语义，最要紧的是第三条：

| 观测 | 现集 | 结论 |
|---|---|---|
| `Sessions(raw)` | raw 里的名字 | 消失 = 旧集 − 现集 |
| `ServerEmpty` / `NoServer` | 空 | 旧集里**全部**算消失 |
| `NoTmux` / `Unobservable` | —— | **「不知道」，绝不当作「都没了」**；快照原样保留 |

> 第三条是 P1 那条教训（空 `raw` 同时意味着「零会话」和「出错」）**在死亡帧这条路上的复发点**。
> 观测失败时报一堆死亡帧，会把活着的会话全部误 retire。有专门的测试钉住。

另外两条边界：**第一次观测不报死亡**（否则 daemon 一启动就诬告一批）；
**同一份观测重复到达不重复报**（差分是有状态的，天然幂等）。

## 3. wire：additive，旧 monitor 安全

`TmuxSessionClosed { name }` 进 `EMITS`（`tmux_session_closed`），
**不 bump `PROTO_VERSION`**。

**兼容性不是假设、是核实过的**：monitor 遇未知 kind 走 `warn` 后跳过
（`ssh_source.rs:2428`，且既有测试 `unknown_kind_returns_none` 钉住）⇒ 旧 monitor
行为退回今天的「快照 + miss 计数」，不崩。

**不带 sid，只带 name**：`#{@ccm_sid}` 在 hook 上下文取不到（P0 实测拿到空 ⇒ 活会话被判灰）；
而 name→sid 的映射 monitor 本来就有（最新那份 `tmux ls` 原文）。
**让知道的人去查，比让不知道的人硬传更稳。**

monitor 侧收到即 `SessionChange{removed}` 送 emitter（**唯一写者照常分流**）⇒ 绕过 miss 计数。
查不到 sid 时**不猜**（never-bound 会话 / 快照还没到），交给对账兜底——那条路对 never-bound
有专门的不误判逻辑。**`RETIRE_MISS_THRESHOLD >= 2` 与快照路径一字未动。**

## 4. 真机验证（私有 socket）

| 验证 | 结果 |
|---|---|
| `kill-session alpha`（还剩 beta） | **死亡帧 18 ms** |
| 再 `kill-session beta`（最后一个，server 随之退出） | 也发出 beta 的死亡帧 |
| `hello` 帧的 `emits` | 含 `tmux_session_closed` ✓ |
| **重复上报** | **没有** —— 恰好 2 帧（alpha / beta） |

**一个把我自己绕了一下的测量坑**：`grep -c tmux_session_closed` 数出 3，
因为 **`hello` 帧的 `emits` 数组里也含这个字符串**。逐帧列出来才看清实际是 2。
（同一类：数数之前先确认数的是不是那个东西。）

## 5. 变异验收（Phase D）

| 变异 | 结果 |
|---|---|
| **A** 观测失败也当「都没了」 | **成立**：红 `unobservable_never_reports_deaths_and_keeps_snapshot` |
| **B** 差分方向搞反（新出现的当死亡） | **成立**：红 4 条 |
| **C** 第一次观测就报死亡 | **成立**：红 `first_observation_reports_nothing` |

**如实标注**：铁律 8 要求并行多 agent 审计；本会话常驻指令「除非用户要求不开 agent」
⇒ 主线程变异 + 全门禁 + 真机实跑代替。**这是欠账，不是强度裁剪。**

## 6. 第 3 步：删 `TMUX_EMIT_INTERVAL`（**本轮已做**）

上一轮把它单独留出来，理由是「铁律 9 点名它是本区最像会自己犯的错」。本轮单独走完。

### 6.1 差点顺手删掉的东西：**首轮立即发一拍**

`spawn_tmux_ticker` 除了打节拍，还承担 monitor 连上时的第一次探测
（它的注释自己写着「立刻发一拍再进入 sleep」）。**整个删掉的话，空闲机器上 daemon 要等到
第一个 hook 触发才探 —— 可能是「永远」。** 那不是把轮询换成事件，是把信号删了。

⇒ 换成一次性 `initial_tmux_probe()`：**零线程、零定时器，但那一拍留着**。
原来那条 `tmux_ticker_fires_immediately` 测试改判为 `initial_probe_fires_once`
（并加了「**不该发第二拍**」——它不是节拍器），**性质没放松**。

### 6.2 必须同步做的第二件事：接上 `WatchEvent::Shutdown`

`sink.is_closed()` 那道复查此前**靠 ticker 每 8s 醒一次**。ticker 一删，reader 线程会
一直阻塞在 `recv()` —— 不是泄漏（进程退出时随之消亡），但「没人听就停读」这条性质会丢。
**而且漏做不会红任何测试。**

⇒ `WatcherPoke::shutdown()` + `main` 在 select 结束后调它；`WatchEvent::Shutdown` 上的
`#[allow(dead_code)]` 一并去掉。**实测**：把 daemon 的 stdout 接给 `head -1`（读一行就关管道）
⇒ **111 ms 退出**（超时上限设的 20s）。

### 6.3 验收：重跑对照实验，不是看测试绿

| | 「多个中杀一个」→ 死亡帧 | 起飞后初探帧 |
|---|---|---|
| P4 后（ticker 还在） | 136 / 137 ms | —— |
| **P5 删 ticker 后** | **126 ms**（两次复现一致） | **2**（初探仍在） |

延迟没有退化。**生产段零定时器**（`thread::sleep` / `recv_timeout` /
`Duration::from_secs` / `Instant::now` 逐个扫为 0），有守卫钉住、变异成立。

## 7. 门禁

| 门禁 | 前 | 后 |
|---|---|---|
| daemon `cargo test` | 160 | **171** |
| monitor `cargo test --lib` | 620 | **622** |
| daemon clippy / fmt / `readonly_guard` | 0 / clean / 绿 | 同左 |
| `PROTO_VERSION` | — | **未 bump**（additive） |
| `RETIRE_MISS_THRESHOLD` / `TMUX_LS_FMT` | — | **一字未动** |

## 8. 签收

- [x] 快照差分（三态语义 + 首次不报 + 幂等），三条变异全部成立
- [x] `TmuxSessionClosed` 帧 + `EMITS` 登记 + monitor 消费（绕过 miss 计数，兜底路原样保留）
- [x] 真机实测 **18ms**，无重复上报
- [x] **删 `TMUX_EMIT_INTERVAL` + 接 `Shutdown`**：延迟 **126 ms**（未退化）· stdout 关闭 **111 ms** 退出 · 生产段零定时器（守卫 + 变异成立）· **首轮初探保住了**（§6.1，差点顺手删掉）

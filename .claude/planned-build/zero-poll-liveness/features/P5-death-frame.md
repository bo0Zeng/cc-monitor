# P5 — 正向死亡帧（**第 1/2 步已交付；第 3 步删轮询 B 刻意留到下一轮**）

> 主计划：`../MASTERPLAN.md` §1 P5 · §3 账本 · §4 顺序表第 6 位 · 铁律 9
> 前置：**P4**（`3ccb6d6`）+ P5 前置实测（`d9f464c`：136ms vs 5042ms，带对照组）

## 1. P5 是三步，本轮做了前两步

| 步 | 内容 | 状态 |
|---|---|---|
| 1 | daemon 留住上一份 `tmux ls` 快照 + 差分 | **✅ 本轮** |
| 2 | 新帧 `TmuxSessionClosed { name }` + monitor 消费 | **✅ 本轮** |
| 3 | 删 `TMUX_EMIT_INTERVAL`（轮询 B） | **⏸ 刻意留到下一轮**，理由见 §6 |

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

## 6. ★ 第 3 步（删轮询 B）为什么留到下一轮

**不是没时间，是这一步该单独走一圈。** 三条理由：

1. **主计划铁律 9 点名它是「本区最像会自己犯的错」**——为了让零定时器守卫变绿而删掉唯一
   信号源。这种一步值得有自己的完整 B→F，而不是塞在别的功能尾巴上。
2. **它还捆着一件必须同时做的事**：删了 ticker 之后 `sink.is_closed()` 就没人复查了
   （现在靠 ticker 每 8s 醒一次），必须把写端关闭接到预留的 `WatchEvent::Shutdown` 上，
   否则 reader 线程会一直阻塞在 `recv()`。漏做不会红任何测试。
3. **验收判据是「重跑对照实验」而不是「测试绿」**——删完要再量一次延迟仍是百毫秒级。

**先落新信号、让它待一轮，再撤旧信号**，这个顺序本身也更稳。

## 7. 门禁

| 门禁 | 前 | 后 |
|---|---|---|
| daemon `cargo test` | 160 | **169** |
| monitor `cargo test --lib` | 620 | **622** |
| daemon clippy / fmt / `readonly_guard` | 0 / clean / 绿 | 同左 |
| `PROTO_VERSION` | — | **未 bump**（additive） |
| `RETIRE_MISS_THRESHOLD` / `TMUX_LS_FMT` | — | **一字未动** |

## 8. 签收（部分）

- [x] 快照差分（三态语义 + 首次不报 + 幂等），三条变异全部成立
- [x] `TmuxSessionClosed` 帧 + `EMITS` 登记 + monitor 消费（绕过 miss 计数，兜底路原样保留）
- [x] 真机实测 **18ms**，无重复上报
- [ ] **删 `TMUX_EMIT_INTERVAL` + 接 `Shutdown`：留到下一轮单独走一圈**（§6）

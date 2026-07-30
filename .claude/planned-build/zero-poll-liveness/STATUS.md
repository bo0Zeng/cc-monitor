# STATUS — zero-poll-liveness

> **恢复入口。每轮先读本文件，再读 `MASTERPLAN.md` 与当前 feature 文件。**

## 当前阶段

**✅ 全区收官（2026-07-30）—— P0-P7 八个功能全部交付签收。**

**用户要求（2026-07-29「我要把轮询杀掉」）已达成**：daemon 里 **A/B 两条轮询都已删除**
（2s 判活 tick + 8s `tmux ls` tick），生产段**零定时器**，由 `no_timer_guard.rs` 钉住
（判据落在「周期性唤醒」而非「出现 `Duration`」，四条变异成立含「护栏自身失效」）。

**四路事件与实测延迟**（每格都是真机实测，测法见各 feature 文档）：

| 场景 | 事件源 | 实测 |
|---|---|---|
| claude 进程退出 / 被强杀 | pidfile inotify + `pidfd` | **~18ms**（原 ≤2s） |
| 杀掉某 origin 仅剩的会话 | tmux server 的 `pidfd` | **27ms**；跨 cgroup SIGKILL **30ms** |
| server 复活 | socket 目录 inotify `IN_CREATE` | **153ms**（含 100ms 去抖） |
| **多个会话里杀掉其中一个** | tmux hook → SIGUSR1 → 正向死亡帧 | **126ms**（**对照组：拆掉 hook 5042ms**），原 8s×2 ≈ **16s** |

**唯一一条正确性改进**（不只是延迟）：`pidfd` 绑进程实例本身 ⇒ **PID 复用在机制上不存在**。

**成果落档**：`doc/INVARIANTS.md` **§41**（四路事件 + 三盲区分类 + 护栏纪律 + 兼容部署）·
§24 补「事件路同样只经 emitter」· §24bis 四处过时表述订正 · **BACKLOG E34 已结案**
（含对其原措辞三处订正）· E33 标成「延迟那半已解、诊断那半未做」。

**兼容与部署**：wire 两处 additive（`observation` 字段 + `TmuxSessionClosed` 帧），
**`PROTO_VERSION` 未 bump**；`BUILD_ID` → **`p1r-event-liveness`**（P5 漏做、P7 补上）。
**真机生效仍需重部署。**

**三条红线全程未破**：`TMUX_LS_FMT` · `RETIRE_MISS_THRESHOLD >= 2` · `shared/ccm` 本体
——**一字未动**。死亡帧是**绕过** miss 计数的快路径，不是替换兜底。

## 自动模式

**全自动**（用户 2026-07-30 原话「批准，开始全自动跑」）：loop 连续 B→G，
只在阻塞 / 需新决策 / 全部完成时停。功能计划（Phase B）不再逐个呈交。

## 功能进度

| # | 功能 | 状态 |
|---|---|---|
| P0 | 五项机制实测 | **✅ 完成签收**（`features/P0-machine-facts.md`）。五项里三项坏答案，定死三条设计 |
| P1 | `ZeroSessions` 观测分类（销 `INVARIANTS:408` 残留） | **✅ 完成签收**（`features/P1-zero-sessions-sentinel.md`）。三条变异双向成立；延迟「永不」→ **~16s**（有界化不是即时化） |
| P2 | pidfd 替判活轮询 + 建统一事件 channel | **✅ 完成签收**（`features/P2-pidfd-unified-channel.md`）。账本第 1 行到最终形态；**端到端实测 ~18ms**（原 2s tick）；两条变异双向成立 |
| P3 | tmux server 生/死/复活（**不删 8s 轮询**） | **✅ 完成签收**（`features/P3-tmux-server-lifecycle.md`）。调研的 ⚠ 盲区已消；实测 kill-server→27ms · 复活→153ms · 跨 cgroup SIGKILL→30ms；零新定时器 |
| P4 | daemon 装 tmux hook + SIGUSR1 通知通路 | **✅ 完成签收**（`features/P4-tmux-hook-notify.md`）。拆 P4a/P4b，**顺序由安全性决定**（SIGUSR1 默认终止进程 ⇒ 处理器必须先于 hook）。真机私有 socket 实测：**通路打通**（探针被信号终止）+ **PID 复用防御成立**（starttime 写错不误伤）；**默认 socket 零改动**。**同一个自指陷阱连踩七次**，其中一次让守卫成了安慰剂——是变异揪出来的 |
| P5 | 正向死亡帧 + 删 `TMUX_EMIT_INTERVAL` | **✅ 完成签收**（`features/P5-death-frame.md`）：快照差分（三态；**观测失败绝不当「都没了」**）+ `TmuxSessionClosed` 帧（additive、不 bump、旧 monitor 跳过）+ monitor 消费（绕过 miss 计数，**兜底路一字未动**）+ **删轮询 B 并接上 `Shutdown`**。**生产段零定时器**。实测：多个中杀一个 **126ms**（删前 136/137）· stdout 关闭 **111ms** 退出 · 首轮初探保住了（差点顺手删掉）|
| P6 | 零定时器门禁 + 延迟 e2e | **✅ 完成签收**（`features/P6-no-timer-gate.md`），两半都做完：**① 守卫** `no_timer_guard.rs` 扫全 crate 生产段，**判据落在「周期性唤醒」而非「出现 `Duration`」**（去抖窗口是 `Duration` 但不是定时器）；非定时器用途**逐条登记带理由**，多一处未登记就红。四条变异成立（含「护栏自身失效」）。**② 端到端延迟 e2e** 并进 `graylight-daemon-frames`（5 → 8 条，`ci.yml` 地板同步抬），阈值 5s 是**数量级判据不是性能指标**。★ 并轨时**撞出并修掉一个 P5 留下的真回归**（对照组确认非本轮引入，见该文 §7.1）|
| P7 | 文档收口 + E34 结案 | **✅ 完成签收**（`features/P7-docs-and-closeout.md`）：`INVARIANTS §41` 新节（四路事件带实测延迟与测法 + 三盲区分类 + 零定时器护栏两条派生纪律）· §24/§24bis 订正 · **E34 结案 + 对其原措辞三处订正** · E33 只标解了一半（诊断那半是 UI 改动、不在本区）。★ **开工复测抓出 P5 漏掉的 `BUILD_ID` bump** —— 改不改都全绿，漏掉则整轮工作在已部署远端休眠；已补为 `p1r-event-liveness`，传导四条核实全部实跑 |

## 阻塞项 / 待用户表态

| # | 事项 | 阻塞谁 | 状态 |
|---|---|---|---|
| 1 | ~~主计划审批~~ | — | **✅ 2026-07-30 用户已批准** |
| 2 | ~~授权装 hook~~ | — | **✅ 2026-07-30 用户已授权**（P0-② 实测证明 per-session 路线不存在 ⇒ 这条授权正是必需的） |

**当前无阻塞项。**

## 必须做但刻意延后的（不许忘）—— **三条全部结清**

| # | 事项 | 结果 |
|---|---|---|
| 1 | **bump `BUILD_ID`** + 重部署 | **✅ 已 bump 为 `p1r-event-liveness`**。★ **原排 P5、P5 漏做、P7 复测抓出** —— 这条常量改不改都全绿，是「有些遗漏不会红任何测试」的标本。**重部署仍待真机** |
| 2 | 给 6 套非 CI e2e 做 socket 隔离（E41） | **✅ 由 `gate-integrity` G-C 做掉**（`unset TMUX` + 短 `TMUX_TMPDIR`，**E41 已销**）。⇒ P6 的延迟 e2e **直接并进 `graylight-daemon-frames`**（5 → 8 条），没另造套件 |
| 3 | 删 ticker 前必须接 `WatchEvent::Shutdown` | **✅ P5 已接**（`WatcherPoke::shutdown()`）。实测 stdout 关闭 → **111ms 退出** |

## 收官后的移交（给下一个碰这块的人）

**本区已关闭，无在途工作。** 三件登记在别处、不属本区的事：

| 事项 | 去处 |
|---|---|
| **重部署**（让 `p1r-event-liveness` 在真机生效） | 待用户；不 bump 时的后果与单一事实源机制见 `INVARIANTS §41.5` |
| **E39**：`notify-debouncer-mini` 静默吞掉 inotify 队列溢出 | BACKLOG E39（**既有盲区、非本区引入**；`pidfd` 对溢出免疫 ⇒ 本区让情况变好）。**绝不为它补定时器** |
| **E33 的诊断那半**：给 tab 加「三格一眼可分」的可见诊断 | BACKLOG E33（UI 改动）。三格内容要跟着改写：「在等那 ~16 秒」现在≈不可感知；「daemon 没在发帧」要多列一种成因——**hook 没装上** |

**给 `local-as-remote` L1 的提醒（E40）**：cgroup 隔离结论**只对「daemon 经 SSH 起」成立**。
本地路径下 daemon 可能与 tmux **同锅**，届时 pidfd 探针会被一起端。
**必须重测这一格，不许继承本区的结论。**

## loop 停止条件

- P0 拿到会推翻设计的答案 → 停，交回用户重定方向
- 跑到 P4 而授权未给 → 跳过 P4/P5，继续 P6/P7，收尾如实列出
- 同一步 ≥2 次失败 → 停
- 门禁红且非在途变异 → 停
- 全部完成 → Phase G → 停

## 与其他工作区的关系

- **执行顺序表第 21 行就是本区**（E34）。原表写「（新，未分配工作区）」+「需改
  `shared/ccm` 本体」——**两条都要订正**：本区已建，且 hook 由 daemon 装 ⇒ 不碰 ccm
- `gate-integrity` 也会碰 `ci.yml` ⇒ 双方都只追加
- `local-as-remote` L1 会碰同一个 daemon ⇒ 本区已先落地，L1 继承事件模型。
  **但 E40 那格不许继承**：cgroup 隔离结论只对「daemon 经 SSH 起」成立，本地可能同锅 ⇒ 必须重测

## 时间线

- 2026-07-30 Phase A 落盘（本文件 + `MASTERPLAN.md`）
- 2026-07-30 **P0-P7 全部交付签收，本区收官**。两条轮询删净、生产段零定时器；
  「多个中杀一个」从 ~16s 降到 **126ms**（有对照组）；`INVARIANTS §41` 落档，**E34 结案**

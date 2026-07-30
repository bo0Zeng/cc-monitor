# P4 — daemon 装 tmux hook + SIGUSR1 通知通路

> 主计划：`../MASTERPLAN.md` §1 P4 + §「P4 设计修订」· §3 账本第 1 行 · §4 顺序表第 5 位
> 前置：**P2**（统一 channel）· **P3**（`ServerState::Alive(pid)` 这个「该装 hook」的时机）
> 授权：用户已给（「授权」）—— 这是本区唯一需要外部授权的一步

## 1. 它补的是**唯一**没有内核事件源的那个场景

| 场景 | 事件源 | P4 前延迟 |
|---|---|---|
| claude 进程退出 / 被强杀 | pidfile inotify + pidfd（P2） | ~0 |
| 杀掉某 origin **仅剩的**会话 | pidfd 看 server 进程（P3） | ~0 |
| server 复活 | socket 目录 inotify（P3） | ~0 |
| **多个会话里杀掉其中一个** | **没有** —— server 还活着、socket 还在 | **~16s（只有 8s 轮询兜）** |

tmux 自己知道最后这件事，`session-closed` hook 就是那个信号。

## 2. 拆成 P4a / P4b，**顺序是安全性决定的**

> **`SIGUSR1` 的默认处置是终止进程。** 先装 hook 再装处理器 =
> 给一个会自杀的 daemon 装上自杀触发器。

- **P4a**（`7127357`）daemon 侧：`WatchEvent::Poke` + 窄句柄 `WatcherPoke` + `main` 的
  SIGUSR1 流。**没有 hook 在发信号 ⇒ 完全惰性**，可以安全先落地。
- **P4b**（本次）：`--tmux-notify` 子命令 + 在 `ServerState::Alive(pid)` 臂装 hook。

## 3. 通路（**daemon 文件系统写归零**）

```
tmux hook (全局 [50], run-shell -b)
   └─> <daemon exe> --tmux-notify <daemon_pid> <daemon_starttime>
          ├─ 读 /proc/<pid>/stat 校验 starttime 相符
          └─ kill(pid, SIGUSR1)
   daemon SIGUSR1 流 → WatcherPoke::poke() → 统一 channel → 立刻重探
```

三处刻意的设计：

| 决定 | 理由 |
|---|---|
| **不传会话名** | 原方案把 `#{hook_session_name}` 写进日志文件 ⇒ ① 撞红线 I7 ② 名字要过 shell 引号，含 `"` / `$(...)` 就是注入面。不传 ⇒ **两个问题一起消失**。代价（信号无载荷、会合并）由「重探 + 差分」天然免疫 |
| **`run-shell -b`** | `-b` 是后台执行。不加的话 tmux 会**同步等**我们的进程跑完，把关会话这条路径卡住 |
| **固定槽位 `[50]`** | 调研实测空着。固定槽位才**可撤销**（`set-hook -gu 'session-closed[50]'`），而不是追加到一串未知 hook 后面 |
| **三个事件都装** | `session-renamed` 最容易被当成可选 —— 名字是消费侧查表的键，改名不通知会让下一次差分误判成「一个消失 + 一个新出现」 |

**停机时不摘 hook**（`hook_unset_args` 存在但生产路径不调，有注释说明）：留着的 hook 指向
已死的 pid，`notify` 校验不过就静默 no-op；server 重启 hook 本就没了；主动摘反而会在
「同机两个 daemon」时摘掉对方的。

## 4. 真机验证（**全程私有 socket**，默认 socket 一次都没碰）

用 `sleep` 当探针：它对 SIGUSR1 的默认处置就是终止 ⇒ 「进程没了」= 信号真送达。

| 验证 | 做法 | 结果 |
|---|---|---|
| **通路打通** | 私有 socket 起 `a1`/`a2`，装 hook 指向探针的真 pid+starttime，`kill-session -t a1` | 探针**被终止** ⇒ hook → notify → SIGUSR1 全程打通 |
| **PID 复用防御** | 同样装 hook，但 starttime **故意写错**（`1`） | 探针**存活** ⇒ starttime 不符时静默 no-op，不误伤无关进程 |

收尾：默认 socket 三个真实会话 **逐字未变** · hook 数 **57 → 57** ·
`pgrep` 只剩用户那台 server · `/tmp/p4b*` 零残留。

**本轮没往默认 socket 装任何 hook** —— 用户虽已授权，但开发验证没有理由碰住着真实会话的那台。

## 5. ★ 同一个自指陷阱，本功能连踩**七**次

扫源码的守卫把判据字面量写进自己源码，`include_str!` 读回来被自己命中：

| # | 形态 | 后果 |
|---|---|---|
| ①② | `rfind("#[cfg(test)]")` 找到的是测试里那行字面量 / `!contains(<字段 pub 形状>)` 被自己命中 | 一个几乎没剥掉、一个恒红 |
| ③ | **解释这个坑的注释**逐字引用那串 | 又触发一次 |
| ④⑤ | 「Poke 与 TmuxProbeDue 共用臂」那条 `contains` 恒真 | **守卫是安慰剂** —— 是**变异验收**揪出来的：拆臂后它没红 |
| ⑥ | 新模块两条守卫扫到**模块头注**里提到的 `#{@ccm_sid}` / 那个建目录调用 | 两条一起红 |
| ⑦ | **`readonly_guard`（红线 I7 那道）** 扫到我注释里逐字写的那个建目录调用名 | 全局守卫红 |

**处置分两种，界限很清楚**：

- 我**自己写的**守卫 ⇒ 修守卫：判据 `format!` 运行时拼 · 只扫剥掉 `cfg(test)` 的生产段 ·
  再剥行注释 · 剥离锚点用 `"\n#[cfg(test)]\nmod tests"`（源码里是转义写法，不自匹配）
- **`readonly_guard` 那道红线守卫 ⇒ 改我的措辞，不改它。**
  它连注释一起扫是 **fail-closed 的设计**（模块头注自陈：偏向少剥 → 顶多假阳性）。
  为自己方便去把红线守卫改成剥注释，是拿安全网换便利。

⇒ **新增一条纪律**：daemon 源码的散文里**不许逐字引用** `readonly_guard` 的禁用模式。

## 6. 变异验收（Phase D）

| 变异 | 结果 |
|---|---|
| **A** 拆掉 `Poke`/`TmuxProbeDue` 共用臂（P4a） | 初版**没红**（守卫是安慰剂）⇒ 修好守卫后**成立** |
| **B** 窄句柄字段改 `pub`（P4a） | **成立** |
| **C** 再开一条事件 channel（P4a） | **成立**（实得 2 处 ≠ 1） |
| **D** hook 里塞 `#{@ccm_sid}` | **成立**（`never_uses_ccm_sid_format_in_hooks` 红） |
| **E** 本模块加一处文件系统写 | **成立**（模块内守卫 + `readonly_guard` 双红） |
| **F**（真机）starttime 写错 | **成立**：探针存活 ⇒ 不误伤 |

**如实标注**：铁律 8 要求并行多 agent 审计；本会话常驻指令「除非用户要求不开 agent」
⇒ 主线程变异 + 全门禁 + 真机实跑代替。**这是欠账，不是强度裁剪。**

## 7. 门禁

| 门禁 | 前 | 后 |
|---|---|---|
| daemon `cargo test` | 152（P4a 后） | **160** |
| daemon clippy | 0 | **0** |
| `readonly_guard` | 绿 | **绿**（中途红过一次，是我的注释触发的，已改措辞） |
| monitor / vitest / tsc | 620 · 866 · 0 | **不变**（P4 只碰 daemon） |

## 7bis. ★ 真 daemon 端到端实测（P5 开工前补做，带对照组）

P4 交付时只验到「hook → 信号送达」（用 `sleep` 当探针）。**没验的那半**是「daemon 收到
信号后真的重探并发帧」—— 而删轮询 B 的前提正是这半。P5 开工第一件事就是把它补上。

私有 socket 起两个会话 + 真 daemon（`CLAUDE_CONFIG_DIR` 指向临时目录），
`kill-session -t s1`，量到第一帧新 `tmux_sessions` 的延迟：

| 组 | hook 数 | kill → 新帧 |
|---|---|---|
| **实验组**（daemon 自己装的 hook 在） | 3 | **136 ms** |
| **对照组**（把三个 hook `set-hook -gu` 拆掉） | 0 | **5042 ms** |
| **实验组**（复现一次） | 3 | **137 ms** |

**为什么要对照组**：单看 136ms 说明不了问题 —— 8s ticker 恰好落在那 136ms 里的概率约
1.6%，不算小到可以忽略。拆掉 hook 后延迟回到秒级（那是 ticker 的节拍），**因果才成立**。

顺带确认两件 P4 没直说的：daemon **自己**在 `ServerState::Alive` 那个臂装上了 3/3
（日志「tmux hook 已装 3/3」），且 `SIGUSR1 处理器已就位` 先于它出现 —— P4a/P4b 那条
安全性顺序在运行时也是对的。

⇒ **删轮询 B 的前置条件已满足**（主计划成功标准 1 / §1 P3 / §4 顺序表三处写死的那条）。

## 8. 本轮没做的

- **没往默认 socket 装 hook**（§4）
- **快照差分还没做** —— P4 只做到「立刻重探」。「消失的是**哪个**会话」要 daemon 留住上一份
  `tmux ls` 快照，那是 **P5** 的事（正向死亡帧要那个名字）
- **`TMUX_EMIT_INTERVAL`（轮询 B）仍在** —— 删它的前置条件写死在三处，属 P5

## 9. 签收

- [x] 通路打通（真机私有 socket 实测，探针法）
- [x] PID 复用防御成立（反向实测：starttime 不符不误伤）
- [x] daemon 文件系统写归零（`readonly_guard` 绿）
- [x] 六条变异全部成立（其中 A 是修好安慰剂守卫之后才成立的）
- [x] 默认 socket 零改动（会话逐字未变 / hook 57→57 / 无孤儿 / 无残留）

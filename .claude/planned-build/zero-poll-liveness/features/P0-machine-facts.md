# P0 — 五项机制实测（本工作区唯一可能推翻方向的一步）

> 主计划：`../MASTERPLAN.md` §1 P0 · 前置：无 · 后继：P1-P5 全部依赖本文件的答案
>
> **这个功能不写产品代码。** 它的产出就是本文件的「§3 实测结果」——
> 后面四个功能的形态取自这里，所以**任何一项没测到就不许往下写代码**。

## 1. DoD

- [ ] 五项各有**实测证据**（命令 + 原始输出），不是推理、不是照抄调研文档
- [ ] 每项明确落到「好答案 / 坏答案」，坏答案**当场写下降级路径的具体形态**
- [ ] 全程只碰隔离 socket（走 shim，无 `-L`/`-S` 一律 rc=97）
- [ ] 收尾**逐字核对默认 socket 会话清单未变**（基线已存
      `scratchpad/default-socket-baseline.txt`：`cc-7c2a26d6` / `cc-9d66c46d` / `cc-d7692cdf`）
- [ ] 收尾核对**默认 socket 已设 hook 数仍为 0**（本轮独立复测已确认基线=0，与调研一致）
- [ ] 测试 socket 全部清除、无孤儿进程
- [ ] 跑完**强制回 Phase F 对账主计划**（P0 的答案可能改 P2-P5）

**明确不做**：不碰默认 socket · 不起真实已认证的 `claude`/`codex` ·
不装任何软件包 · 不改仓内任何代码文件（本轮只写本计划文件）

## 2. 五项的测法（逐条可照做）

### ① `session-closed` hook 里 `#{@ccm_sid}` 能不能展开？

**为什么问**：决定死亡帧带 `sid` 还是带 tmux 会话名。会话正在销毁，session 级 option
可能已经不可访问。

**测法**：隔离 socket 建会话 → `set-option -t <会话> @ccm_sid FAKE-SID-123` →
设全局 `session-closed[50]` hook，把 `#{hook_session_name}` 与 `#{@ccm_sid}` **都**写进日志 →
`kill-session` → 看日志里 `@ccm_sid` 是 `FAKE-SID-123` 还是空。

**判色**：必须先确认 hook 真的设上了（`show-hooks -g | grep '\[50\]'` 有输出），
否则「日志里没有 sid」可能只是 hook 没生效。

- 好答案（能展开）→ 死亡帧直接带 sid，monitor 零反查
- 坏答案（展开成空）→ 带名字，monitor 用最新快照反查（主计划 §2 注 2 已论证安全）

### ② per-session hook（`set-hook -t <会话>`）可不可行？

**为什么问**：可行则只影响我们建的会话、不碰全局槽位，授权面小得多。

**测法**：两个会话 A/B，只给 A 设 `set-hook -t A 'session-closed[50]'` →
分别 kill A 和 B → 看日志里是否**只有 A 那条**。

- 好答案（生效且只对 A）→ P4 用 per-session，全局槽位不碰
- 坏答案（不生效 / 泄漏到 B）→ 用全局 `[50]`（已获用户授权）

### ③ daemon 落在哪个 cgroup？

**为什么问**：决定 daemon 自持的 pidfd 探针扛不扛「tmux 整锅 SIGKILL」。

**测法**：daemon 由 monitor 经 SSH 直接 exec ⇒ 量「经 ssh 起的进程」的 cgroup
与「tmux server」的 cgroup 是否同一个。本机若可 `ssh localhost` 就直接量；
不可则退一步量 `sshd` 的 cgroup 结构 + tmux server 的 cgroup，判断二者是否必然同锅。

- 好答案（不同锅）→ P3 的 server 死亡路照做
- 坏答案（同锅）→ P3 该路降级为「SSH 断连即全清」（已有路径），如实标注

### ④ `tmux ls` 三态的确切 stdout / rc

**为什么问**：`NO_SESSIONS` 哨兵（P1）必须能和「无 tmux」「exec 失败」互斥。
现在 `run_tmux_ls` 把三者全压成同一个空串。

**测法**：分别量 ⓐ server 活 + 零会话 ⓑ server 不在（死 socket）ⓒ 无 tmux 可执行文件
三种情况下 `tmux ls -F '<TMUX_LS_FMT>'` 的 stdout 与 rc。

**注意**：ⓐ「server 活但零会话」在 tmux 里本身难构造——最后一个会话消失时 server 就退出。
要测就得让 server 上有个非会话的存活理由，或确认「零会话 ⇒ server 必退」从而 ⓐ **不存在**。
**这一条的答案会直接决定 P1 哨兵的语义**（是「零会话」还是「server 没了」）。

### ⑤ `notify` / `notify-debouncer-mini` 把 inotify 队列溢出报成什么

**为什么问**：零轮询下溢出 = 静默漏事件，必须映射成「立刻全量重同步」。
**绝不许偷偷补一个定时器兜**——那等于零轮询造假。

**测法**：读 cargo registry 缓存里 `notify 6.1` / `notify-debouncer-mini 0.4` 的源码，
找 `IN_Q_OVERFLOW` / `Event::need_rescan` / `EventKind::Other` 的处理路径。
比跑一个真溢出（要塞满 16384 条默认队列）更快且更确定。

- 好答案（能拿到溢出信号）→ P3 把它接到「全量重同步」
- 坏答案（crate 吞掉）→ 换低层 `inotify` crate，或**如实登记为盲区**

## 3. 实测结果（2026-07-30，隔离 socket `p0a`-`p0f`，全部已清）

**总评：五项里三项是坏答案，其中一项若照直觉写会造成真 bug。P0 的价值成立。**

### ① `#{@ccm_sid}` 在 `session-closed` 里**解析到别的会话**（比"空"危险得多）

三会话 A/B/C 各设 `@ccm_sid = SID-A/B/C`，全局 `session-closed[50]` hook 同时记
`#{@ccm_sid}` 与 `#{hook_session_name}`：

```
杀 B → ccm_sid=[SID-C]  session_name=[B]
杀 A → ccm_sid=[SID-C]  session_name=[A]
```

`#{hook_session_name}` **正确**；`#{@ccm_sid}` 恒等于**当时"当前会话"**（C）的值。
更早的两会话版同样：杀 A 拿到 `FAKE-SID-BBB`。

⇒ **这不是"取不到"，是"取到别人的"。** 若按直觉写 `#{@ccm_sid}`，
**杀掉 A 会把还活着的 C 变灰** —— 一个静默的、方向完全错的 retire。
`#{session_name}` 的老坑（调研坑 1）在 **session 级 option 上同样成立**，
而调研只记了 `session_name` 那一半。

**判色前置已做**：`show-hooks -g | grep '\[50\]'` 每次都先确认 hook 真的设上了，
所以"日志里 sid 不对"不可能是 hook 没生效造成的。

**⇒ 定型（覆盖主计划 P0-① 的"好答案"分支）**：死亡帧**只带 `#{hook_session_name}`**，
monitor 侧用最新快照做 name→sid 反查。主计划 §2 注 2 已论证该路径对 `/branch` 漂移安全。

### ② per-session `session-closed` **完全不触发**（且证明不是 per-session 机制的问题）

只给 A 设 `set-hook -t A 'session-closed[50]'`：`show-hooks -t A` 能看到、`show-hooks -g`
干净（没泄漏到全局）—— 但**杀 A 时一条都没记**。

**对照实验**（关键，否则分不清"per-session 整体不 work"和"session-closed 专门不 work"）：
同一个 A 上同时设 `session-renamed[50]`（per-session）+ `session-closed[51]`（per-session）：

```
rename A → A2 : per-session-RENAMED 触发 ✓
kill   A2     : 日志无新增 ✗
```

⇒ **per-session hook 机制本身可用，`session-closed` 专门不支持 per-session。**
合理解释：会话对象正销毁，它自己的 session 级 hook 已不可达。

**⇒ 定型**：E34 期望的"最干净路线（把 sid 烤进 per-session hook）"**不存在**。
P4 必须用**全局 `[50]`** —— 用户 2026-07-30 已授权。

### ③ `run-shell -b` 与同步版的分界（本项计划外，实测中撞出来的）

| 触发方式 | hook 跑不跑 | `hook_session_name` | `@ccm_sid` |
|---|---|---|---|
| 杀掉**多个中的一个**，`-b`（后台） | **跑** | 正确 | 别人的（见 ①） |
| 杀掉**最后一个**，`-b`（后台） | **没记到** | — | — |
| 杀掉**最后一个**，同步（不带 `-b`） | **跑** | 正确（`only`） | **空** |
| `kill-server` | **两种都不跑** | — | — |

「最后一个 + `-b`」写不进去的机理与调研 §4 记的 `wait-for` 失败同源：
**干净退出时 tmux 会先 SIGTERM 掉自己的 `run-shell` 子进程**，后台版在写盘前就被打死。

**⇒ 定型（这是本项最有价值的产出）**：用 `-b`，**绝不用同步版**——同步 `run-shell`
会阻塞用户实况 tmux server，风险不可接受。「最后一个会话 / `kill-server`」这两格
**交给 P3 的 pidfd**（server 退出 ⇒ pidfd 立刻醒）。
⇒ **hook 与 pidfd 的分界从此有实测依据，不是设计时的猜测**：
hook 管「多个中杀一个」，pidfd 管「杀到没了 / server 被端」。两者不重叠、合起来无缺口。

### ④ 四态，靠 **rc + stdout 是否为空** 可分（locale 无关）

**⚠ 本项 Phase D 自审时发现漏测，已补**：初测结论写的是「『server 活着但零会话』
**不存在**」——那只在 `exit-empty on`（默认）下成立。tmux 有 `exit-empty` 选项，
**关掉之后该状态就存在**，且它的 rc 与"有会话"相同：

```
ⓐ server 活 + 有会话                     : rc=0, stdout 有行
ⓑ 杀掉唯一会话（exit-empty on = 默认）    : rc=1, stdout=[], stderr=[no server running on …]
                                           ⇒ server 随最后一个会话一起退出
ⓒ socket 文件根本不存在                   : rc=1, stdout=[], stderr=[error connecting to … (No such file…)]
ⓓ 杀掉唯一会话（exit-empty off）           : rc=0, stdout=[] 长度 0, stderr 空,
                                           `display-message -p '#{pid}'` 仍返回 pid
                                           ⇒ **server 真的还活着、零会话**
ⓔ PATH 里无 tmux                          : `command -v tmux` 失败 ⇒ 现有 NO_TMUX 门已覆盖
```

用户实况的默认 socket 上 `exit-empty on`（已只读确认），所以 ⓓ 在本机不出现——
但 **daemon 跑在任意远端主机上，不能假设那台机器也是默认值**。

**关键洞察**：ⓑ / ⓒ / ⓓ 三者对**retire 决策而言完全等价**（都是"零会话"），
它们的区别只对 P3 的"复活监视"有意义。所以 P1 的哨兵语义应是 **`ZERO_SESSIONS`**
而不是 `NO_SERVER` —— 抽象层次对了，P3 加 server 活/死的细分时**不必改帧契约**。

另两条：
- **server 死后 socket 文件仍留着**（`srw-rw---- … 0 bytes`）⇒
  **「socket 文件存在」≠「server 活着」**，P3 不许用文件存在性判活
- **死 socket 上调用不会把 server 拉活**（与调研一致）

**⇒ 定型（部分订正 `INVARIANTS:408` 的措辞）**：那条残留项写的「命令成功但零会话」
**在默认配置下不出现**（默认是 server 直接退出、rc=1），但在 `exit-empty off` 下**确实出现**
——所以它的措辞不算错，只是漏了更常见的那一半。P1 的哨兵要**同时**覆盖两者。

判据（**只用 rc + stdout 是否为空，不碰 stderr 文本**——文本有两种且不该拿英文消息当判据）：

| 观测 | 分类 | monitor 能不能安全 retire |
|---|---|---|
| `rc=0` + stdout 非空 | `Sessions(raw)` | 按 raw 对账 |
| `rc=0` + stdout 空 | `ZeroSessions`（ⓓ） | **能** |
| `rc=1` | `ZeroSessions`（ⓑ/ⓒ） | **能** |
| `command -v tmux` 失败 | `NoTmux` | 跳过（现有行为） |
| 其他 rc | `Unobservable` | 跳过（保守） |

现有 `run_tmux_ls` 的 `tmux ls … 2>/dev/null || true` **正好把 rc 丢掉了**，
把上表的五行全压成"空串或有内容"两行 —— 这就是 P1 要改的那一处。

**一处刻意的保守**：把 `rc=1` 直接判成 `ZeroSessions` 意味着"socket 权限异常"
这类罕见情形也会被判成零会话 ⇒ 理论上可能误 retire。缓解：socket 路径是 uid 隔离的
（`/tmp/tmux-<uid>/`），同 uid 下权限异常几乎不可能。**P3 落地后有更强的判据**——
daemon 那时持有 server 的 pidfd，"server 是否活着"由内核回答而不是靠解析 `tmux ls`
⇒ 届时 `rc=1` 但 pidfd 说活着 = 真异常 ⇒ 归 `Unobservable`。**这个升级不改帧契约**
（`ZeroSessions` 的语义不变），所以 P1 现在就能安全落地、P3 再收紧。

### ⑤ `notify-debouncer-mini` **静默吞掉 inotify 队列溢出**（坏答案，但不是本区引入的）

- `notify 6.1.1/src/inotify.rs:208-209`：`Q_OVERFLOW` → `Event::new(EventKind::Other).set_flag(Flag::Rescan)`，
  并提供 `Event::need_rescan()` ⇒ **底层报了**
- `notify-debouncer-mini 0.4.1/src/lib.rs:319-332`：`add_event` **只读 `event.paths`**，
  `kind` / `attrs`（`Flag::Rescan` 就住在 attrs 里）全丢；而溢出事件的 **`paths` 是空的**
  ⇒ `for path in event.paths` 循环体**一次都不执行** ⇒ 溢出**完全不可见**

**⇒ 定型 + 一处重要的范围订正**：主计划 §6 风险 3 写「零轮询下溢出无兜底」，
听起来像本工作区引入的风险。**实际是既有盲区**：
- 今天那条 2s tick **只扫内存里的 `state.sessions` 判活，不重扫目录**
- 目录发现（`SessionAdded`）与 jsonl 行流**本来就只靠 notify 事件、本来就没有周期兜底**
- **`pidfd` 对 inotify 溢出免疫**（内核直接通知进程死亡，不经 inotify 队列）

⇒ **P2 不会让情况变差，反而把判活这一路从 inotify 依赖里摘出来。**
溢出盲区登记为独立 BACKLOG 项；P3 可选加一个 raw-`notify` 溢出哨兵（额外一个
inotify 实例、只为拿 `need_rescan`）。**绝不为它补定时器。**

### ③（原编号）daemon 的 cgroup：不同锅，pidfd 扛得住 —— 但对 L1 不成立

```
tmux server (pid 3111899) : /user.slice/user-1000.slice/session-12881.scope
sshd (system)             : /system.slice/ssh.service
每个 SSH 登录             : /user.slice/user-1000.slice/session-<新 N>.scope（实测有 5 个活的）
tmux 每个 pane 子进程     : user@1000.service/tmux-spawn-<uuid>.scope（实测 6 个）
```

daemon 由 monitor 经 SSH exec ⇒ 落在**新的** `session-<N>.scope`，
与 tmux server 的 `session-12881.scope` **必然不同** ⇒
「tmux 整锅 SIGKILL」打不到 daemon ⇒ **P3 的 server 死亡路照做**。

**⚠ 一处必须写给别的工作区的话**：这条隔离**只对「daemon 经 SSH 起」成立**。
`local-as-remote` **L1（本地 = 不走 ssh 的远端）**里 daemon 可能与 tmux **同锅**，
届时 pidfd 探针会和 tmux 一起被端。**L1 落地时必须重新判这一格。**

### 附：P2 的依赖前置已确认

`libc 0.2.186` 在 `src-tauri/Cargo.lock` 里已解析 ⇒ daemon 加它符合
「版本钉在 lock 已解析过的、好离线」那条既有策略，**不需要联网**。

### 清场核对

| 项 | 结果 |
|---|---|
| 测试 socket（`p0canary`/`p0a`/`p0b`/`p0bc`/`p0d`/`p0e`/`p0f`） | 全部 kill + `rm` ✓ |
| 默认 socket 会话清单 | 与基线 **逐字未变** ✓（`cc-7c2a26d6` / `cc-9d66c46d` / `cc-d7692cdf`） |
| 默认 socket 已设 hook 数 | 仍为 **0** ✓（未碰） |
| 孤儿进程 | 无 ✓ |
| 安装的软件包 | 0 ✓ |
| 家目录文件改动 | 0 ✓ |

**一个自己踩的坑，留档**：清场时 `pgrep -f 'p0canary|…|evlog'` 报出 pid 724910，
一度以为有孤儿——实际是**调研 §11.2 记的 `pgrep -f` 自匹配**（发起命令的 shell 自己的
命令行含这些字面量）。改用 `ps -eo pid,args | grep -E '[p]0…'` 复核为无孤儿。
**另一处**：清场脚本里 shim 仍在 `PATH`，`tmux ls` 被拒 rc=97 ⇒ "after" 文件为空 ⇒
diff 报"有变化"。是脚本自伤不是真变化，改用 `/usr/bin/tmux ls` 复核后逐字未变。

## 4. 代码审计结果（Phase D）

**强度：低风险**（零产品代码，只写文档）⇒ 主线程自审 + 对照实验代替多 agent。
审的对象不是代码，是**实测的严谨性**。

### 自审抓到并已修的一处漏测（本项最重要的 D 产出）

④ 初测结论写成「『server 活着但零会话』**不存在**」。自审时问了一句
「这个结论依赖什么前提」——答案是 `exit-empty` 的默认值。补测证实
**`exit-empty off` 下该状态存在，且 rc 与"有会话"相同（rc=0）**。
若没补这一测，P1 会写出一个"rc=0 就当有会话"的分类器，在
`exit-empty off` 的远端主机上**永远不 retire**。

**方法论**：结论里出现「不存在 / 不可能」时，必须显式写出它依赖的前提，
再去查那个前提是不是可配置的。

### 逐项严谨性核对

| 项 | 判色前置做了吗 | 有对照吗 | 结论强度 |
|---|---|---|---|
| ① `@ccm_sid` 解析错 | ✓ 每次先 `show-hooks -g \| grep '\[50\]'` 确认 hook 设上 | ✓ 两会话版 + 三会话版（排除"第一个/最后一个"造成的假象） | **强**（两次独立复现，且 `hook_session_name` 同时是对的 ⇒ 排除"hook 整体没跑") |
| ② per-session `session-closed` 不触发 | ✓ `show-hooks -t A` 确认设上、`show-hooks -g` 确认没泄漏 | ✓✓ **`session-renamed` per-session 对照触发** ⇒ 排除"per-session 机制整体不 work" | **强** |
| ③ `-b` vs 同步 | ✓ 同上 | ✓ 四格全测（多个中一个/最后一个×后台/同步 + kill-server） | **强** |
| ④ 四态 | ✓ rc 与 stdout 分别捕获、`display-message -p '#{pid}'` 独立确认 server 真活 | ✓ 补测 `exit-empty off` | **强**（补测后） |
| ⑤ debouncer 吞溢出 | — | — | **中**：结论来自**读源码**（`add_event` 只读 `event.paths` + 溢出事件 `paths` 为空），**没有真触发过一次溢出**。代码路径无歧义，但**如实标注这是代码阅读结论、不是运行时观测** |
| cgroup | ✓ 直接读 `/proc/<pid>/cgroup` | ✓ 列出 tmux-spawn scope 与 session scope 两族 | **强**（结构性论证 + 实测） |

### 两处如实标注的未复测项

1. **pidfd 本身本轮未测**。跨 cgroup 存活是**用户调研已实测**的（其 §10 复现命令里
   有 `systemd-run` 双单元 + `SIGKILL` 那一组，结论 ✅）。**P2 必须自带 pidfd 的
   双向变异验收**（杀 → 事件到；不杀 → 事件不到），不许只引用调研。
2. **⑤ 没有真触发 inotify 溢出**（要塞满 16384 条默认队列）。若 P3 决定加溢出哨兵，
   那一步要自己造一次真溢出验收。

## 5. 工程审计结果（Phase E）

**P0 改了主计划的四处，全部是"设计被实测收紧"，没有一处是打补丁。**

| # | 主计划原文 | P0 之后 | 性质 |
|---|---|---|---|
| 1 | P0-① 好答案分支「死亡帧直接带 sid」 | **该分支不存在**，只能带名字 | 分支删除（不是二选一，是一条路被证伪） |
| 2 | P0-② 好答案分支「用 per-session，全局槽位不碰」 | **该分支不存在**，必须用全局 `[50]` | 同上。**用户已授权的正是需要的那条** |
| 3 | §6 风险 3「零轮询下溢出无兜底」措辞暗示本区引入风险 | **既有盲区**；且 pidfd 对溢出免疫 ⇒ **P2 让情况变好** | 范围订正（避免把既有债记到本区账上） |
| 4 | P1 哨兵叫 `NO_SESSIONS`、判据是"命令成功但零会话" | 叫 **`ZeroSessions`**，判据是 **rc + stdout 空否**，覆盖 ⓑ/ⓒ/ⓓ 三态 | 语义收紧 |

**一条新增的账本内容（§3 第 2 行的最终形态被 P0 定死）**：
`run_tmux_ls()` 的返回类型不能是裸 `String`，必须是四值枚举
`Sessions(raw) | ZeroSessions | NoTmux | Unobservable`。**P1 建立它**，P3 只加调用时机
与内部判据（pidfd 收紧 `Unobservable`），**不改这个契约** ⇒ 后面不会有人为了新场景
再往里塞第五种哨兵字符串。

**一条必须写给别的工作区的话**（跨工作区影响，已写进 §3 实测结果）：
cgroup 隔离**只对「daemon 经 SSH 起」成立**。`local-as-remote` L1 里 daemon 可能与
tmux 同锅 ⇒ pidfd 探针会被一起端。**L1 落地时必须重判这一格**，不能继承本区结论。

**最大遗产（给后面每一个功能）**：**hook 与 pidfd 的分界是实测出来的，不是设计出来的。**
hook 管「多个中杀一个」（`-b`，绝不阻塞用户 server），pidfd 管「杀到没了 / server 被端」
（hook 在这两格根本不触发）。任何人后来想"统一成一个机制"都得先推翻这四格实测。

## 6. 签收

- [x] 通过代码审计（低风险档：实测严谨性自审 + 对照实验；抓到并补掉 ④ 的漏测）
- [x] 通过工程审计（四处主计划订正 + 账本第 2 行最终形态定死 + 一条跨工作区提醒）
- [x] 主计划已据此更新（见 `../MASTERPLAN.md` §7 变更记录 03）

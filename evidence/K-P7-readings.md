# K-P7 读数与代价表 —— `KP7D1` / `KP7D2` / `KP7D3` 的全部答案

> **量于**：工作树 `/home/zbl/文档/claudecode-frontend/.claude/worktrees/k-p7` ·
> 分支 `track/k-p7` · 基点即入场 `HEAD = ab56c7d674c42ad8f45c301ccf260fa0c8934a03`
> （本文写作时**零源码改动**，`§⑨` 给三口径）。
> **量具**：`evidence/K-P7-protocol-census.py`（输出 `evidence/K-P7-protocol-census.out`）
> ＋ 下文逐条写出的 shell 命令。量具的被测对象**由 `--root` 决定，默认是它自己所在的那棵树**，
> 输出头上印着「被测工作树 + HEAD」（`brief` 12「量具住址要能唯一定位到那一份」）。
> **本文里的行号一律带校验位**（旁边抄那一行的逐字内容），或钉在上面那个 sha 上（`brief` 13c）。
> ⚠ **本件是摸底件：零生产改动、不给裁定。** 出口与选路归 PM / 用户（`KP7D3` 明写）。

---

# ① `KP7D1①` —— `K-P1 KPY8` 那条「不做 + 触发器」，逐字读完

## 甲 · 原文住址（三处，都在计划仓 `.claude/planned-build/backend-consolidation/`）

| # | 住址 | 逐字内容（校验位） |
|---|---|---|
| 1 | `features/K-P1-后端常驻化.md:572` | `### KPY8 「多客户端的流」不做，但要留**触发器**而不是留一句话` |
| 2 | `features/K-P1-后端常驻化.md:591-592` | `- **不做「多客户端的流」** —— 只做 \`§0b-2㈢\` 那两档：一条流 + 不限次的「只读 hello 就走」。` / `  fan-out 与 \`Overflow.lost\` 丢帧账的重定义**明确不做**，由 \`KPY8\` 的触发器看着。` |
| 3 | `features/K-P1-后端常驻化.md:431-432` | `⇒ 一件做完，第一刀按 \`§0b-2㈢\` 的两档连接切；多客户端的**流**（fan-out + 丢帧账重定义）` / `  留成本件明确的 \`不做什么\`，并按 \`KPY8\` 立触发器。〕` |

落到代码上的那一份：`remote-daemon-proto/src/single_stream_guard.rs:1`
逐字 `//! \`K-P1 KPY8\`：**「多客户端的流」本件明确不做 —— 留一个触发器，不留一句话。**`

命令（可重跑，**没有 `head`/`tail` 在替我截答案**）：
```
grep -rn  "KPY8" <计划仓/backend-consolidation> | grep -v "K-P7-协议补" | wc -l   ⇒ 12 行
grep -rln "KPY8" <计划仓/backend-consolidation> | grep -v "K-P7-协议补" | wc -l   ⇒  5 份
sed -n '572,580p;588,594p' .../features/K-P1-后端常驻化.md
```
🔴 **口径必须排除本件件文件** —— 我在它的 `§8` 里写 `KPY8`，
**不排除的话这个数每写一段就涨一次**（`brief` 12：一句话里嵌了每轮都变的量，
下一轮自动变成假话；实测本轮它从 17 涨到 18）。**12 / 5 这个数才是稳定的那一个。**
逐份（`grep -rc`，排除 0，排除本件）：
`K-P1 8 · K-P6 1 · INDEX.md 1 · audits/K-P6-PM.md 1 · audits/K-P1-D1.md 1`。
⚠ 自查记一笔：**本条第一版写的是「11 行 / 5 份」，那是一条带 `head -50` 的命令目测出来的**
（`brief` 12 那族 —— 今天现打的三条教训之一），**当轮自己抓到并改了**；
改的过程里又踩到「数把自己算进去」那一格，一并治了。

## 乙 · 三问逐条答死

### 问 ①「**当时为什么不做**」

原文给的理由**不是「太难」，是「账要重新定义，而那不属于搬运」**。逐字两条：

- `K-P1 §0b-2㈢` 第 2 点（`features/K-P1-后端常驻化.md`，逐字）：
  「N 个连接要 fan-out，而 **`Overflow.lost` 的丢帧账是按那一个通道记的**…
  ⇒ 分流之后「丢了哪几帧」这本账要**重新定义**。★ **这是语义变更，不是搬运。**」
- 同节 `⇒ ★★ 最省的第一刀`：连接分两档（「流」只许一条 · 「只问一句」不限），
  逐字「**上面 1/2/3 一处都不用动**」——
  也就是说 `K-P1` 是**用一个分档把这三处硬编码整个绕过去了**，不是解决了它们。

⇒ **当时不做的理由，一句话**：常驻这一件要买的是「换载体」，而 fan-out 要付的是「重新定义一本账」，
两者不是同一笔钱；`K-P1` 选了绕开，并把没付的那笔钱**记成一个会响的触发器**。

### 问 ②「**留的触发条件是什么**」

**逐字**（`features/K-P1-后端常驻化.md:577`）：
「判定：`§0b-2㈢` 明确不做的那一半（**第二个要「流」的客户端**）一出现就红」。

落到机器上的那一份（`single_stream_guard.rs` 的失败文案，逐字）：
```
★ **这多半不是 bug，是提醒**：`K-P1 §2` 明确不做「多客户端的流」——
fan-out 与 `Overflow.lost` 丢帧账的重新定义留给**第二个要「流」的客户端**出现那天。
真到了那天：先立件，把那本账重新定义清楚，再回来改这张表；
只是重构动到了这几处：把新的数与理由一起写进来。
```
⇒ 🔴 **触发条件是「第二个要『流』的客户端」出现**，
而触发之后**规定的动作是「先立件重新定义那本账，再回来改这张表」**——
**它明写着「回来改这张表」是合法的**。这一条对 `KP7D2②` 是决定性的，见 `§④-乙`。

### 问 ③「**今天满足了没有**」——🔴 **没有满足，而且本件的题面也不会满足它**

⚠ 这一格是本件最容易被将错就错的一格。**答案要分成两问**：

| 问 | 读数 |
|---|---|
| 今天本机 daemon 上有没有第二个要「流」的客户端？ | **没有**。`listen::admit` 那一臂逐字 `Verdict::Attach if stream_taken => Admit::Refuse(REFUSE_BUSY)`，`main.rs` 的发牌只有 `busy.swap(true` **一处**（本文 `§②` 现打 = 1）。今天口上只挂得住一条流，第二条被出声拒掉。 |
| `K-P6`/`K-P7` 要的**是不是**「第二个要流的客户端」？ | 🔴 **不是。** 它要的是**本机 daemon 自己去当另一台 daemon 的客户端**，再把收来的帧**汇进**它对界面那条流。 |

**这两件事的方向相反，账也不一样**：

- `KPY8` 守的是 **fan-out** —— **一条**观测通道（`mpsc::channel::<Frame>(CHANNEL_CAPACITY)`）
  要喂 **N 个**消费者 ⇒ 那本按「一条通道」记的丢帧账被 N 份消费者分掉 ⇒ **必须重新定义**。
- `K-P7` 要的是 **fan-in** —— **N 个**上游（远端 daemon）各自一条通道，汇进**一条**下游（界面）。
  每个上游各有各的通道、各有各的 `Overflow` 账，**语义不用重新定义**；
  缺的只是「这一帧/这本账是**谁的**」——那是一维**新字段**，不是一次语义变更。

⇒ 🔴 **`KPY8` 的触发器今天没有响，本件也不该把它读成「今天满足了」。**
但它**旁边那根针**会响：`PINS` 里 `observe/watcher.rs` / `mpsc::channel::<Frame>(` 那一条，
全 crate 登记 **3**，逐字写着「**第 4 处出现时回来重判它是哪一种**」——
fan-in 的转发通道就是那个第 4 处。**它响的时候，正确动作是那条针自己写的「回来重判」，不是撞墙。**

⚠ **诚实边界**：以上是**读文本读出来的**（三处原文 + 守卫头注 + 失败文案 + 那条 `why`），
不是跑出来的。「fan-in 不需要重定义 `Overflow.lost`」这句话我**没有**用一次实现去证；
它是从 `LostFrame` 的字段与 `loss_is_recoverable` 的分组推出来的（读数在 `§②-丁`），
而**推的那一步在这里**：`subject` 只装 sid / tmux 会话名，跨机之后 sid 与会话名**可能重名**
⇒ 精确地说是「**不需要重定义，但需要加一维 origin，否则 `subject` 跨机歧义**」。
**这一格 PM 若要更硬的证据，得等一次真实现，本件给不了。**

---

# ② `KP7D1②` —— `§0a` 那四个读数逐条重打

量具：`evidence/K-P7-protocol-census.py`（输出 `.out`）。
被测树 `HEAD = ab56c7d`（量具自己把它印在输出头上）。

## 甲 · `Frame` 变体数 ⇒ ✅ **11，PM 对**（PM 复核过的两个之一）

**两把独立的尺子，同值**（`§3 P7M1` 要的就是这一格）：

| 尺子 | 口径 | 读数 |
|---|---|---|
| ① 声明面 | `pub enum Frame {` 内部、大括号深度 0、行首即标识符 | **11** |
| ② 穷尽 match | `impl Frame` 的 `loss_is_recoverable` 的 `Frame::X { .. } =>` 臂 | **11** |
| ②′ 穷尽 match | 同上，`loss_identity` | **11** |

三者**逐个名字相同**（差集两侧皆空）：
`Hello`(wire.rs:114) `Line`(:223) `SessionAdded`(:246) `SessionStatus`(:296) `SessionRemoved`(:307)
`TurnEnd`(:335) `TmuxSessionClosed`(:353) `TmuxSessions`(:359) `Overflow`(:382) `Reply`(:401) `Cancelled`(:415)
（行号钉在 `ab56c7d` 上；`wire.rs` md5[:8] = `6b96f08d`，全文 1489 行。）

**分母**：`remote-daemon-proto/src/wire.rs` 一份文件里那一个 `enum`。

## 乙 · `inbound::COMMANDS` 条数 ⇒ ✅ **8，PM 对**（PM 复核过的另一个）

住址 `remote-daemon-proto/src/inbound.rs:78`，逐字（校验位）：
```rust
pub const COMMANDS: &[&str] =
    &["bus-kill", "bus-list", "bus-send", "cancel", "kill", "launch", "ping", "resolve"];
```
条数 **8** · 字典序 **是**（`the_commands_mirror_matches_the_registry` 与 `COMMANDS` 有序那两条判据的口径）。
⚠ PM 写的是 `inbound.rs:78-79`，**两行都对**：`78` 是 `pub const` 那行，`79` 是字面量那行。

**分母**：那一个 `const` 的字面量元素数。**全部是 request/reply 形状**（每条走 `Request{id,cmd,args}` → `Frame::Reply`），
**没有一条是长连接 / 流** —— 这一句 PM 写对了，我复核成立。

## 丙 · 「三根针」⇒ 🔴 **不成立：`PINS` 现打是 6 条，不是 3 条**（PM 没复打的那两条之一）

`件文件 §0a②` 逐字：「`single_stream_guard` 三根针钉着「只有一条流」：`PINS` 现打 ——
`writer_task(` 恰好 2 · `inbound::spawn(` 恰好 2 · `busy.swap(true` 全 crate 恰好 1。」

**现打**（量具 `【3】`）：`PINS` 这张表**登记 6 条**。PM 点的是其中**前 3 条**，
那 3 条的数**逐个都对**，但**表只有一半被写进 `§0a`**：

| # | 文件 | 锚点 | 登记(文件内) | 我重打 | 登记(全 crate) | 我重打 | 判 | PM 的 `§0a` 提到没有 |
|---|---|---|---|---|---|---|---|---|
| 1 | `main.rs` | `writer_task(` | 2 | **2** | 2 | **2** | ✅ | 提到 |
| 2 | `main.rs` | `inbound::spawn(` | 2 | **2** | 2 | **2** | ✅ | 提到 |
| 3 | `main.rs` | `busy.swap(true` | 1 | **1** | 1 | **1** | ✅ | 提到 |
| 4 | `observe/watcher.rs` | `mpsc::channel::<Frame>(` | 1 | **1** | 3 | **3** | ✅ | 🔴 **没提** |
| 5 | `listen.rs` | `Admit::Stream` | 1 | **1** | 2 | **2** | ✅ | 🔴 **没提** |
| 6 | `main.rs` | `REPLY_BURST` | 2 | **2** | 2 | **2** | ✅ | 🔴 **没提** |

**分母怎么数的**：`PINS` 那张表的**元组个数**（表体由 `&[` … `]` 的括号配对切出，不是按行数）。
**全 crate 那一列的人群**：`remote-daemon-proto/src/**/*.rs` 共 **76** 份，
**扣掉调用者自己那份** `single_stream_guard.rs`（`scan_tree!` 逐字展开成
`scan_tree_excluding_self($root, $exts, file!())`）⇒ 分母 **75** 份。

**用什么量的（两把尺子）**：
- 尺子甲 = crate 自己那份（`the_single_stream_shape_is_still_exactly_one_client`），
  判决落在门禁 `daemon` 那一格：入场趟 **587 passed**（`§⑨`）。
- 尺子乙 = 本量具**复刻的** `guard_core::production_code`
  （`production_source` 剥 `#[cfg(test)] mod X {…}` → 剥块注释 → 删整行 `//` → 剥行尾 `//`）。
  **12 个数（6 条 × 两列）逐个相同**，`P7M1` 这一格两把尺子同值。

🔴 **两处顺带订正（都不是 PM 的账，但引它的人会栽）**：
1. `single_stream_guard.rs` 的模块头注写「它钉的是哪**三**处『恰好一个客户端』」，
   而 `PINS` 是 **6 条**。那不是矛盾 —— 头注说的是**三个概念**
   （`HelloFlushed` 见证 · 观测帧单消费者 · `writer_task` 让位预算），
   `PINS` 是**六个带次数的锚点**。⚠ 但正题那条测试的 doc 也写「三处」，
   而它的 `for` 循环跑的是 6 条 ⇒ **照头注读会少数一半**，`§0a` 就是这么少的。
2. ⚠ **本仓有两张都叫 `PINS` 的表，别混**：
   `single_stream_guard.rs:166` 那张（本件的，6 条，五元组）
   与 `ratchet_guard.rs:65` 那张（`KPY7` 的，**7 条**，四元组，语料只有
   `readonly_guard.rs` / `no_timer_guard.rs`）。
   命令：`grep -rn "const PINS" remote-daemon-proto/src/` ⇒ **2 处**。

## 丁 · `Overflow.lost` 的语义 ⇒ 现打如下（PM 没复打的另一条）

**字段**（分母 = 那个变体体内的字段声明）：
```
Frame::Overflow { dropped: u64, lost: Vec<LostFrame>, lost_truncated: bool }
LostFrame        { kind: &'static str, subject: Option<String> }
```
**记账单位**（逐字，`wire.rs:377-381` 的 `Overflow` 头注）：
「The bounded frame channel back-pressured and the reader had to drop `dropped` frames
(a slow/wedged SSH pipe). Emitted once when the channel drains enough to accept it…
`dropped` counts frames dropped since the last overflow signal.」
⇒ 单位是**一条有界帧通道**（`observe::watcher` 那条 `CHANNEL_CAPACITY`）。

**`lost` 装谁**（`loss_is_recoverable` 的穷尽分组，现打）：
- 可恢复 **3**：`Line` · `TurnEnd` · `TmuxSessions`（不进 `lost`）
- 不可恢复 **8**：`SessionAdded` · `SessionRemoved` · `TmuxSessionClosed` · `SessionStatus` ·
  `Hello` · `Overflow` · `Reply` · `Cancelled`（进 `lost`，带 `kind` + `subject`）
身份表上界 `LOST_IDENTITY_CAP`（住 `observe/watcher.rs`），触顶置 `lost_truncated`。

🔴 **本件真正关心的那一句（`§0a③` 说「要重定义」，我复核的结论要收一格）**：
`subject` 的取值空间逐字是「会话 sid / tmux 会话名」，**没有任何一维说「哪台机」**。
⇒ 于是：
- **fan-out**（一条通道喂 N 个消费者）⇒ 那本账真的要**重定义** —— `§0a` 说的是这一种，成立。
- **fan-in**（N 台机的帧汇进一条下游）⇒ 每台机各有各的通道、各记各的账，
  **语义不变**；坏的只有 `subject` 的**唯一性**（两台机上可以有同名 tmux 会话）
  ⇒ 要的是**给帧加一维 origin**，不是重定义这本账。
⇒ 🔴 **`§0a③` 那句「`Overflow.lost` 要**重定义**才装得下第二条流的丢弃计数」，
在 fan-in 这条路上**收窄成「要加一维，不必重定义」**。这一格影响的是本件的代价量级，见 `§④`。

⚠ **诚实边界**：这是从字段与分组**推**出来的，`fan-in` 这条路今天盘上一处都没有 ⇒
没有活体可切。**要更硬只能等一次真实现。**

## 戊 · 顺带复打「用户裁定那三格」——🔴 **格 1 的文件数错了，格 2、格 3 成立**

| PM 的格 | 我打的 | 判 |
|---|---|---|
| 格1 `relay/` **11 份文件** / 8514 行 | 文件 **12 份** / 行 **8514** | 🔴 **文件数 11 应为 12**；行数对 |
| 格2 `relay/` 里 `Frame::` / `wire::` 命中 **0** | **0**（分母 = 那 12 份的**全文**，注释也算） | ✅ |
| 格3 SSH 拨号还在界面进程（`ssh_source.rs`） | ✅（`connect_and_exec` 只在 `ssh_source.rs` 里，`:3838` 逐字 `let stream = match connect_and_exec(cfg, with_bg, tail_only).await {`） | ✅ |

**分母**：`remote-daemon-proto/src/relay/` 下 `*.rs`，**子目录 0 个**。逐份行数（`ab56c7d`）：
`bind_guard 71 · creds 416 · creds_guard 438 · http1 666 · mod 155 · nodelay_guard 87 ·
route 290 · server 4155 · table 740 · table_guard 411 · tee 595 · upstream 490` ⇒ 12 份 / 8514 行。
⚠ 行数 8514 **逐字对上**而文件数差 1 ⇒ 多半是 `wc -l relay/*.rs | tail -1` 取了 total 行、
份数另数了一次；**`brief` 12 那条「同句给分母」正是治这个**。

🔴 **格 1 还有半句要订正，而这半句对本件是要害**：
PM 写「HTTP 中转**已经在后端那一侧了** …… 编在**同一个二进制**里」。
- 「同一份代码 / 同一个二进制」**成立** —— `--relay` 是 `remote-daemon-proto` 的一条分派臂
  （`remote-daemon-proto/src/main.rs:1197` 逐字 `Some("--relay") => relay::run(&agent_home, &args),`）。
- 🔴 但**进程上它不住在本机后端里**：它是**界面起的第三个进程**。
  `src-tauri/src/local_daemon.rs:1512` 逐字 `pub fn start_local_relay(bin: std::path::PathBuf) -> bool {`
  → 体内调 `local_backend::supervise(bin, relay_child_args(), relay_child_envs(), …)`
  → `src-tauri/src/backend/control/local_backend.rs:376` 逐字 `let mut cmd = std::process::Command::new(&bin);`
  ⇒ **`fork/exec` 出来的另一个 OS 进程**，父进程是**界面**，不是本机 daemon。
  `relay/mod.rs:46` 自陈逐字：「`--relay` 住**一次性子命令**分派臂（`main.rs`），是**独立进程**」。
⇒ **准确说法**：中转的形状是「**同一份代码 · 另一个进程 · 由界面监护 · 界面与它之间零协议**」，
不是「住进本机 daemon 里」。这一格决定了「把 SSH 挪过去做**邻居**」——**邻居是谁的邻居**，
而那正是 `§④` 那张代价表分岔的地方。

## 己 · 🔴 顺带核出来的一条：**`K-G3 §143` 这条引用指错了地方**

件文件 `§1 KP7D2②` 与 `§2-2` 两处都写「改那几根针 = 改判定口径，`K-G3 §143` 明禁」。
`brief` 13 逐字要求「引用一条定框 / 判据时把**整条读完**，并写明它**排除了**什么」⇒ 我读了整条：

`.claude/planned-build/backend-consolidation/features/K-G3-门禁自己的三个洞.md:143` 逐字（校验位）：
```
- **不动那五道已有的门的判定口径**（`run_gate_sum` 的「包数 ≠ N」与「0 passed 不是绿」两条自检**一个字不许改**）。
```
它点名的 `run_gate_sum` 现打是 **`scripts/gate.sh` 里的一个 shell 函数**
（`src-tauri/src/shared_crate_registry.rs:236` 逐字「`K-H2a` 给 `scripts/gate.sh` 加了 `run_gate_sum cargo <N>` —— `N` 是**包数相等断言**」）。
命令（**无 `head`**）`grep -rn "run_gate_sum" <本树> --include=*.rs --include=*.sh --include=*.ts | grep -v node_modules`
⇒ 命中住 **3 份文件**：`scripts/gate.sh`（定义与调用）· `src-tauri/src/capability_registry.rs` ·
`src-tauri/src/shared_crate_registry.rs`（后两份是**盯着 `gate.sh` 那个数**的判据）。
⇒ 🔴 **它守的是门禁那五道门，射程里没有 `single_stream_guard`。**
⚠ 自查记一笔：本条第一版写「全在 `shared_crate_registry.rs` 与 `gate.sh` 上」，
那是一条带 `head -6` 的命令目测出来的（漏了 `capability_registry.rs`），**当轮自己抓到并改了**。
**结论不受影响** —— 三份全在门禁那一侧。

**核它有没有漂**（`brief` 13c：行号要带校验位；`brief` 11：判「动没动」不许只靠一条 `grep`）：
```
git log --format=%h -- <该文件> | wc -l                       ⇒ 5（全部版本，没有 head/tail 截断）
逐版本 sed -n '143p'  ⇒ 173971f / ea8e64c / 157e24e 三版逐字相同；80e7a58 / 2206c0e 那时文件还短
git log --oneline -S "K-G3 §143" --all                        ⇒ 7 个提交，最早的是 d3eace5「立件 K-R22」
```
⇒ **不是漂了，是从一开始就指的别的东西。**
⚙ 同一条引用在别处是**对**的：`features/K-R22-门禁红了不说为什么红.md:98` 逐字
「🔴 **不动那五道门的任何判定口径**（`K-G3 §143` 的硬边界）」——那是门禁的账。
到 `aae8bb3`（`K-P6` 交回、立 `K-P7`）那一拍它被借去说那几根针，**借错了对象**。

**那几根针真正的约束在哪**（三处，都读完了）：
1. **它们自己的失败文案**（`single_stream_guard.rs`，逐字）：「真到了那天：先立件，把那本账重新定义清楚，
   **再回来改这张表**」⇒ **改表是它设计里的合法动作**，条件是先立件把账写清。
2. `KPY7`「本件一行都不许放宽已有判据」—— 那是 **`K-P1` 那一件的** DoD，**射程是 `K-P1` 那一轮**。
3. 机器那一档：`remote-daemon-proto/src/ratchet_guard.rs:65` 那张**同名 `PINS`**
   （逐字 `    const PINS: &[(&str, &str, usize, &str)] = &[`，四元组，现打 **7 条**），
   语料由 `source_of` 写死，逐字只有两支 ——
   `:135` `"readonly_guard.rs" => include_str!("readonly_guard.rs"),` ·
   `:136` `"no_timer_guard.rs" => include_str!("no_timer_guard.rs"),`
   （`grep -n "include_str!" ratchet_guard.rs` ⇒ **恰好这两行**）⇒ **不含 `single_stream_guard.rs`**。

⇒ 🔴 **今天没有任何机器禁止改 `single_stream_guard::PINS` 的那 6 条**；禁令只在纪律那一档。
**我查了这几条路**（`brief` 14：说得出查了哪几条，说不出就只能写「我没查」）：
① `grep -rn "const PINS" remote-daemon-proto/src/` ⇒ **恰好 2 处**
   （`single_stream_guard.rs:166` 逐字 `    const PINS: &[(&str, &str, usize, usize, &str)] = &[`
    与 `ratchet_guard.rs:65`），两张表**互不引用**；
② `ratchet_guard` 的语料表（上面第 3 条），**逐支读完**；
③ `single_stream_guard.rs` 全文（657 行）读过 —— 它对自己那张表的唯一约束是
   `every_pin_is_a_live_anchor_not_a_zero`（`want > 0` 且 `crate_want >= want`），
   **那是防「把 want 改成 0 蒙混」，不是防「按新事实改数」**；
④ 计划仓 `grep -rn "K-G3 §143" .` ⇒ 现打 **8 行 / 4 份**，其中 **5 行是我本轮自己写进 `§8` 的**
   ⇒ **本轮之前是 4 行 / 3 份**：`K-R22:98`（引对了）· `audits/K-R22-PM.md:56`（引对了）·
   `K-P7:87` 与 `K-P7:112`（**引错了，就是本条报的这两处**）。四处**逐处读完**。
⚠ **诚实边界**：以上是**四条路**，不是「所有路」的穷举 —— 那个分母我给不出。
⚠ **本件一根针都没动**（零生产改动）。要不要改、怎么改，归 PM —— `KP7D3` 明写本件不自批。

---

# ③ `KP7D2` 第一问 —— **中转那个形状对 SSH 成不成立？**

## 甲 · 先把「中转形状」拆成两句，它们**不是同一句话**

PM 的题面写作「**界面完全不在字节路径上**」。它其实是两个独立的性质，中转两个都占，
而 SSH 那条流**只占一个**：

| 性质 | 中转 | SSH 那条流 |
|---|---|---|
| **P1 · 界面不碰传输字节**（不解帧、不拿句柄） | ✅ 界面只传一个端口 + 一个凭据路径 | ✅ **可以做到** —— 界面今天碰它只是历史，没有结构上的必需 |
| **P2 · 载荷的消费者不是界面**（后端消费完就丢） | ✅ 消费者是 claude 自己 | 🔴 **不成立** —— 载荷**就是界面的数据模型** |

⇒ 🔴 **「界面完全不在字节路径上」这句话对 SSH 只成立一半。**
而**塌代价的是 P1，不是 P2** —— 见丙。

## 乙 · ① SSH 那条流上今天有哪几类数据是界面要的（清单 + 住址）

**先钉分母**：「那条流」= `ssh_source.rs:3838` 起的那一条
（`connect_and_exec` 拿 `russh::ChannelStream` → `stream_loop` 一直读）。
它上面**只有 daemon 的 NDJSON 帧**，没有别的：
`connect_and_exec` 走 `Channel::into_stream()`，而 `ssh_source.rs:2104` 逐字
「`Channel::into_stream()` 只搬 `ChannelMsg::Data` —— **`ExtendedData`（= stderr）**…」
⇒ **stderr 与 exit status 在这条流上根本到不了界面**（那是 `connect_and_exec_capture` 那条**另一条**路的事）。

**清单 = `parse_frame` 的 11 个字面臂**（量具 `【6】`，与 daemon 侧 11 个变体**逐个对上**，两侧差集皆空）：

| # | kind | 界面要吗 | 落到界面的哪里（住址） |
|---|---|---|---|
| 1 | `hello` | **要** | 版本/能力协商 + `claude_dir`/`homes`；`ssh_source.rs:5893 InboundFrame::Hello {`、`inbound_client::DaemonHello::from_hello_frame` |
| 2 | `line` | **要**（热路径） | `Batcher::push` → `flush_lines(replay, app, …)` → 前端 tab |
| 3 | `session_added` | **要** | `session_changes.send(SessionChange{added,…})` → 建骨架 tab |
| 4 | `session_status` | **要** | 同上 `status_changed` → 红绿灯 |
| 5 | `session_removed` | **要** | 同上 `removed` + `announced.remove(&sid)` → 灰灯/归档 |
| 6 | `overflow` | **要** | `app.emit(REMOTE_HEALTH, …)` → toast「丢了约 N 条帧」 |
| 7 | `tmux_sessions` | **要** | `record_tmux_raw(origin, raw)` → 对账 |
| 8 | `tmux_session_closed` | **要** | 正向死亡帧 → 直接 retire |
| 9 | `reply` | **要** | `inbound_client::route_reply` 按 `id` 路由回请求方 |
| 10 | `cancelled` | **要** | `route_cancelled` |
| 11 | `turn_end` | **不要**（认识但刻意不消费） | `ssh_source.rs` 那一臂逐字 `"turn_end" => None,`，注释逐字「它是发给 **aterm** 的」 |

⇒ **11 类里界面真要 10 类**（分母 = `parse_frame` 的字面臂 11 个）。
⚠ 还有**一类不是帧**、但界面同样要：**这条流本身活没活**
（`stream_loop` 返回 `Err` ⇒ 重连 + `announced` 清算 + 断连归档）。它今天由「读到 EOF / 写不出去」表达。

## 丙 · ② 这几类能不能「由后端消费之后、用今天已有的协议帧告诉界面」

🔴 **这一问问反了一格，答之前必须先说破：那 10 类**本身就是**今天已有的协议帧。**

`ssh_source.rs` 从 SSH 通道上读到的，是**逐行 NDJSON 的 `wire::Frame`**——
与本机 daemon 发给界面的**同一套帧、同一份解析器**
（`local_daemon.rs` 有 8 处借 `ssh_source` 的帧解析族，`K-P6` 读数 `②` 已逐处列出）。
⇒ 「由后端消费之后再用协议帧告诉界面」在这条流上**不是一次转译，是一次转发**。

| 类 | 判 | 理由（逐类） |
|---|---|---|
| `line` `session_added` `session_status` `session_removed` `turn_end` `tmux_sessions` `tmux_session_closed` `overflow` `reply` `cancelled` | **能**（10/11） | 它们**已经是** `Frame` 的变体，原样转发即可；界面侧解析器一行不用改 |
| `hello` | **能，但必须带 origin 才不出错** | 🔴 远端的 `hello` 若原样出现在本机那条流上，界面会把它读成**本机后端的**（`§0a③` 那一格成立）。它不是「装不下」，是「装得下但**说不清是谁的**」 |
| 「这条流活没活」 | **能** | 今天由传输层的 EOF 表达；转发之后由「本机后端说远端断了」表达 —— 需要一个**说法**（新帧或 `origin` + 一个 removed 语义），**不需要字节** |

🔴 **没有任何一类非要界面看见原始字节不可。**
分母 = 上表 11 类 + 「流活没活」那一格，共 12 格；**12 格里 0 格要原始字节。**
理由是结构性的、不是巧合：**这条 SSH 通道上跑的本来就是协议，不是不透明的载荷。**

⚠ **反过来那半也如实写**（`KP7D2` 明写「反过来也一样」）：
**在「那条流」之外**，SSH 面上确实有**必须是原始字节**的东西，但它们**都不是界面要看的**：
- **端口转发**（`src-tauri/src/port_forward.rs:127` 逐字
  `let _ = tokio::io::copy_bidirectional(&mut tcp, &mut chs).await;`）——
  纯字节隧道，消费者是连上那个本地口的**第三方**，**界面一个载荷字节都不看**。
  ⇒ 这一条**就是中转形状，而且今天已经长在界面进程里了**。
- **SFTP**（`sftp.rs` / `sftp_pool.rs`）—— 文件字节，消费者是文件系统。
- **一次性 exec**（`connect_and_exec_capture`）—— 要 stdout/stderr/exit 三样，
  形状是 request/reply，**天然装得进 `Request`/`Reply`**。

## 丁 · ③ 所以：**三堵墙里，第一堵根本不用撞**

`§0a` 那三堵墙，逐堵重判：

| 墙 | `§0a` 的说法 | 本件的判 |
|---|---|---|
| **一 · 协议只有一问一答，11 个变体没有一个装得下双工字节流** | 成立（读数对） | 🔴 **不用撞** —— **从来不需要一条「双工字节流」变体**。要转发的是**帧**，不是字节；而帧的形状协议里已经有 11 个。这一堵是被题面（「把字节流转回界面」）造出来的，不是被需求造出来的 |
| **二 · 那几根针钉着「只有一条流」** | 成立（6 条，本文 `§②-丙` 重打） | **要不要撞，取决于选哪条路**（`§④` 逐条答；有一条路**一根都不碰**） |
| **三 · 协议没有 origin / target 这一维** | 成立 | 🔴 **只有这一堵是硬的、且每条路都要面对**（唯一例外是「每 origin 一条连接」那两条 —— 它们把 origin 留在**连接**上，就像今天一样） |

🔴 **`KP7D2③` 要的那句直话**：
**三堵墙里，第一堵（「协议装不下一条双工字节流」）本件判定为不必撞** ——
因为 SSH 那条流上跑的**就是这套协议本身**，要搬的是「谁来解这条流」，不是「怎么把字节隧道回界面」。
**本件的代价因此确实塌了一格**：从「给协议加一条流的形状 + 重定义丢帧账 + 解三根针」
塌到「**给帧加一维 origin / 或者干脆让 origin 继续由连接决定**」。
**但没有塌两格** —— 第三堵墙（origin）是硬的，`§④` 里每一条路都得为它付钱，
只是**付法不同**（加字段 vs 多开一条连接）。

⚠ **不许把这句话读大**：塌的是**协议面**的代价。
**拨号面**的代价一格没塌 —— 凭据、`ConnectStage` 那 6 个阶段的 UX、
`russh` 进不进 daemon 的依赖树、Windows 上有没有常驻口，一样都没少（`§⑤` 诚实边界）。

---

# ④ `KP7D2` 候选表 —— 五条，各答四问

**四问逐条**：①要改什么（哪几份文件、多大面）· ②那几根针怎么办 · ③老客户端认不认（additive 吗）·
④`Overflow.lost` 要不要重定义。

⚠ **面的量级怎么读**：下表的「份数」是**要动的文件数**（现打的住址），
「行」一律不给 —— 本件没有实现，**给不出行数就不写**（`brief` 12）。

## 甲 · 候选 A ·「中转形状」：后端消费、界面只收派生状态

**一句话**：本机后端拨号、收帧、**自己消费**，界面只拿一份「派生状态」。

- ① **要改什么**：全部 11 类帧的消费逻辑要从界面搬进 daemon
  （`stream_loop` 现打 **683 行**生产逻辑：`ssh_source.rs:3763`–`:4445`，
   `host_label` 在 `ssh_source.rs` 里现打 **98 处**），
  再发明一套「派生状态」帧把 UI 要的东西送回去。
- ② **针**：一样要碰（它仍然要在 daemon 里多一条 `Frame` 通道）。
- ③ **老客户端**：不认 —— 派生状态是全新的一套。
- ④ **`Overflow.lost`**：要**重新发明**（派生状态的丢失账与帧的丢失账不是一回事）。

🔴 **判：这条路不成立，本件不建议 PM 花时间在它上面。**
理由不是贵，是**它把界面已经有的 10 类帧再翻译一遍，翻译出来的东西和原件一样多**——
**载荷的消费者就是界面**（`§③-甲` 的 P2 不成立）。
中转能这么干，是因为它的消费者是 claude；SSH 这条流没有这个条件。
⚠ 这正是件文件 `§0-用户裁定` 那句「⚠⚠ 但两者有一个真差别，不许糊过去」说的东西 ——
**它是真的，我复核成立。**

## 乙 · 候选 B ·「加一个 `origin` 维 + 复用同一条流」（fan-in 汇进本机那条流）

**一句话**：本机 daemon 拨号、收远端帧、**贴上 origin 原样转发**到它对界面那条流上。

- ① **要改什么**（现打住址，**7 个面**）：
  1. `remote-daemon-proto/src/wire.rs` —— 11 个变体各加一维 `origin`（或提一个信封层）。
  2. `remote-daemon-proto/src/inbound.rs` —— `Request` 加 `target`；`COMMANDS`（8 条）要按 target 分派。
  3. `remote-daemon-proto/src/main.rs` —— 转发通道 + 拨号命令。
  4. **新** 一个 SSH 层（daemon 侧今天 `russh` 依赖图命中 **0**，见 ③ 的代价栏）。
  5. `doc/IPC-PROTOCOL.md`（现打 1138 行）—— 🔴 **不是可选的**：
     `protocol_doc_guard` 判「`wire.rs` 里每个 serde 字段名必须在文档里出现过」
     + `frame_variants()` / `documented_frame_kinds()` 对拍 ⇒ **漏一格当场红**。
  6. `src-tauri/src/ssh_source.rs` —— `parse_frame` 11 臂 + `InboundFrame` 11 变体 + 消费侧按 origin 分流。
  7. `src-tauri/src/inbound_client.rs` —— 今天 origin 靠 `register(origin, client)` /
     `client_for(origin)` / `LOCAL_ORIGIN`（`inbound_client.rs:684` 逐字
     `pub const LOCAL_ORIGIN: &str = "<local>";`）；改成「一条连接多 origin」之后这套要重写。
  ⚙ **一个有利读数（尺子是「子串 `origin`」，不是标识符 —— 别读大一格）**：
  origin 在界面那一侧**已经是一等公民**，在 daemon 那一侧**一处代码态都没有**。

  | 人群（`git ls-files -z` 去重） | 分母 | 含子串 `origin` 的份数 |
  |---|---|---|
  | `src-tauri/src/**.rs`（monitor Rust） | 106 | **49** |
  | `src/**.ts`（前端） | 341 | **107** |
  | `remote-daemon-proto/src/**.rs`（daemon） | 76 | **6** |

  daemon 那 6 份**逐处看过，代码态 0 处**：`watcher.rs` 3 处 + `proc.rs` 1 处命中的其实是英文词
  **`original`**（子串尺子的假阳，如实登记）；`fallback_guard.rs:603` 是测试名
  （逐字 `fn counterexample_b_the_origin_mine_is_alive_outside_the_population() {`）；
  `listen.rs:66` 与 `wire.rs:367` 是注释。
  🔴 **最要紧的那一处**：`observe/usage_query.rs:141` 逐字
  `// origin 不带——monitor 侧收到后盖主机 label。`
  ⇒ **daemon 自己写着「origin 由 monitor 盖」** —— 这一格不是我推的，是它自陈的。
  **候选 B 就是要把这句话反过来。**
- ② **针**：🔴 **要碰第 4 根**。转发通道 = 第 4 处 `mpsc::channel::<Frame>(`
  ⇒ 登记的全 crate **3** 变 4 ⇒ 那条针红。
  **两边都说清**（`KP7D2②` 要求的）：
  - **改它**：那条针的 `why` 自己逐字写着「**第 4 处出现时回来重判它是哪一种**」，
    失败文案逐字写着「真到了那天：先立件…再回来改这张表」⇒ **改这张表是它设计里的合法动作**，
    条件是**同轮把「这第 4 处是哪一种」写进 `why`**。
  - **不改它**：走不通 —— 除非把转发帧塞进**已有**的某条通道
    （应答通道 `REPLY_CHANNEL_CAPACITY=256` 或观测通道），而两者都会把两台机的账混在一起。
  ⚠ 另一根要一起看：`writer_task(` 登记 2（stdio 一条 + 常驻一条）。
  转发**不新增 writer**（同一个 writer 写出去）⇒ **这一根不动**。`busy.swap(true` 同理不动。
- ③ **老客户端**：🔴 **字节层面 additive，语义层面不 additive** —— 见 `§⑥ P7M2`，那是本件找到的反证。
  ⇒ 必须**用能力协商门控**（照 `capabilities` / `§26` 那条既有纪律），不能只靠「加个可选字段」。
- ④ **`Overflow.lost`**：**不必重定义，但必须跟着加 origin**（理由与读数在 `§②-丁`）。
  ⇒ **不撞**「语义变更不是搬运」那条头注 —— 加一维是 additive，不是改语义。

## 丙 · 候选 C ·「真开第二条流」：每个 origin 一条 daemon↔界面连接

**一句话**：本机 daemon 的监听口上，界面对**每台远端**再开一条连接，说「这条给我 origin=pi 的帧」。

- ① **要改什么**：`listen.rs`（307 行生产段）的分档 + `main.rs::serve_listening` 的发牌 +
  每连接一份 watcher/writer/reply 通道；界面侧 `inbound_client` 的 origin 注册表**几乎不动**
  （它本来就是按 origin 存连接的）。**`wire.rs` 一个字节不用改。**
- ② **针**：🔴 **要碰第 3、4、6 根**：`busy.swap(true`（发牌从「一张牌」变成「一台机一张」）·
  `mpsc::channel::<Frame>(` 全 crate 3→N · `REPLY_BURST`（预算要按连接算，那条 `why` 逐字
  「每连接一个 writer 之后这条预算要按连接算 —— 那时本行的数会变，**正好逼人回来重判**」）。
  🔴 **这条路正是 `KPY8` 触发器守的那件事本身**（第二个要「流」的客户端真的出现了）
  ⇒ 按 `KPY8` 的规定动作：**先立件把账重新定义，再改表**。
- ③ **老客户端**：**认** —— 线上字节一个都没变（协议本体没动），旧 monitor 只是不会去开第二条连接。
- ④ **`Overflow.lost`**：**不必重定义**（每条连接各一条通道、各一本账，与今天逐字同义）。

## 丁 · 候选 D ·「不搬流，只搬『拨号』这个动作」——🔴 **量出来是：不成立**

`KP7D2` 点名「第三条候选特别要认真答」，并且给了它自己的判据：
「它要答的是**拨号建立之后那条 `ChannelStream` 归谁持有**，如果答案仍是界面，那『解耦清楚』就没做到」。

**现打**（住址钉在 `ab56c7d`）：

| 问 | 读数 |
|---|---|
| 拨号之后那个句柄是什么类型 | `ssh_source.rs:1308` 逐字 `) -> Result<russh::ChannelStream<client::Msg>, String> {` |
| 谁接住它 | `ssh_source.rs:3838` 逐字 `let stream = match connect_and_exec(cfg, with_bg, tail_only).await {` —— **`stream_loop`，在界面进程里** |
| 它之后被怎么用 | `ssh_source.rs:3861` 逐字 `    let (stream, parked) = crate::inbound_client::split_and_park(stream);` —— **切成读写两半，两半都归界面**：读半边喂 reader task 解 NDJSON，写半边 `ParkedWriter` 收 hello 后变成能发命令的 `InboundClient` |

⚠ **自查记一笔（第 5 处）**：上面这个行号我第一版写的是 `:3858`，**错了 3 行** ——
那是拿 `sed -n '3838,…p'` 的输出**目测数出来的**，不是 `grep -n` 打出来的。
`brief` 13c 那条「指进本树的行号要么带校验位要么不写」**当场兑现了一次**：
旁边抄的那一行逐字内容让它**核得动**，于是这一处被逮住并改对。
⚙ 顺带用同一把尺子钉死分母：`grep -rn "\bconnect_and_exec(" src-tauri/src --include=*.rs`
⇒ **全仓 2 处**（`:1304` 定义 + `:3838` 唯一调用点），**没有第二个持有者**。

⇒ 🔴 **答案是「仍归界面」，而且是双工的两半都归界面。**
`ChannelStream` 是 `russh` 的类型 ⇒ 只搬拨号动作，**`russh` 一行都出不了界面 crate**
（现打：`russh` 在 monitor 的 `Cargo.lock` 命中 **1**、在 daemon 的 `Cargo.lock` 命中 **0**；
命令 `grep -c '^name = "russh"$' <lock>`）。
- ① 要改什么：只有拨号那一处，面最小。
- ② 针：**一根都不碰**。
- ③ 老客户端：**认**（协议零变化）。
- ④ `Overflow.lost`：**不动**。
🔴 **但 ①–④ 全绿买不到任何东西**：`ChannelStream` 归界面 ⇒「解耦清楚」没做到；
进程仍是界面 ⇒「独立跑」也没做到。
⚠ **本件不为了便宜就说它成立** —— `KP7D2` 那句警告命中，**量出来是什么就写什么**。

## 戊 · 候选 E ·「中转形状的**真同构**」：另起一个进程做**字节代理**，origin 仍由连接决定

**一句话**：把中转那个形状**逐条抄过来** —— 同一份代码里一条新分派臂（如 `--ssh-broker`）、
**独立进程**、界面只**配置**它（拨哪台、用哪把钥匙、听哪个回环口）；
它把远端 daemon 的 NDJSON **原样**接到一条回环 socket 上，
界面像今天读 `ChannelStream` 一样去读那条 socket。

这是把用户那句「**ssh 和 http 中转放在一块，归为通信协议方面的部分**」按中转**今天真实的形状**
（`§②-戊`：同一份代码 · 另一个进程 · 界面只配置 · 界面不在字节路径上）逐条落到 SSH 上。

- ① **要改什么**：
  1. `remote-daemon-proto/src/main.rs` 加一条分派臂（照 `Some("--relay") => relay::run(…)` 那一行的形状）。
  2. **新** 一个 `ssh/` 层（拨号 + 回环监听 + 双向 copy）。现成材料**都在树里**：
     回环监听 + 非回环 bind 守卫 + 在途连接上界 + 读写期限，`relay/server.rs` 全套；
     双向 copy 的先例是 `port_forward.rs:127` 那一行。
  3. `src-tauri/src/ssh_source.rs` —— `connect_and_exec` 那一处换成「连本地那个口」。
  4. `layering_guard` 要给新层登记（今天的层是 `observe` / `control` / `plugin` / `relay` + 顶层）。
  5. **`wire.rs` / `inbound.rs` / `doc/IPC-PROTOCOL.md` 一个字节都不用改。**
- ② **针**：⚙ **一根都不碰** —— 而且这不是巧合，是**可验证的结构事实**：
  6 根针的锚点全部住在 `main.rs` / `listen.rs` / `observe/watcher.rs`，
  而**字节代理里没有 `Frame`**（正如今天 `relay/` 里 `Frame::`/`wire::` 命中 **0**）。
  ⚠ **一个真条件**：新层**不许复用** `listen::Admit`（那条针全 crate 登记 2，复用即 3 ⇒ 红）
  ⇒ 它要有自己的握手，正如 `relay/server.rs` 有自己的。
- ③ **老客户端**：**认** —— 线上字节逐字节不变（转的就是原帧）。
- ④ **`Overflow.lost`**：**不动** —— 远端 daemon 自己那本账原样过境，记账单位没变。
- 🔴 **它买到什么 / 买不到什么（如实两栏）**：
  - 买到：`russh` 出界面进程 · 拨号与流住进「通信层」· 中转与 SSH **真的成了邻居**（同一份代码、同一类形状）·
    **协议面零改动** · **origin 仍由「哪条连接」决定，和今天一模一样**（第三堵墙绕过去了）。
  - 🔴 **买不到 P2**：界面**仍然**解那些 NDJSON 帧（它是载荷的消费者，这一点没有路能改）。
    ⇒ 「界面完全不在字节路径上」在这条路上也**只成立一半**，别把它说成中转的等价物。
  - 🔴 **凭据这一格挡着**：拨号要私钥 / ssh-agent，而 `K-P1 §2` 逐字
    「**不碰凭据面** —— `K11` 那条硬前置未满足」。中转那边这一格是**已经付过的**
    （`relay_child_envs()` 用 `CCM_RELAY_CREDENTIALS` **显式把路径交给子进程**，
    `local_daemon.rs:1501-1510`）⇒ **有现成范式，但那是 `K11`/`K-H2` 的账，不是本件能自批的。**
  - 🔴 **`ConnectStage` 那 6 个阶段过不去**：`ssh_source.rs:422` 逐字 `pub enum ConnectStage {`
    ——`Dialing`/`HostKey`/`Failed`/`Won`/`Auth`/`Established`，经 Tauri Channel 流给前端做泳道日志。
    它**只在 `test_remote_connection` 那条路上 emit**（daemon 流 / exec / SFTP 一律传 `None`，零事件）
    ⇒ **不在「那条流」上**，但只要 `russh` 要彻底出界面，这 6 格就得跨进程 ——
    而今天 11 个 `Frame` 变体里**没有一个装得下它**。
    ⚠ **这是本件找到的、唯一一类「界面要、而今天的协议帧装不下」的数据**，
    但它**不在 `KP7D2` 第一问的分母里**（第一问问的是「那条流上」）。**分开记，别混。**

## 己 · 五条并排（**代价表 —— 本件的产出到此为止，不选路**）

| | A 中转形状 | B origin 维 | C 第二条流 | D 只搬拨号 | E 字节代理 |
|---|---|---|---|---|---|
| ① 改协议本体 | 大改（新一套派生帧） | 🔴 改（11 变体 + `Request`） | **不改** | **不改** | **不改** |
| ① 要动的面 | 界面 683 行消费逻辑 + 新协议 | 7 个面（含 `doc/` 1138 行那份契约） | daemon 分档/发牌 + 每连接一套 | 1 处 | daemon 新层 + 界面 1 处 |
| ② 那几根针 | 要碰 | 🔴 碰第 4 根（3→4，**有合法改法**） | 🔴 碰第 3/4/6 根（`KPY8` 正题） | 一根不碰 | ⚙ **一根不碰**（条件：不复用 `Admit`） |
| ③ 老客户端 additive | ❌ | ⚠ **字节 additive · 语义不 additive**，要能力门控 | ✅ | ✅ | ✅ |
| ④ `Overflow.lost` | 重新发明 | 加一维（**不重定义**） | 不动 | 不动 | 不动 |
| 买到「解耦清楚」 | 是 | 是 | 是 | 🔴 **否** | 是 |
| 买到「独立跑」（Linux） | 是 | 是 | 是 | 🔴 **否** | 是（要走常驻那条路） |
| 买到「独立跑」（Windows） | 🔴 **判不了 —— 见 `§⑤`** | 同左 | 同左 | 否 | 同左 |
| 凭据面（`K11` 硬前置） | 挡着 | 挡着 | 挡着 | 不涉及 | 挡着（有中转的现成范式） |
| 🔴 零定时器铁律（`§④-辛`，**`§0a` 没写的第四堵**） | 要付 | 要付 | 要付 | 不涉及 | 要付（**可能只落在「登记」那一档**） |

## 庚 · `KP7D3` 要的那半：**`K-P6` 第二阶段的题面该怎么写**

`K-P6` 交回时给的三阶段（`features/K-P6-拨号搬进本机后端.md:237` 逐字）：
「㈠ 先给协议补 origin/target 维与「第二条流」的账…㈡ 再把 `connect_session` 搬进后端，
界面侧只剩「请后端拨一条」；㈢ 最后收 34 个层2 调用点。⚠ ㈠ 不先做，㈡㈢ 都落不了地。」

🔴 **本轮的读数改了 ㈠ 的前提**：㈠ 被写成「补协议」，而现打的结果是
**有两条路（C / E）协议面一个字节都不用改**。⇒ **㈠ 不是一件必做的事，是一个 PM/用户要做的选择。**

**⇒ 第二阶段的题面今天写得了，但它有三个版本，选哪个取决于 `§④-己` 那张表怎么裁。**
三个骨架（**只是骨架，PM 裁**）：

| 选了 | 第二阶段的题面骨架 | 它的第一条 DoD 该断什么 |
|---|---|---|
| **B**（origin 维） | 「**给出方向的帧加一维 origin、给入方向的 `Request` 加一维 target，并把能力协商门控一起做完**；本阶段**不搬拨号**，只把协议面与文档面（`doc/IPC-PROTOCOL.md`）落地，并把 `PINS` 第 4 条从 3 改成 4 且同轮写清『这第 4 处是哪一种』」 | 「旧对端拿到带 origin 的帧**不会静默当成本机的**」——`P7M2` 那条反证就是它的死值验 |
| **C**（第二条流） | 「**把 `listen` 的分档从『一条流』改成『一个 origin 一条流』**，`busy` 那张牌按 origin 发、`REPLY_BURST` 按连接算；协议本体一个字节不改。**同轮按 `KPY8` 的规定动作立件把 fan-in 的账写清楚**」 | 「两条流各记各的 `Overflow`，两本账不串」——要一个**活体**双流夹具，不能是空真 |
| **E**（字节代理） | 「**在 `remote-daemon-proto` 加一条 `--ssh-…` 分派臂 + 一个新层**，拨号与远端 daemon 的字节都住那儿，界面改成连一条回环口；协议面零改动、`single_stream_guard` 一根针不碰（**新层不许复用 `listen::Admit`**），并给新层补 `layering_guard` 的登记」 | 「界面 crate 里 `russh` 的代码态命中归零」——尺子直接用 `K-P6` 那把严格尺子 `evidence/K-P6-russh-ruler.py`。⚠ 它上一轮的读数是「21 处 / 3 份文件」，**这个数我本轮没重打，住址在 `evidence/K-P6-readings.md §③-乙`**（`brief` 13：转述一个读数要么自己重打、要么写明没重打 + 住址） |

**三条共同的那一段（选哪条都要，建议直接写进第二阶段题面）**：
1. 🔴 **凭据怎么交给拨号那一侧** —— `K-P1 §2` 逐字「**不碰凭据面** —— `K11` 那条硬前置未满足」。
   中转已经付过一次这笔钱，范式住 `src-tauri/src/local_daemon.rs:1501`
   （逐字 `pub(crate) fn relay_child_envs() -> Vec<(String, String)> {`），
   体内 `:1507` 逐字 `        envs.push(("CCM_RELAY_CREDENTIALS".into(), p.display().to_string()));`
   ⇒ 端口与凭据路径**显式**当 env 交给子进程；头注逐字给了理由：
   「由**写那份文件的那一侧**把路径说出来，别让它从环境里猜」。
   ⚠ **但 SSH 的凭据是私钥 / ssh-agent，不是一份 JSON**，这条范式**只能抄形状，抄不了内容**。
2. 🔴 **`ConnectStage` 那 6 格怎么跨进程** —— `ssh_source.rs:422` 逐字 `pub enum ConnectStage {`，
   6 个变体经 Tauri Channel 流给前端做泳道日志，**只在 `test_remote_connection` 那条路上 emit**。
   它**不在「那条流」上**，所以不进 `KP7D2` 第一问的分母；
   但只要 `russh` 要彻底出界面，这 6 格就得跨进程，而**今天 11 个 `Frame` 变体没有一个装得下它**。
   ⇒ **这是全件唯一一类「界面要、而今天的协议帧装不下」的数据。别把它和会话数据混在一格里。**
3. 🔴 **零定时器铁律那一格**（`§④-辛`）—— 题面里要逐条说清：哪几处是**超时上限**（登记就行，
   照中转「写成毫秒 + 登记 + 写解锁条件」那条形状）、哪几处是**节拍**（竞发错开 · 重连退避 ·
   `DAEMONLESS_POLL_INTERVAL` · 6 处 `Instant::now` 埋点 —— **这些搬进去就是把 `P0–P5` 的收益还回去**）。
4. **Windows 那一格照 `§⑤` 端给用户**，别在第二阶段里假装它解决了。

⚠ **本节不选路**（`KP7D3` 明写「选路归 PM / 用户，本件只给代价表」）。
上面三个骨架是**并列的三份草稿**，不是推荐。

## 辛 · 🔴 **B / C / E 三条都要付、而 `§0a` 一个字没写的那笔钱：零定时器铁律**

**病灶一句话**：**界面那一侧的拨号路径是靠定时器写的，而 daemon 那一侧明令禁定时器。**
两条铁律在「把拨号搬进 daemon」这条边上**正面冲突**，而 `K-P6`/`K-P7` 的 `§0a` 都没提到它。

- daemon 侧：`remote-daemon-proto/src/no_timer_guard.rs`，头注逐字
  「它守的性质是：daemon **自己的生产代码**里不出现会让线程 / 任务自己醒来的构件」。
  禁用清单（现读）：`thread::sleep` · `time::sleep` · `recv_timeout` · `time::interval` ·
  `Instant::now` · `Duration::from_secs`。例外要**逐条登记**进 `REGISTERED_DURATION_USES`
  （现打 **3 条**）并写解锁条件。
- 界面侧：`src-tauri/src/ssh_source.rs:3583` 逐字
  `// INVARIANT §10：唯一的等待是 tokio::time::sleep（async、非阻塞），绝不 std::thread::sleep。`
  —— **它把 `tokio::time::sleep` 当成允许的那一个**。

**现打**（量具 `【7】`，分母 = 各文件**生产段**，尺子是子串 ⇒ 这个数是**上界**）：

| 文件 | 禁用构件命中 | 里面最扎眼的几处 |
|---|---|---|
| `ssh_source.rs` | **21** | `tokio::time::sleep(RACE_STAGGER * i as u32).await;`（happy-eyeballs 竞发的错开起拨）· `const HANDSHAKE_DEADLINE: Duration = Duration::from_secs(45);` · `const DAEMONLESS_POLL_INTERVAL: Duration = Duration::from_secs(2);` ＋ `tokio::time::sleep(DAEMONLESS_POLL_INTERVAL).await;`（**一个真轮询**）· `tokio::time::sleep(backoff).await;`（重连退避）· `Instant::now` **6 处**（`[perf]` 埋点） |
| `port_forward.rs` | 1 | `tokio::time::sleep(std::time::Duration::from_millis(100)).await;` |
| `sftp.rs` / `sftp_pool.rs` | 0 / 0 | —— |

**分档**（哪些是真障碍、哪些只是要登记）：
1. **只要登记的**：纯超时上限那一类。⚙ **中转已经付过这笔钱、范式在树里** ——
   `REGISTERED_DURATION_USES` 3 条里有 2 条就是它的，而且**刻意写成毫秒**
   （`server.rs` 的 `Duration::from_millis(30_000)` · `upstream.rs` 的 `Duration::from_millis(600_000)`）
   来避开被禁的 `Duration::from_secs`。**抄这条形状就行。**
2. 🔴 **真障碍**：会**自己醒过来**的那几处 —— 竞发错开（`RACE_STAGGER`）· 重连退避（`backoff`）·
   `DAEMONLESS_POLL_INTERVAL` 那个轮询 · `Instant::now` 的 6 处 `[perf]` 埋点。
   这些**不是超时上限，是节拍**，而 `P0–P5` 那一整条工作线的全部目的就是把节拍从 daemon 里拔掉。
   ⇒ **搬它们进 daemon = 把那条线的收益还回去。**

**⇒ 对代价表的影响（逐条）**：
- **D**（只搬拨号）：不涉及 —— 它什么都没搬进 daemon。
- **B / C / E**：**三条一样要面对**。它**不推翻**任何一条，但它是一笔
  `§0a` 那三堵墙**之外**的钱，**必须写进 `K-P6` 第二阶段的题面**，否则实现方会在中途撞上。
- ⚙ **对 E 稍轻一格**：字节代理只需要「连上 + copy」，
  竞发/退避/埋点**可以留在界面那一侧**（界面决定什么时候叫代理去拨、拨不通什么时候重试）
  ⇒ 搬过去的只有「一次拨号 + 一个超时上限」，正好落在「只要登记」那一档。
  ⚠ **这一句是我从两侧的职责边界推的，没有实现可切** —— 别把它当成量出来的。

---

# ⑤ 诚实边界（本件**给不出**的几格）

1. 🔴 **Windows 上买不买得到「独立跑」——本件不解锁**（件文件 `§4` 已钉，我复核**成立、且没有改善**）：
   本机后端在 Windows 上**起得来**（原生 sidecar + stdio 监护），但**没有 `K-P1` 那条常驻监听口**
   ⇒ 上表「独立跑（Windows）」那一行，**五条路一样答不了**。
   ⚠ 这一格 `K-P6` 量过、PM 没复打，**本件也没复打**（要真 Windows 读数，`§2.4` 明禁起真 daemon）。
   **它是「判不了」，不是「不成立」。**
1b. **零定时器那一格我量的是「有几处禁用构件」，不是「搬过去要花多少工」** ——
   后者要一次真实现才知道（`§④-辛` 那句「对 E 稍轻一格」明标了是推的）。
2. **`russh` 进 daemon 的代价没量**：现打依赖图 —— `russh` daemon **0** / monitor **1**；
   `ring` 两边**各 1**；`rustls` daemon **1** / monitor **0**；`aws-lc-rs` 两边 **0**
   （命令 `grep -c '^name = "<pkg>"$' <lock>`，两份 lock 分别在 `remote-daemon-proto/` 与 `src-tauri/`）。
   ⇒ 加密后端那一格 daemon 已经付过（`ring` 在图里），
   **但 `russh` 本体会给 daemon 的依赖树加多少、`KG1` 成功标准② 那条「裸交叉编译 + 二进制量级不变」还成不成立，本件没量。**
   现打：daemon crate 里**没有**任何依赖数 / 体积的机检（`grep -rln "Cargo.lock\|cargo tree" remote-daemon-proto/src` ⇒ 4 份，都不是依赖闸）。
3. **代价表里的「面」是文件数，不是行数** —— 没有实现就给不出行数，`brief` 12。
4. **`§①-乙` 问③ 与 `§②-丁` 的两句关键结论是「读文本 + 推」，不是「跑出来的」**，各自的诚实边界写在原处。
5. **本件不给「哪条路更好」的裁定**（件文件 `§4` 逐字），`KP7D3` 的出口只给出口本身 + 理由。

---

# ⑥ `§3` 变异复验（摸底件的形状，三刀）

## P7M1 · `§0a` 任一读数用**另一把尺子**重打 ⇒ **两把同值**

| 读数 | 尺子甲 | 尺子乙 | 判 |
|---|---|---|---|
| `Frame` 变体数 | 声明面（`pub enum Frame {` 内深度 0 的标识符）⇒ **11** | 穷尽 match 臂（`loss_is_recoverable` / `loss_identity`）⇒ **11 / 11** | ✅ 同值，且**名字逐个相同**（差集两侧皆空） |
| `PINS` 六条 × 两列 = 12 个数 | crate 自己那份（`the_single_stream_shape_is_still_exactly_one_client`），判决在门禁 `daemon` 格 **587 passed** | 本量具复刻的 `production_code` | ✅ **12 个数逐个相同** |
| `relay/` 行数 | `wc -l` | 量具按 `\n` 计 | ✅ 8514 = 8514 |
| `relay/` 文件数 | PM 写 11 | 量具数 `*.rs` ⇒ **12** | 🔴 **不同，写出来了**（`§②-戊`） |

**非空对照**（`brief` 14w②：「差集为空」要附对照）。住址 `evidence/K-P7-zero-diff-mutation.py`
的 `p7m1_control`，逐字输出在 `.out` 里（**只读，不改被测树**）：

| 锚点 | `observe/watcher.rs` 内 | 全 crate（分母 75 份） | 应当 |
|---|---|---|---|
| `mpsc::channel::<Frame>(`（登记的那一个） | 1 | 3 | 与登记的 1 / 3 相同 ✅ |
| `mpsc::channel::<Frame2>(`（改一个字） | **0** | **0** | 归零 ✅ |
| `mpsc::channel::<`（放宽） | **2** | **6** | 变大 ✅ |

⇒ 三个锚点读出三个不同的数 ⇒ **尺子不是恒返回登记值**，`§②-丙` 那 12 个「相同」不是空真。

## P7M2 · 给 `KP7D2` 任一候选的「老客户端认不认」找**反证** ⇒ 🔴 **找到了**

**被切的主张**：候选 B（origin 维）的 ③ 第一版写的是「**additive，老客户端认**」。

**反证一（出方向）**：旧 monitor 拿到一帧 `{"kind":"hello",…,"origin":"pi"}`，
`parse_frame` 对**未知字段**的口径逐字（`ssh_source.rs` 头注）：
「**多余 / 未知字段**（如 hello 里的 `build_id`）→ 忽略，不影响解析」
⇒ 它**不会报错，会把远端那台的 hello 当成本机后端的** —— 那正是 `§0a③` 说的「尤其致命」。
**忽略未知字段在这里不是降级，是一个自信的错答案。**

**反证二（入方向）**：`wire::Request` 现打**没有** `deny_unknown_fields`
（`remote-daemon-proto/src/wire.rs:496-497`，逐字 `#[derive(Debug, Clone, Deserialize)]` / `pub struct Request {`）
⇒ 旧 daemon 收到 `{"id":"x","cmd":"kill","args":{…},"target":"pi"}` 会**丢掉 `target` 在本机执行**
—— 一次**静默的杀错机器**。
⚠ 对照组（证明这条反证不是空真）：本仓**确实有** `deny_unknown_fields` 的 wire 面 ——
`src-tauri/src/backend/control/launch_wire.rs:29` 逐字 `#[serde(deny_unknown_fields, rename_all = "camelCase")]`。
同族现打 **14 处** `#[serde(… deny_unknown_fields …)]`，命令（**没有 `head` 截答案**）：
`grep -rn "deny_unknown_fields" src-tauri/src --include=*.rs | grep '#\[serde' | wc -l` ⇒ 14。
⇒ 「这套 wire 是宽容的」是**协议帧那一份**的性质，**不是全仓的性质**。
⚠ 自查记一笔：本条第一版写「5 处」，那是一条带 `head -12` 的命令目测出来的，**当轮自己抓到并改了**。

⇒ **主张改成**：「**字节层面 additive（不 bump `PROTO_VERSION`，旧解析器不炸）；
语义层面不 additive（忽略 origin/target 的旧对端会给出错答案）** ⇒ 必须走能力协商门控。」
**P7M2 判：反证找到了 ⇒ 原主张不算数，已改。**

## P7M3 · 改一行不提交 ⇒ `KP7D4` 三个口径**各认得出**

台子：`evidence/K-P7-zero-diff-mutation.py`（逐字输出 `.out`）。
锚点 = `remote-daemon-proto/src/wire.rs` 末尾**追加一行**（命中 1、切第 1 处），
`try/finally` 复原，**复原后再跑一次三口径**。切前 md5 `6b96f08d6226…`、
切后 `8ec363335741…`（不同 ✅ = 变异真落地了）、复原后 `6b96f08d6226…`（**逐字节回到切前** ✅）。

| 口径 | 切之前 | 切之后（应当认得出） | 复原之后 |
|---|---|---|---|
| ① `git diff ab56c7d -- src src-tauri e2e scripts remote-daemon-proto --stat` | 退出码 0 · 输出空 | 🔴 `remote-daemon-proto/src/wire.rs \| 1 +` / `1 file changed, 1 insertion(+)` | 退出码 0 · 输出空 |
| ② `git status --porcelain -- <那五个路径>` | 退出码 0 · 输出空 | 🔴 ` M remote-daemon-proto/src/wire.rs` | 退出码 0 · 输出空 |
| ③ 逐文件 md5（人群 `git ls-files -z`） | 两边都有 856 份 · **不同 0** · 单边独有 5 | 🔴 **不同 1** | **不同 0** |

**三刀三格全中 ⇒ `KP7D4` 不是空真。本轮零 CRASH**（三趟三口径的判定行都在，没有一趟是异常退出）。
⚠ **分母说明（别把它读成漂移）**：`.out` 里第 ③ 格的「单边独有」在**入场那一趟是 0、交回那一趟是 5**
—— 差的就是本轮新增并提交进 git 的那 5 份 `evidence/K-P7-*`。
**那是分母变了，不是漂移**（`brief` 12：「两次分母不同的哈希读起来跟一次漂移一模一样」）。
盘上现存的 `.out` 是**交回趟**那一份（单边独有 5）。

---

# ⑦ `7u` —— 把实现整个退掉，还有多少条新断言仍绿

**本轮没有生产实现可退**（零源码改动，`KP7D4`）。能退的只有本轮新增的两份 `evidence/` 文件，
而它们一退，本文里的读数**一条都不剩** ⇒ **`7u` 在本轮不适用**，
**不是「退了还绿」**。如实写，不冒充一次 `7u` 通过。

---

# ⑧ selftest 入场 / 交回

本件**零生产改动、零新判据** ⇒ selftest 与判定行的**新增格数 = 0**，标签多重集差 = 空。
门禁九格逐格读数在 `§⑨`（八格逐格相同，第九格见那里的说明）。

---

# ⑨ 盘上状态与门禁

## 甲 · 门禁（沙箱，`PB_WS=backend-consolidation .claude/devbox/gate <本树> k-p7`，**跑了三趟**）

| 格 | PM 给的基线 | ① 入场趟（`ab56c7d`） | ② 交回趟（`3b875d1`） | ③ 收尾趟（`e97c966`） |
|---|---|---|---|---|
| cargo | 1438 | **1438 passed** | **1438 passed** | **1438 passed** |
| generated | ok | **ok** | **ok** | **ok** |
| daemon | 587 | **587 passed** | **587 passed** | **587 passed** |
| npm | 1590 | **1590 passed** | **1590 passed** | **1590 passed** |
| ccm e2e ×4 | 12 / 8 / 264 / 72 | **12 / 8 / 264 / 72** | **12 / 8 / 264 / 72** | **12 / 8 / 264 / 72** |
| pb check | FAIL=0 BROKEN=0 | **FAIL=0 BROKEN=0** | 🔴 **FAIL=1 BROKEN=0** | **FAIL=0 BROKEN=0** |
| 总判 | `GATE: OK` | **`GATE: OK`** | 🔴 **`GATE: FAIL`** | **`GATE: OK`** |

🔴 **前八格三趟逐格相同、与 PM 给的基线也逐格相同** —— 这就是「零生产改动」在门禁上的读数。

**第九格那一红一绿，别读成「修好了」** —— 逐字写清它是怎么回事：
- ② 那一趟红的只有一条：`FAIL [J3 陈账] INDEX.md 比源文件旧 —— 重跑 pb index 落盘`。
  根因定死、不是猜（与 `K-P6` 交回那一趟**同形同因**）：
  `find . -name '*.md' -newer INDEX.md` ⇒ **恰好一份** `features/K-P7-协议补第二条流与origin维.md`；
  `INDEX.md` mtime `1788680903`(00:48:23) < 件文件 mtime `1788683364`(01:29:24)。
  ⇒ **是「我按件文件的要求写 `§8`」这一下顶红的**，与代码零关系。
- ③ 那一趟绿，🔴 **不是因为我修了什么** —— 是 **PM 在两趟之间自己跑了一次 `pb index`**
  （计划仓 `642493b`「订正 PM 的一个馊数：relay/ 是 12 份不是 11 份」那一拍前后，
  `INDEX.md` mtime 变成 `1788684349`(01:45:49)），而 ③ 恰好落在那之后、
  **落在我写下一段 `§8` 之前**（件文件 mtime `1788684382`，01:46:22）。
  ⇒ **它是一次赛跑的结果，不是一个修复。** 我此刻再跑一趟，第九格会**重新变红**。
- ⇒ **可搬走的那句话**：只要实现方还在写 `§8`，J3 那一格就会红；
  **它由 PM 在收窗口那一拍收（先 `freeze --verify` 再 `pb index`）**，不是实现方能关掉的。

🔴 **我一趟 `pb index` 都没跑**：它是**生成命令**，`brief` 19 逐字「窗口开着期间一概不跑生成命令
（`pb index` / `pb doc` 这一族会重写冻结面上的生成区）；收窗口那一拍**先 `freeze --verify` 再跑**」，
而且那整段住在「**三之二 · 只给 PM 的**」。

⚠ **分母如实抄门禁自己印的那句**：cargo 那格「本树未铺 `src-tauri/embedded-daemons/`
⇒ `embedded_daemons` cfg 不置 ⇒ 上面那个合计里少了『本地后端真的能起来吗』那一族（4 条）」。

## 乙 · 零改动三口径（交回趟现打）

**① `git diff ab56c7d -- src src-tauri e2e scripts remote-daemon-proto`**
⇒ **输出空 · 退出码 0**。
**非空对照**（`brief` 14w②）：同一条命令换基点 `ab56c7d~5`（= `5924b91`）
⇒ `6 files changed, 704 insertions(+), 5 deletions(-)`（`scripts/gate.sh` · `src/first-run-hint.ts` …）
⇒ **这条命令不是恒空的。**
⚠ **差点又踩一次 `K-P6` 记过的那个坑**：先拿 `ab56c7d~1` / `~2` / `~3` 做对照，**三个都是空** ——
那不是尺子坏了，是 `ab56c7d` 前后那几拍**只增 `evidence/`、不动源码**。
**空对照不是对照**，往回退到 `~5` 才拿到真的那一个。

**② 三处 `git status --porcelain`**（交回这一刻）：
| 处 | 输出 |
|---|---|
| 本工作树 `.claude/worktrees/k-p7` | ` M evidence/K-P7-readings.md`（本文，交回前的最后一次落盘，随后提交） |
| 代码仓主树 `cc-monitor` | **空** |
| 计划仓 `.claude/planned-build` | ` M backend-consolidation/features/K-P7-协议补第二条流与origin维.md`（`§8` 上报）—— **按 `brief` 3 不提交，留给 PM** |

**③ 逐文件 md5**（人群 `git ls-files -z`，本仓路径全是中文 ⇒ 不许用换行分隔）：
**两边都有 856 份 · md5 不同 0 份 · 只在 HEAD 有 5 份**（全是本轮新增的 `evidence/K-P7-*`）·
只在基点有 0 份。
⚠ 单边文件按 `brief` 12 **不进「逐字节相同」的分母**。
⇒ **主干完全可用**：`track/k-p7` 与基点 `ab56c7d` 在**所有源码文件上逐字节相同**。

## 丙 · 🔴 收工前自查 —— 拿本轮的病理回头打自己（`brief` 15）

本轮题面点名的三条教训里，头一条是「**报『有几处』前先看命令里有没有 `head`/`tail` 在替你截答案**」，
第二条是「**指进本树的行号要么带校验位要么不写**」。
收工前我拿这两条逐条回打自己写下的每一个数与每一个行号，**当轮自抓 5 处**，
逐处已在原地改掉并留了「自查记一笔」：

| # | 我先写的 | 现打（无截断） | 怎么栽的 | 结论受影响吗 |
|---|---|---|---|---|
| 1 | `KPY8` 命中「11 行 / 5 份」 | **17 行 / 6 份**（排除我自己写的 `§8` ⇒ **12 行 / 5 份**） | 命令带 `head -50`，目测 | 否（三处原文住址不变） |
| 2 | `deny_unknown_fields` 同族「5 处」 | **14 处** | 命令带 `head -12` | 否（反证只需存在性） |
| 3 | `run_gate_sum`「全在 2 份文件上」 | **3 份**（多一个 `capability_registry.rs`） | 命令带 `head -6` | 否（三份全在门禁那一侧） |
| 4 | `origin` 前端 TS「62 份」 | **107 份**（分母 341）；daemon 侧「只在一个测试名里」应为**6 份含子串、代码态 0 处** | git pathspec 的 `**` 与 `*` 同义 ⇒ 人群重了/漏了；且没逐处看命中 | **反而更强**：逐处看之后逮到 `usage_query.rs:141` 那句自陈 |
| 5 | `split_and_park` 那一行写 `ssh_source.rs:3858` | **`:3861`**（错 3 行） | 拿 `sed -n '3838,…p'` 的输出**目测**数行号，没用 `grep -n` | 否 —— **旁边抄了逐字内容 ⇒ 核得动 ⇒ 被逮住**（`brief` 13c 当场兑现一次） |

⚠ **第 4 处不只是数错** —— 它是「量具的作用域对不上事实」那一族：
我用**子串** `origin` 当尺子，而 `watcher.rs`/`proc.rs` 那 4 处命中的是英文词 **`original`**。
**逐处看过才发现**。⇒ 本文凡是用子串尺子的地方都写了「尺子是子串，不是标识符」。

⚠ **还有一条不是数、是方法**：`§⑨-乙` 的非空对照，我头三次挑的基点（`~1`/`~2`/`~3`）**都是空的**——
那正是 `K-P6` 上一轮末尾记下的那条（「差点把一个空对照当成对照用了」）。**同一个坑，同一个仓，第二次。**

**行号校验位的全表复核**（`brief` 13c）：收工前我把本文里**所有**「`路径:行号`」形逐条拿去对盘 ——
分母 = 从本文里采出、**解析得进本树**的那些（指向计划仓 / 别的树的一律不进分母）。
第一趟逮到 **4 处错行号**（`split_and_park` `:3858`→`:3861` · `relay/mod.rs` `:56`→`:46` ·
`wire::Request` `:495-501`→`:496-497` · `relay_child_envs` `:1503-1509`→`:1501-1510`），
**四处全部因为「旁边抄了那一行的逐字内容」而核得动**，逐处已改。
改后再对一趟：**去重后 32 处进本树，越界 0 处，逐处落在真行上**
（那 4 处改完之后重号，所以这个 32 与第一趟的分母不是同一个数 —— **分母变了，不是漂移**）。
⚠ **诚实边界**：复核脚本是**当场写的一次性检查**，没有落进 `evidence/`（它只做「行内容 vs 引文」对拍，
换一份文档就得改）；**它证明的是「这 35 处此刻对得上」，不是「本文再也不会漂」** ——
行号会随源码变，真正撑住的是旁边那段逐字内容。

## 丁 · 本轮新增的文件（全在写区 `evidence/`，**零 `.sh`**）

- `evidence/K-P7-protocol-census.py` —— 量具（`.py`，名带 `K-P7-`）
- `evidence/K-P7-protocol-census.out` —— 它的输出
- `evidence/K-P7-zero-diff-mutation.py` / `.out` —— `P7M3` 那一刀的台子与逐字输出
- `evidence/K-P7-readings.md` —— 本文

# U6a · `IPC-PROTOCOL.md` 先修再冻结 + 字段对拍机检

- 工作区：unified-backend · 主计划 §3 第四梯队（**U6 拆出的前半**）· 任务 #94
- 风险档：**中**（不改协议行为，只改文档 + 加机检）

## Phase B：U6 必须拆（粒度准则）

主计划 U6 一行里塞了约 10 件事：文档先修 · request-id · 取消 · 背压 · `version`+能力协商 ·
主键 opaque · argv 三分 + 死循环护栏扩 · `--resolve` 吸收 · 字段双向对拍 ·
「见 Hello 前不许写 stdin」机检 · 「命令处理器不许跑在流线程上」。

按 planned-build 的粒度准则（「一个功能应能在一份计划里讲清、几步内实现完」）这是 4-5 个功能。
且 **DoD 自己写着「文档先修」是前置** ⇒ 拆：

- **U6a（本功能）**：修文档 + **加字段对拍机检**。后者是防复发的那半 ——
  计划原话「没有它，那 8+5 条漏列一条都发不出来」。
- **U6b**：双向通道本体（request-id / 取消 / 背压 / 能力协商 / 两条机检）。依赖 U6a 的冻结基线。

## Phase B 核实：计划说的「七处在说谎」，我自己量了一遍，三处出入

**不采信计划的数字**（这一轮已被陈旧断言坑过三次：回边偏一个函数 · U0 的 DoD 一半不成立 ·
U4 的 DoD 无法验证）。逐条实测：

| 计划怎么说 | 实测 | 判定 |
|---|---|---|
| 「帧字段表漏 8 个线上字段**含刚冻结的 `attachable`**」 | 漏 8 个属实：`agent_kind` · `byte_offset` · `codex_dir` · `emits` · `kinds` · `liveness_confidence` · `next` · `observation`。**但 `attachable` 在文档里，没漏** | 数对，**括号里那句错** |
| 「一次性查询表漏 **5** 个子命令」 | 表里漏 **6** 个：`--account-trust-zero` · `--fork-session` · `--list-accounts` · `--search` · `--session-accounts` · `--tmux-notify`。其中 **2 个全文都没出现**（`--account-trust-zero` / `--tmux-notify`），另 4 个文档别处提过、只是不在那张表里 | **6 不是 5**，且要分「全文没提」与「表里漏」两档 |
| 「时序图画的正是已修掉的 v2.21 竞态 bug」 | 待逐图核（本轮做） | — |

## DoD

| # | 项 | 验收 |
|---|---|---|
| ① | 帧字段表补齐 8 个漏列字段 | 机检（见③）通过 |
| ② | 一次性查询表补齐 6 个漏列子命令 | 机检通过 |
| ③ | **字段名双向对拍机检** | 一条测试：`wire.rs` 的每个 serde 字段名都必须在 `IPC-PROTOCOL.md` 里出现；`main.rs` 分发的每个子命令都必须在文档的查询表里。变异：从文档里删一个字段 ⇒ 红 |
| ④ | 握手时序图与实际握手一致 | 逐图核；不一致就改，并写明改了什么 |
| ⑤ | 全量门禁绿 | 两侧 `cargo test` + `fmt` + `tsc` + `npm test` |

**不做**：不改任何协议行为 · 不动 wire 字段 · 不做 U6b 的任何一项（request-id / 取消 / 背压 / 能力协商）。

## 实现期与计划的偏离

护栏在自己上线前，就把**我写进计划里的四个数字**全推翻了。这是这一轮最值得记的一件事：
Phase B 我是用手工 grep 数的缺口，四个数没有一个对。

| 计划说 | 机检实测 | 差在哪 |
|---|---|---|
| 帧字段表漏 **8** 个 | **7** 个 | 手工那版多报了 `next` —— 它是 `SeqCounter` 的进程内每文件序号计数器，**根本不上线**。抽取器第一版扫全文件的 `name: Type` 形态，把它一起报了 |
| 查询表漏 **6** 个子命令 | 全文档零出现 **2** 个；查询表里缺 **3** 个 | `--list-accounts` / `--search` / `--session-accounts` 早在表里，手工那版看漏了 |
| （没提） | **1 处改过名没同步** | `tmux_sessions` 帧的字段文档里叫 `classification`，全仓（daemon + monitor）**没有任何东西叫这个名字** —— 线上真名是 `observation` |
| 「逐图核握手时序图」当成一条例行检查 | **图上四处全错，且错的方向一致** | 见下 |

### ★ 最重的一条：时序图描述的是**已经被修掉的那个 bug 的行为**

`doc/IPC-PROTOCOL.md` 的「跨进程握手时序图」画的是 v2 修复**之前**的握手：

| 图上 | 实际（`src-tauri/scripts/cc.ps1.tpl` + `bind.rs`） |
|---|---|
| 第 4 步写 await 文件、第 5 步设窗口标题 | **反过来**：先设标题、再写文件 |
| deadline 800ms（文档里三处 + UI 文案一处） | **3000ms** |
| notify-debouncer 100ms（两处） | **50ms** |
| EnumWindows 一次成 | 找不到会**重试 ≤600ms（12 × 50ms）** —— 图上整个没画 |

顺序那条是要害。`cc.ps1.tpl` 自己的注释写着：旧顺序下 monitor 的 notify 在文件落地瞬间就
EnumWindows 找 marker，**扫得越快越容易找不到窗口** ⇒ 删 await 走失败路径 ⇒ 绑定成败全凭时序运气；
v2.21 实测「每个新 shell 首次 `cc` 固定烧满超时」。

**照这张图重新实现一遍 PS 侧，会精确复刻那个故障。** 这比「漏写一个字段」危害大得多 ——
漏写的读者会发现对不上，照着写的读者不会。

顺带发现同一处漂移还在另外两个地方：`bind.rs` 自己的模块头（也是旧顺序，且写 `title == marker`
而实现是 `title.contains(marker)`）、`src/settings/cc_integration.ts:291` 给用户看的
「握手超时（800ms）」。三处一并改了。

### 因此多做了一件计划外的事：**握手护栏**（`profile_installer.rs` 的 `mod handshake_doc_guard`）

计划里只有「wire 字段 / 子命令」两条对拍。但顺序那条漂移**光靠字段对拍抓不到** ——
它是两个文件之间的时序约束，两边单看都合理，只有合起来看才错。所以补了三条：

1. `ps_template_sets_the_window_title_before_writing_the_await_file` —— 钉顺序（比较两行的字节位置）
2. `handshake_timings_in_the_template_appear_in_the_protocol_doc` —— 模板里的 deadline / 轮询步长必须在文档里出现
3. `monitor_side_timings_appear_in_the_protocol_doc` —— debouncer 窗口、重试 `12 × 50ms` = `600ms` 必须在文档里出现

放 `profile_installer.rs` 而不是 `bind.rs`：`bind.rs` 几乎整个 `#[cfg(windows)]`，
护栏放那儿**在 Linux CI 上一条都不会跑**（实测 `cargo test bind::` 是 0 个）。
`profile_installer.rs` 用 `include_str!` 内嵌 `cc.ps1.tpl`、且测试在 Linux 上真的跑（666 个里有它）。

### 抽取器写坏了两次，两次都被自检抓住

`wire_field_names()` 被写坏过两次，**两次都抽到 0 个字段**：

1. 逐行维护「当前是否可序列化」的开关 —— 被 doc 注释与 `#[serde(...)]` 属性行搅乱。
2. 改成按区间取，但假定 `#[derive(...)]` 的下一行就是类型声明 —— `wire.rs` 里第一个 derive
   后面跟的是 `#[serde(rename_all = "snake_case")]`，区间当场收在真正的声明行上。

两次都是 `fields.len() >= 15` 这条自检响的，**没有一次悄悄变绿**。这条自检是这个夹具唯一能信的理由。

还踩到一次 `readonly_guard`：我用 `"\n}"` 做区间收尾，字符串里一个**没配对的右大括号**
把 `strip_cfg_test` 的括号配平提前收尾，整个 `protocol_doc_guard.rs` 的 `#[cfg(test)]` 段漏进生产段。
`no_test_code_leaks_into_any_production_section` 抓到了，按它自己的提示改措辞、没动剥法（§41.4 第 1 条纪律）。

### 变异验证里出过一次假红和一次假绿

- **假红**：给 `Frame` 加一个字段测「加字段忘了写文档 ⇒ 红」，rc=101 —— 但那是
  `error[E0063] missing field in initializer` 的**编译失败**，护栏根本没跑。换成「全 crate 改名」
  这种编得过的变异才是真验证。
- **假绿**：`cp -a` 还原后测试仍报变异后的字段名 —— `cp -a` 保留 mtime，cargo 认为文件没变、
  复用了带变异的旧产物（`include_str!` 是编译期烘进去的）。**还原后必须 `touch`。**

最终变异账（全部确认失败信息是护栏断言、不是编译错误）：文档抹掉 `observation` ⇒ 红 ·
wire.rs 改名 `codex_dir` ⇒ 红 · 文档抹掉 `--fork-session` ⇒ 红 · 换回 v1 顺序 ⇒ 红 ·
deadline 3000→2500 ⇒ 红 · debouncer 50→60 ⇒ 红 · 重试 12→10 ⇒ 红 · 还原 ⇒ 全绿。

### 零行 daemon 生产代码改动

`remote-daemon-proto` 的 diff 只有一行 `mod protocol_doc_guard;`，而该模块整个在 `#![cfg(test)]` 内。
「线上行为逐字不变」这次是**结构性成立**（release 构建里它不存在），不需要靠字节对拍来论证。

## 代码审计结果（D）

两路并行：**① 逐条核实文档新说法 + 反向扫查**（「文档里有、代码里没有」那一类，机检**刻意不查**这个方向）
**② 设法绕过五条新护栏**。

### ①-阻塞

| # | 位置 | 问题 | 处置 |
|---|---|---|---|
| B1 | `wire.rs:80-81` | 注释说「『旧 daemon 缺字段 → client 得 0』的向后兼容实现在 **cc-monitor 反序列化侧**」—— **假的**。`ssh_source.rs` 的 `"line"` 分支只取 `session_id/path/seq/raw`，全仓 `byte_offset`/`byteOffset` 零命中。monitor 根本不反序列化它。而我写进 §10 的语义段正是引这条注释背书 | 待修（两处） |
| B2 | `doc/IPC-PROTOCOL.md:443` | `__ccm_rbind` **全仓没有定义** —— 文档把它写成注册流程入口与对外契约。实现早已搬进 `shared/ccm`，`sftp.rs:1059` 还有测试**明令**别名块不得含它。照文档写的外部集成方会调一个不存在的函数：bash 下只打一行 command-not-found、**rc=0 继续跑** ⇒ 静默不注册、↗ 永远「未绑定窗口」 | 待修 |

**B2 正是 `classification` 那一类的第二例**，而且更糟：那个至少名字像；这个是整个函数不存在。
仓内先前审计已记过（`STATUS.md:88`、`PHASE-A-摸底-D-文档改写清单.md:67`），**至今未改** ——
说明「记在待办里」对这类漂移不起作用，只有机检起作用。

### ①-重要

| # | 问题 | 要害 |
|---|---|---|
| I1 | `session_added` 帧字段行漏 `attachable` | **新护栏放行了** —— 因为 `DOC.contains("attachable")` 在 §9.3 命中。这正是护栏头注里自己承认的「只查出现」那条弱点被真实命中 |
| I2 | `--read-session-from-offset` 全文档零出现，**且护栏结构上扫不到** | `dispatched_subcommands()` 只 `include_str!("main.rs")`，而这条（连同 `--list-projects` / `--list-sessions` / `--read-session-tail`）分派在 `observe/history_query.rs`，`--include-tools` / `--scope` / `--after-ms` / `--limit` 在 `observe/search_query.rs`。**护栏对 main.rs 之外的分派表是盲区** |
| I3 | 这轮新写进文档的 6 个字段，**cc-monitor 一个都不消费** | 文档「缺 = claude」「缺 = authoritative」读起来像 monitor 实现了默认值；实际是**字段被整个丢掉**，缺省值碰巧等于丢弃行为。真发 `agent_kind:"codex"` monitor 一样当 claude。唯一真消费者是仓外 aterm |
| I4 | §11 扫描绑定重试节奏：文档 `+1500/3000/4500/6000ms`（4 次 ≤6s） | 实际 `lib.rs:604` 是 `0..15 × 600ms ≈ 9s` |
| I5 | §11 把远端实现指到 `remote-section.ts::CCM_WRAPPER_SNIPPET` | 那其实是 `shared/ccm-aliases.sh`，**29 行只有 3 个别名**，无任何 rbind 逻辑；真实现在 `shared/ccm:626` |
| I6 | §11 步骤 2 `set-titles-string "#T"` | 已改成由 tmux 从 `@ccm_sid` 合成 marker；`#T` 只是回退。**照文档写会退回一个已修的 bug**（claude 抢写 pane 标题冲掉 marker，实测 ↗ 约 1/5 命中） |

I2 是最要紧的一条：它说明护栏的**抽取面**画小了。修法不是补一条 `--read-session-from-offset`，
而是让 `dispatched_subcommands()` 覆盖所有真正做分派的文件。

### ①-建议（已收）

- §11 步骤 3 漏了 `~20s 自愈重打` 与 `tmux set-option @ccm_sid`；步骤 1 的 `$BASHPID` 应为 `$$`
- §9 说 pidfile 的 `name`「当前保留未用」已过时（`session_map.rs:237` 在用）
- 三处源码路径引用随 `observe/` 重构失效（doc:311/405/407）
- 我新写的 `--account-trust-zero` bullet **自相矛盾**：签名写 `<cwd>` 又说「不收任何路径参数」。
  实情是收 `cwd`、但 `.claude.json` 的根写死 `$HOME`，`cwd` 只当查表键 ⇒ 措辞应为「不收任何**文件/配置目录**路径参数」
- doc:414「所有路径参数严格限制在 `<claude_dir>/projects/` 内」在新增子命令后已不成立
- hello 字段的**文档列举顺序 ≠ 线上字节序**（线上 `claude_dir, codex_dir, kinds, capabilities, emits`），
  而对面拿它对 fixture ⇒ 加一句「顺序以 `wire.rs` 声明序为准」
- doc:89 「超时必报警」比代码宽：存在指纹不匹配的陈旧 registry 时超时**不告警**

### ①-核过确认正确的（关键几条）

7 个字段的**挂载帧、skip 语义**逐条对上（由 `dg3_codex_fields_serialize_when_present` 等精确字节串钉住）；
`byte_offset` 的「计 `\r`、含 `\n`、残行不计、resume N ⇒ tail -c +(N+1)」**逐条成立**并有对拍测试；
`emits`/`capabilities` 正交成立；`observation` 三取值端到端对上，**`classification` → `observation` 这次改对了**；
**握手时序图逐步全部对得上**（含 tpl:63-64 先设标题、tpl:69-72 后写文件、debouncer 50ms、
重试 `0..12 × 50ms`、轮询 30ms/deadline 3000ms 二选一退出、循环外补查 registry）。

## 工程审计结果（E）

### ②-护栏强度审计顺带暴露的一件事：审计 agent 自己留下了变异

那个「设法绕过护栏」的 agent 把 `cc.ps1.tpl` 的两行顺序换回 v1 之后**没有还原**。
是我新写的 `ps_template_sets_the_window_title_before_writing_the_await_file` 报红抓到的
（`git checkout` 还原，`guard_support.rs` / `wire.rs` / `main.rs` 逐一核过无残留）。

两条教训：
1. **并发 agent 与主线程改同一批文件会互相覆盖。** 本轮我有两处编辑被它的 `cp -a` 快照回滚
   （第四条握手护栏 + `ARCH_DOC` 常量），事后重做。以后派「会改文件」的审计 agent 时，
   主线程要么停手等它，要么给它 worktree 隔离。
2. **护栏当场兑现了价值** —— 它抓的第一个真实回归，来自审计流程本身而不是业务改动。

### 主计划自洽性

- **账本 S6**（wire 协议 + `IPC-PROTOCOL.md`）的「文档先修再冻结」这一半 **U6a 交付完毕**；
  「双向」那一半留给 U6b。S6 原文写「该文件 7 处在说谎」—— 实测远不止 7 处，已在变更记录订正。
- **新登记 S6a**（跨进程握手时序约束，四处双写）—— U6a 交付其最终形态（4 条护栏钉住）。
- **对 U6b 是净利好**：`every_dispatched_subcommand_...` + `dispatch_registry_is_complete` 会**强制**
  U6b 新增的每个子命令/字段同步进文档。U6b 加 request-id / 取消 / 背压时，护栏自动生效，不用另立纪律。

### 给后续功能的三条移交

| 收件人 | 内容 |
|---|---|
| **U7（读面合流）** | D 审计实测：`agent_kind` / `liveness_confidence` / `byte_offset` / `codex_dir` / `kinds` / `emits` **cc-monitor 一个都不消费**，`parse_frame` 直接丢弃。U7 要把读面搬到 daemon，这六个字段就是「daemon 已产出、monitor 尚未接」的现成缺口清单 —— 别当成新工作，是**已存在的未接线**。（daemon 侧 DG1 也还没接，今天硬写 None/空。） |
| **U6b** | `--resolve` 吸收、argv 三分要动分派面 ⇒ 必然触发 `dispatch_registry_is_complete`。这是设计意图，不是障碍。 |
| **U13（文档改写 + 重命名）** | `__ccm_rbind` 这类「文档里有、代码里没有」的反向漂移，**机检刻意不查**（U6a 已如实登记这条盲区）。U13 要做一次全仓反向扫查 —— 本轮 D 审计用人工做了 §9/§10/§11，其余章节没覆盖。 |

### 一个没在本轮解决的判据弱点（如实登记）

`attachable` 这条（I1）暴露了「只查出现」的真实代价：它**本来就在文档里**（§9.3 有整节），
只是没进 §10 的帧字段行，于是护栏放行。字段级的「必须在**对应帧的字段行**里」需要解析 markdown 表格
+ 建立「字段 ↔ 帧」的映射，成本明显高于本轮收益，**登记不做**。
子命令那一侧已经收紧到「必须在两张表之一」，是因为那边的表结构简单、判据成本低。

## 签收

- [x] 过代码审计（D）—— 2 阻塞 + 6 重要 + 7 建议**全部修完**；护栏两处判据据审计收紧
- [x] 过工程审计（E）—— 账本 S6 半交付 / S6a 新登记；三条移交已写给 U7 / U6b / U13
- [x] 主计划已更新（F）—— MASTERPLAN 变更记录 3 条 + 账本 S6a；STATUS 进度表与门禁基线已刷新

### 门禁终态（本轮实测，均 RC=0）

daemon `cargo test` **203**（HEAD 201，+2 为对拍护栏；`dispatch_registry_is_complete` 是第 3 条，
与另两条同批） · monitor `--lib` **667 / 3 ignored**（+4 握手护栏） · `npm test` **80 文件 1154 例** ·
`tsc` 0 · 两侧 `cargo fmt --check` 干净 · clippy 与 HEAD 逐条相同（零新增）。

### 变异账（全部确认失败信息是护栏断言，不是编译错误）

文档抹掉 `observation` ⇒ 红 · `wire.rs` 全 crate 改名 `codex_dir` ⇒ 红 · 文档抹掉 `--fork-session` ⇒ 红 ·
把 `--fork-session` 从查询表挪到散文（全文仍有）⇒ 红 · 文档抹掉 `--read-session-from-offset` ⇒ 红 ·
PS 模板换回 v1 顺序 ⇒ 红 · deadline 3000→2500 ⇒ 红 · debouncer 50→60 ⇒ 红 · 重试 12→10 ⇒ 红 ·
逐一还原（`cp -a` 后 `touch`）⇒ 全绿。

**一条作废的变异**：给 `Frame` 加字段测「加字段忘写文档 ⇒ 红」，rc=101 但那是
`error[E0063]` 编译失败、护栏根本没跑 —— 换成「全 crate 改名」这种编得过的变异才算数。

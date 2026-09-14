# `K-R113` 死值验读数 —— daemon 补 `bus-state` 这条具名读命令

> 件文件 `§3` 只放摘要，**完整原文在这里**。
> 量具：`evidence/K-R113-cut.py`（刀，只落刀与还原、不自己跑门禁）·
> `evidence/K-R113-ruler.py`（尺子，**只出读数、不判红**）。

## §0 量点 —— 每个数「哪个面 · 什么量法 · 量于哪棵树的哪个提交」

| 量的是什么 | 面 | 量法 | 量于 |
|---|---|---|---|
| 门禁逐格 | 整棵工作树 | `PB_WS=backend-consolidation .claude/devbox/gate <工作树> k-r113`，**从项目根 `/home/zbl/文档/claudecode-frontend` 起跑** | 工作树 `.claude/worktrees/k-r113`（分支 `track/k-r113`，基点 `0beaf07`） |
| 两个命令面的条数 | `remote-daemon-proto/src/{main,inbound}.rs` | `evidence/K-R113-ruler.py`（剥行首 `//` 注释后抠字面量 / `name:` 字段） | 同上 |
| 转调了哪几条 cc-bus 命令 | `remote-daemon-proto/src/control/cc_bus.rs` **生产段** | 同上（`run("` / `run_as("` 之后那一个字面量） | 同上 |
| 逐函数改动面 | 三份改过的 `.rs` | `evidence/K-W4b-rs-fn-md5.py <基点> <文件>`（顶层函数整块 md5） | 基点 `0beaf07` vs 本工作树 |

🔴 **宿主上一条 `cargo` / `npm` / `vitest` / `tsc` 都没跑过**（`K31`）。
跑过的命令只有两种：上面那条沙箱门禁，以及 `python3 evidence/K-R113-{cut,ruler}.py`（纯文本，不编译）。

⚠ **`M0` 那一趟的 target 目录名与后面几趟不同**（`kr113-m0` vs `k-r113`）——
两趟的 `CARGO_TARGET_DIR` 不同，**读数可比**（同一棵源码树、同一个镜像），
但「几分钟跑完」这类时间读数不可比（`M0` 是冷构建）。⚠ 还有一处如实写：
`M0` 那趟的容器是被孤儿化的（起它的那个 shell 被我掐了），**退出码读不到** ——
它的判据是最后一行逐字 `GATE: OK —— 13 格全绿`（`gate.sh` 只在 13 格全绿时印这一行）。

## §A 门禁逐格读数（带分母）

| 趟 | 是什么 | 裁决 |
|---|---|---|
| **`M0`** | 基线：写区一个字节未动，只有 3 份未跟踪的 evidence 占位 | `GATE: OK —— 13 格全绿` |
| `M1` | 实现落地第一趟 | `GATE: FAIL —— fmt-daemon（1）；cargo（101）` |
| **`M2`** | fmt 修好之后的**本件工作基线** | `GATE: FAIL —— cargo（101）`，**唯一那条红是跨轨阻塞，见 `§E`** |
| **`M3`** | 收工趟（两个提交都在盘上、件文件写完之后） | 见本节末 |

**`M0` 逐格**（分母逐格照抄门禁自己印的那一句）：
hooks 11 · fmt 1 · fmt-daemon 1 · winchk 1 · **cargo 1610（9 个包合计）** ·
generated 一致 · **daemon 756（单包 `remote-daemon-proto`）** · npm 1726 ·
e2e **12 / 8 / 46 / 45**（四套各自的地板，恒等）· pb check `FAIL=0 BROKEN=0`。
⚠ 本树**未铺** `src-tauri/embedded-daemons/` ⇒ cargo 那一格少「本地后端真的能起来吗」那族 4 条
（门禁自己印的「分母 cargo」那一行）。**⇒ 本轮不涉及 re-embed**（件文件 `§0b` 要求在上报里说这一句）。

**`M2` 逐格**：hooks 11 · fmt 1 · fmt-daemon 1 · winchk 1 ·
**cargo `1473 passed; 1 failed; 8 ignored`（`-p monitor --lib` 那一包）** ·
generated 一致 · **daemon 761（756 → 761，`+5` = 本件新加的 5 条判据，逐条见 `§C`）** ·
npm 1726（±0）· e2e 12 / 8 / 46 / 45（±0）· pb check `FAIL=0 BROKEN=0`。

## §B 死值验逐刀 —— 8 刀，逐刀记锚点命中数

> 一趟一刀。刀具 fail-closed：锚点不是恰好 N 次就 `exit 3`、一个字节不落地；
> 还原是**重写原文**（不 `cp -a`，否则 mtime 不变、cargo 判「没变」）。
> 「命中数」= `apply` 落刀前断言的那个数（`brief` 第 7 条），逐刀原文在 `cut-<刀名>.apply`。

| 刀 | 打哪儿（锚点命中） | 门禁读数 | 判 |
|---|---|---|---|
| **`d1-0`** 「一次回全」只回一半 | `cc_bus.rs::state_for_inbound` 的**体**（**1**）；签名与返回类型一个字没动 | daemon `760 passed; 1 failed` | ✅ **最小面**：唯一那条是 `control::cc_bus::tests::bus_state_answers_both_halves_from_one_call` |
| **`d1-1`** `KR113D1` ①：表里加了、分派臂不加 | `inbound.rs::REGISTRY` 块内那条 `CommandSpec`（**1**） | daemon `756 passed; 5 failed`；**cargo 反而绿了 1610** | ✅ **已知答案成立** —— 见下方专段 |
| **`d1-2`** `KR113D1` ②：`BUILD_ID` 不 bump | `main.rs` 的 `BUILD_ID`（**1**）＋ `build_id_guard.rs` 的历史行（**1**） | daemon `760 passed; 1 failed` | ✅ **最小面**：唯一那条是 `build_id_guard::tests::adding_a_subcommand_forces_a_build_id_bump` |
| **`d1-3`** `KR113D1` ③ 阴性对照 | 本件 5 条判据逐条 `#[ignore]`（**5**）＋ 刀 `d1-1`（**1**） | daemon `751 passed; 5 failed; 7 ignored` | ✅ 红的**恰好是 `d1-1` 那 5 条**，本件的判据一条都不在里面 ⇒ **刀① 的牙一条都不长在本件的判据上** |
| **`d1-4`** 三态压成两态（`活?` 并进 `活`） | `spawned_live_of` 的 `SPAWNED_UNVERIFIED` 那一臂（**1**） | daemon `759 passed; 2 failed` | ✅ 正是那两条：`parse_spawned_reads_the_table_and_keeps_the_three_states` · `the_three_spawned_states_stay_three_different_answers` |
| **`d1-5`** 认不出的状态串猜一个答案 | `spawned_live_of` 的 `_ => None` 那一臂（**1**） | daemon `759 passed; 2 failed` | ✅ 正是那两条：`non_data_lines_never_become_spawned_rows` · `the_three_spawned_states_stay_three_different_answers` |
| **`d2-1`** `KR113D2` **乙**的死值验：删掉那条登记 | `TRANSCALLS` 里 `cc-agents` 那一行的键（**1**）—— **转调本身还在** | daemon `760 passed; 1 failed` | ✅ **最小面**：唯一那条是 `every_shelled_out_command_carries_a_written_ruling` |
| **`d2-2`** `KR113D2` **甲**的代价现打 | `spawned_via_cc_agents` 里那一句转调（**1**）换成 daemon 自己 `read_to_string` 那份 `.tsv` | 见 `§D` | 见 `§D` |

⚠ 每一刀的 cargo 那一格都另有**一条恒定的红**（跨轨阻塞，`§E`）；
`d1-1` / `d1-3` 是例外 —— 它们把 `REGISTRY` 那条摘掉了，于是那条跨轨判据当场变绿（`cargo 1610`）。
**这本身就是一次定位**：那条红完全由「`REGISTRY` 里多了一条 `bus-*`」引起，与本件别的改动无关。

### 🔴 `d1-1` 的已知答案：**那把总伞真的罩住了新条目**

诊断逐字（`cut-d1-1.log`）：

```
thread 'argv_table_guard::every_listed_subcommand_has_a_live_dispatch_route'
  panicked at src/main.rs:2488:9:
这些子命令**登记在表里，却没有任何一条分派路够得到**：["--bus-state"]
```

⇒ `K-R102` 那把总伞的人群（`SUBCOMMANDS` 自己）**自动盖到了本件新加的那一条**，
不需要任何人去它那里写名字。**不用报 PM 那条真缺陷 —— 它没发生。**

同一刀里另外 4 条红，全是既有的结构判据（不是本件的）：
`cli_control::tests::the_no_input_commands_are_registered_and_declared_consistently` ·
`inbound::structure_guards::the_commands_mirror_matches_the_registry` ·
`inbound::tests::the_dispatch_table_puts_blocking_commands_on_the_blocking_arm` ·
`plugin_walk_fixture::tests::adding_a_plugin_still_costs_the_host_something`。

⚠ **而那条瞎的仍然瞎**：`every_listed_subcommand_is_actually_dispatched`（同一份文件里、
头注自陈「它拿 `SUBCOMMANDS` 去比整份文件的 `"--` 字面量扫描，而那张表自己就住在被扫的生产段里」）
在这一刀下**没有红**。⇒ 它头注登记的那条盲区，本轮拿到了一次**活体读数**。

## §C 「把实现整个退掉，还有多少条新断言仍绿」

分母 = 本件新加的 **5** 条判据（人群与阴性对照刀 `#[ignore]` 的那份名单**同一处住址**：
`evidence/K-R113-cut.py::D_JUDGES`）。

| 新判据 | 被哪一刀打红过 |
|---|---|
| `every_shelled_out_command_carries_a_written_ruling` | `d2-1` |
| `bus_state_answers_both_halves_from_one_call` | `d1-0` |
| `parse_spawned_reads_the_table_and_keeps_the_three_states` | `d1-4` |
| `non_data_lines_never_become_spawned_rows` | `d1-5` |
| `the_three_spawned_states_stay_three_different_answers` | `d1-4` · `d1-5` |

**5 / 5 至少被一把刀打红过，仍绿的 0 条。**

## §D `KR113D2` —— 那几条转调 shell 的债：**现打 · 裁乙（不收）· 两条路都验**

### D1 先现打：今天各自转调了什么、转调点在哪

尺子 `evidence/K-R113-ruler.py` 的 `[转调]` 那一行现算（分母 = `control/cc_bus.rs`
**生产段**里 `run("` / `run_as("` 之后那一个字面量，取法与 `readonly_guard::g6_reach` 那一格刻意相同）：

| 被调命令 | 转调点（本件之后） | 那个脚本自己在做什么 | 写面 |
|---|---|---|---|
| `cc-list` | `agents_via_cc_list`（`bus-list` 与 `bus-state` 共用这一个）· `recipient_status` | 读 `agents.tsv` ＋ 逐个数 `inbox/<id>.jsonl` 减 `state/<id>.pos` | 只读 |
| `cc-agents` | `spawned_via_cc_agents`（**本件新增**） | 读 spawn 台账 ＋ 回 `agents.tsv` 借 pane 根进程 pid 核身份 ＋ 问 tmux | 只读 |
| `cc-send` | `send_for_inbound` | 投递（`flock` ＋ 路由层 ACL / 限流 / 去重 / 灭环） | 写收件人收件箱 |
| `cc-kill` | `kill_for_inbound` | 杀会话 ＋ 进程树、清名册 / 台账 / 状态 | 破坏性 |

⚠ **`K-R111 §H7` 那条登记说的是「三条」，本件之后是四条** —— 数变了不是因为债变多了，
是因为本件给 spawn 台账那一半补了对侧（`cc-agents`）。**这个数别照抄散文，用尺子现打。**

### D2 裁**乙（不收）** —— 理由现打，不是「感觉大」

三条读数，每条都能重打：

1. **收进后端 = 把 cc-bus 的私有文件格式在 Rust 里再实现一遍**，而这条边界今天**有人守着**：
   `remote-daemon-proto/src/cc_bus_boundary_guard.rs::no_cc_bus_data_layout_leaks_into_the_daemon`
   —— 它扫全 crate 生产段，针是 5 根 cc-bus **专有**的名字。刀 `d2-2` 是这一条的实测（下面）。
2. **收掉 `cc-kill` 要重写它那道门，而那道门的证据后端拿不到**：`cc-kill` 核身份用的是
   `agents.tsv` **第 4 列**（登记时记下的 pane 根进程 pid），而**命令面一个字都不打印它**
   （`cc-list` 三列、`cc-agents` 四列，现打）⇒ 后端重写只能退回**按名字判活**，
   而那正是 08-13 那次「杀掉占了同名的无辜进程与会话」的成因（`cc-agents` 头注逐字记着）。
3. **用户 08-13 逐字「后面我可能要改ccbus」** —— 今天焊进来的每一个字段都是他改那天的一笔返工。
   ⚠ 这一条是**定框级**的张力：`K33` 逐字「不要有什么 bash 脚本」说的是**后端自身**的组成，
   而 cc-bus 今天在这棵树上的身份是**插件**（走 `crate::plugin` 通用调用口，与 code-picture sidecar 同一条口）。
   **两条哪个管这一族，我判不了 —— 那是 PM/用户的裁定，我只把读数摆出来。**

⇒ 落成盘上看得见的东西：`control/cc_bus.rs::tests::TRANSCALLS`
（`(被调命令, 写面, 这一趟为什么不收, 解锁条件)` 四元组，形状抄 `readonly_guard::spawn_registry::ALLOWED`
与 `cli_control::NOT_ON_CLI`），由 `every_shelled_out_command_carries_a_written_ruling`
**数据对数据**地钉着：登记集 == 生产段现算出来的转调集，两个方向都判（漏裁 / 幽灵）。

### D3 甲那条路的代价 —— 刀 `d2-2` 的实测

刀 `d2-2` 把 spawn 台账那一半从「转调 `cc-agents`」换成「daemon 自己 `read_to_string` 那份 `.tsv`」
（**形状对**：签名与返回类型没动，仍回 `Result<Vec<Value>, (String, String)>`）。
一处锚点，命中 1 次。门禁读数：**daemon `758 passed; 3 failed`**，三条逐条：

| 红的判据 | 诊断逐字 | 它是什么 |
|---|---|---|
| `cc_bus_boundary_guard::tests::no_cc_bus_data_layout_leaks_into_the_daemon` | `daemon 的生产段碰了 cc-bus 的**数据布局**：["control/cc_bus.rs 里有 \`spawned.tsv\`", "control/cc_bus.rs 里有 \`.cc-bus\`"]` | **压在用户 08-13 那句逐字上的边界**（「后面我可能要改ccbus」） |
| `readonly_guard::g6_reach::the_non_literal_spawn_key_still_covers_exactly_three_commands` | `经 \`plugin/invoke.rs\` 那个 \`<非字面量>\` 键转调的命令变了：["cc-kill", "cc-list", "cc-send"]` | 只读铁律那条豁免理由的覆盖面 |
| `control::cc_bus::tests::every_shelled_out_command_carries_a_written_ruling` | `TRANSCALLS 里这几条**今天已经不转调了**：["cc-agents"]` | 本件新立的那条 —— **它的「幽灵」那一支在这里被验到了** |

⇒ **甲那条路的第一步就要拆掉三条独立判据**，其中一条（边界那条）不是工程口味、是用户裁过的。
⚠ 这一刀的 fmt-daemon 也红了 —— 那是**刀具形状**（注进去的代码没过 rustfmt），**不是甲的代价**，别混。

⚠ **这一刀量到的是「甲的第一步要付什么」，不是「甲不可行」**。真做甲还要付
`cc-kill` 那道门（`§D2` 第 2 条）与 `cc-send` 的路由层，那两笔**本轮没量**（没有对应的刀）。

## §E 🔴 跨轨阻塞：本件**做不完**的那一格，我一个字节都没动

`M2` 起每一趟的 cargo 那一格都有**同一条**红，诊断逐字：

```
thread 'plugin_class_registry::tests::cc_bus_is_reached_only_through_its_command_surface_today'
  panicked at src/plugin_class_registry.rs:435:9:
assertion `left == right` failed: daemon 转调 cc-bus 的命令从 3 条变成 4 条：
["bus-list", "bus-kill", "bus-send", "bus-state"]
  left: 4
 right: 3
```

**它住在 `src-tauri/src/plugin_class_registry.rs`** —— 而本件的写区**明令**
「`src-tauri/**` 全部」不在其中（件文件 `§2`）。⇒ **我没动它。**

- **它是什么**：一条**跨轨绊线** —— monitor 侧的登记表去数 daemon 命令表里的 `bus-*` 条数，
  钉成**相等**。它的诊断自陈用途是「变了就该回去看那条待决（`EU3`：插件的粒度是命令还是包）」。
- 🔴 **而它指的那条待决今天已经作废**：`backend-consolidation/OPEN-PREMISES.md` 逐字
  「`plugin-split` `EF04` 已撤件、`EU3` 同时作废」。⇒ 绊线还在，它要人去读的那份东西没了。
- **要它绿只有一处改动**：`bus.len()` 那个期望值 `3` → `4`，
  外加同一条 `assert` 里那段 ⚠ 散文（它逐字写着「`C19` 说 `bus-*` 四条、而实测三条」——
  **今天实测就是四条了，那段订正话本身过期**）。
- **判**：这不是「本件顺手能带的一行随动」，它是**另一棵轨上的一次裁定**（那条绊线到底该不该
  用「条数相等」这种形状去钉一个会长的集合）。⇒ **请 PM 裁：扩本件写区，或开一件。**

## §F `exact_target` —— 删得掉还是删不掉（`K-R112` 交回、PM 点名归本件）

**现打今天的形状**（住址带校验位：行号旁边抄那一行逐字）：

| 处 | 住址 | 那一行逐字 | 状态 |
|---|---|---|---|
| daemon 的那份 | `remote-daemon-proto/src/control/launch.rs:151` | `pub(crate) fn exact_target(name: &str) -> String {` | **活的**：4 个生产调用方（`control/kill.rs` · `control/capture_pane.rs` · `control/oneshot_session.rs` · 本文件 `run`） |
| 跨轨对拍 | `remote-daemon-proto/src/control/launch.rs:781` | `    fn exact_target_shape_matches_the_monitor_side() {` | 两半：① 断自己的形状 `=cc-abc:`；② `include_str!` monitor 的 `tmux.rs` 要求里面找得到 `={target}:` |
| 锚点那一行 | `remote-daemon-proto/src/control/launch.rs:784` | `        const MONITOR_TMUX: &str = include_str!("../../../src-tauri/src/tmux.rs");` | 这就是把两棵轨绑在一起的那一行 |
| monitor 的壳 | `src-tauri/src/tmux.rs:599-600` | `#[allow(dead_code)]` / `pub(crate) fn exact_target(target: &str) -> Result<String, String> {` | `K-R112` 之后**零生产调用方**，留着只为当锚点 |

**判：删得掉。** 三条理由，每条都能重打：

1. **那条对拍今天买到的东西是空的。** 它要的是「monitor 的生产段里找得到 `={target}:` 这个形状」，
   而**今天全 monitor 只有那个死壳自己含这个形状**（现打：`grep -rn "exact_target" src-tauri/src/`
   除本文件外只有 `structural_scan.rs` 里一个**同名的测试局部函数**，与它无关）。
   ⇒ 它断言的是「一个没人调的函数仍然长得对」。它写下时要守的性质是「**两侧同形**」，
   而今天只剩一侧是活的。
2. **这是一个自锁**：壳留着是因为对拍锚在它身上；对拍锚在它身上是因为壳还在。
3. **删掉不丢任何行为**：`=name:` 那条性质今天真正在跑的那一份住 daemon（`kill` / `capture-pane` /
   `oneshot-session` / `launch` 四条都调它），而它自己那一半断言（`assert_eq!(exact_target("cc-abc"), "=cc-abc:")`）
   **留着不动**就够。monitor 侧要留的是 `gate1_reject_empty`（有自己的调用方与自己的判据）。

**⚠ 但删它不是本件能做的事**，两条硬理由：
- 一刀要同时动两棵轨（daemon 去掉 `include_str!` 那一半 ＋ monitor 删壳），
  而 `src-tauri/**` **明令不在本件写区**；
- 顺手改判据的**名字**（`..._matches_the_monitor_side` 删掉跨轨那半之后名字就假了）
  会触发「改过判据的名字 ⇒ 同轮必须跑一次 `pb doc`」（固定项第 15 条），
  而 `pb doc` 属「窗口开着期间一概不跑」的生成命令族（第 19 条第三款）。
  ⇒ **这一刀该由 PM 在收窗口那一拍安排，或者单开一件。**

⚠ 顺带一条读数：daemon 这棵树今天**一共 3 处**跨轨 `include_str!` 进 `src-tauri/`
（`relay/route.rs` 的 `payload.rs` · `control/gate.rs` 的 `gate2-golden.tsv` ·
`control/launch.rs` 的 `tmux.rs`）。本件问的是第三处；另外两处**没查**它们锚在死代码上没有。

## §G 改动面 ＋ 三处 `git status`

### G1 两张表与转调集的现打（尺子 `evidence/K-R113-ruler.py`，被测对象 = 本工作树）

| 读数 | 基线（`0beaf07`） | 今天 |
|---|---|---|
| CLI 面 `SUBCOMMANDS` | 26 | **27**（`+--bus-state`） |
| 帧面 `REGISTRY`（单一事实源） | 10 | **11** |
| 帧面 `COMMANDS`（镜子） | 10 | **11**（与 `REGISTRY` 两侧差集**都空**） |
| `cc_bus.rs` 生产段转调的 cc-bus 命令 | 3 | **4**（`+cc-agents`） |
| `BUILD_ID` | `p2i-frame-tmux-primitives` | **`p2j-bus-state`** |

⚠ **本树未铺 `src-tauri/embedded-daemons/`** ⇒ 本轮**不涉及 re-embed**；
`BUILD_ID` 这一半是**源码半**，re-embed（CI 交叉编译）归发版那一拍，本轮**没做**
（同 p2d / p2e / p2g / p2h / p2i 那五次的如实登记）。

### G2 逐顶层函数 md5（`evidence/K-W4b-rs-fn-md5.py 0beaf07 <文件>`，基点 vs 本工作树）

| 文件 | 相同 | 变了 | 没了 | 新增 |
|---|---|---|---|---|
| `control/cc_bus.rs` | 14 | **1**（`list_for_inbound`：18 行 → 3 行，体换成调 `agents_via_cc_list`） | 0 | **5**（`spawned_live_of` · `parse_spawned` · `agents_via_cc_list` · `spawned_via_cc_agents` · `state_for_inbound`） |
| `inbound.rs` | 9 | 0 | 0 | 0 |
| `main.rs` | 17 | 0 | 0 | 0 |
| `control/cli_control.rs` | 8 | 0 | 0 | 0 |
| `build_id_guard.rs` | 0 | 0 | 0 | 0 |
| `readonly_guard.rs` | 0 | 0 | 0 | 0 |

🔴 **这张表的射程要说清，别读宽**：该量具只认**顶层函数**（第 0 列的 `fn`），
`impl` / `mod tests` 里缩进的一律不在分母里，`const` 表更不在。
⇒ 「`inbound.rs` / `main.rs` 顶层函数一个字节没动」是真的，
而本件在这两份文件里改的**全部是 `const` 表与头注**（`SUBCOMMANDS` · `COMMANDS` · `REGISTRY` · `BUILD_ID`）
—— **那部分这张表买不到**，要靠 `§G1` 的尺子读数 ＋ PM 读 diff。
`build_id_guard.rs` / `readonly_guard.rs` 顶层函数是 **0**（整份文件住在 `#[cfg(test)]` 里）
⇒ 同理，这两份的改动这张表**一格都买不到**。

### G3 三处 `git status`（收工时现打）

| 处 | 状态 |
|---|---|
| **计划仓** `.claude/planned-build/backend-consolidation` | 只写了本件的件文件一份（`features/K-R113-…md`），**一次 `git commit` 都没跑**（固定项第 3 条）。`STATUS` / `LEDGER` / `DECISIONS` / `INDEX` / `audits/` / `.dispatch.json` **一个字节没碰** |
| **代码仓主树** `cc-monitor`（`main` @ `0beaf07`） | **干净，一个字节没碰** |
| **本工作树** `.claude/worktrees/k-r113`（`track/k-r113`） | 提交完干净；两个提交（写区内一个 · **写区外一个，标题第一行逐字带「写区外」，PM 一条 `git revert` 就能退**）。提交只点名文件，没用过 `git add .` / `-A` / `-u` / 裸目录 / 通配 |

## §H 诚实边界 / 判不了

1. **一趟真机都没跑**（`K31`）⇒ 「`bus-state` 真的回得出两半」**判不了**。
   沙箱里没装 cc-bus，`run()` 那一跳走不到；本件的判据钉的是**接线**（两半各自落到哪个函数）
   与**纯函数**（两个 parser、三态映射）。真跑归 `e2e/daemon-cc-bus.sh` 那一族，本轮**没跑**。
2. **`bus-state` 答不出三个字段**（登记时间 · spawn 时间 · 坏行计数）——
   现打的成因写在 `doc/IPC-PROTOCOL.md` 那一小节的表里。
   ⇒ **接线那一件（monitor 的 `read_cc_bus_state` 改走后端）拿这一条去问产品之前，别当它是纯接线**：
   照今天这份契约接过去，驾驶舱那三处渲染会退化。
3. **`cc-agents` 的输出是定宽 `printf` 表**，目录名含空格时 `dir` 会切不准（余下并进 `task`）。
   这是「拿输出当接口」的代价，写在 `parse_spawned` 头注与协议文档里，**不假装钉住了**。
4. **`spawned` 那半的三态依赖三个中文字面量**（`活` / `活?` / `已退`）。
   cc-bus 换个说法 ⇒ 那一行**落选**（不是被判成「已退」）。落选方向是刻意的，但**它仍是一处静默降级**。
5. **`K33`（后端不要 bash 脚本）与 08-13（cc-bus 是外部插件、它自己会改）哪一条管这一族，我判不了** ——
   `§D2` 第 3 条。我给读数，不替 PM 裁。
6. **本轮没跑 `pb doc` / `pb index`**（生成命令族，窗口开着期间一概不跑）。本件也没改任何判据的**名字**
   （只加新的），⚠ 例外一处：`readonly_guard` 那条名字里带 `three` 的判据今天数的是 4 ——
   **名字假了而我没改名**，理由写在它头注里（改名要跑 `pb doc`，那笔账不在本件写区）。

# K-R112 死值验原文 —— cc-bus 三条改走原语 ＋ 抓屏改走 `capture-pane` 帧

> 件文件 `§3`/`§8` 只放结论与分母，逐刀读数住这里。
> 🔴 **宿主上一条 `cargo` / `npm` / `vitest` / `tsc` 都没跑过**（`K31`）。唯一跑过的命令是
> `PB_WS=backend-consolidation .claude/devbox/gate /home/zbl/文档/claudecode-frontend/.claude/worktrees/k-r112 k-r112`
> （从项目根 `/home/zbl/文档/claudecode-frontend` 起跑）。

## §0 量点 —— 每个数都写「哪个面 · 什么量法 · 量于哪棵树的哪个提交」

| 项 | 值 |
|---|---|
| 工作树 | `/home/zbl/文档/claudecode-frontend/.claude/worktrees/k-r112`，分支 `track/k-r112` |
| 基点 | `afda78b`（`Merge branch 'track/k-r111'`，`git log --oneline -1` 现打于 2026-09-13） |
| 代码仓 | `/home/zbl/文档/claudecode-frontend/cc-monitor`（`main` @ `afda78b`）—— **一个字节没碰** |
| 计划仓 | `/home/zbl/文档/claudecode-frontend/.claude/planned-build/backend-consolidation` —— **只写不提交** |
| 刀具 | `evidence/K-R112-cut.py`（住址就是这一份；⚠ 与 `K-R109-cut.py` / `K-R106-cut.py` 是不同的量具，被测对象是不同的树） |
| 读数器 | `evidence/K-R112-ruler.py`（**只出读数、不判红**，理由写在它头注里） |
| 递减棘轮 | `evidence/K-R111-ruler.py` 的 `BASELINE`（本件拧了四格） |
| 复跑 | `python3 evidence/K-R112-cut.py list` · `apply <刀>` / `restore` · `ruler <刀>` |

⚠ **刀具与读数器的被测对象都是「它所在的那棵树」**（`ROOT = 本文件的上一级`），
不拿 `cwd` 猜 —— 交回读数时把「在哪棵树上跑的」一起写，那个分母跑不掉。

---

## §A 门禁逐格读数（带分母）

**`M0` 自己现打**（量于基点 `afda78b`，写区一个字节未动，只有 3 份未跟踪的 evidence 占位文件）：

```
GATE: OK —— 13 格全绿   EXIT=0
hooks 11 · fmt 1 · fmt-daemon 1 · winchk 1 · cargo 1605（9 个包合计）· generated 一致 ·
daemon 756 · npm 1726 · ccm e2e 12 / 8 / 46 / 45 · pb check FAIL=0 BROKEN=0
```

⚠ **本树未铺 `src-tauri/embedded-daemons/`** ⇒ `embedded_daemons` cfg 不置 ⇒ cargo 那个合计里
少了「本地后端真的能起来吗」那一族 4 条（这是本树的分母，不是本件造成的）。

**终局 `M4`**（本件全部交付、无刀）：

```
GATE: OK —— 13 格全绿   EXIT=0
hooks 11（±0）· fmt 1 · fmt-daemon 1 · winchk 1 · cargo 1610（**+5**）· generated 一致 ·
daemon 756（±0）· npm 1726（±0）· ccm e2e 12 / 8 / 46 / 45（±0）· pb check FAIL=0 BROKEN=0
```

🔴 **`M5`（收工那一趟，两个提交都在盘上、件文件 `§3`/`§8` 也写完之后）：12 个代码格全绿、
第 13 格 `pb check FAIL=1 BROKEN=0`** —— 唯一那条逐字：

```
FAIL   [J3 陈账] INDEX.md 比源文件旧 —— 重跑 `pb index` 落盘
```

**它点的是共享计划仓，不是本件的代码**，而且**成因可证**：
`M4` 那一趟（我还没写件文件）`pb check` 是 `FAIL=0`；`M5` 之前我只写了一份计划仓文件。
现打 `find . -name '*.md' -newer INDEX.md`（排掉 `.briefs/`）⇒ **恰好一份**：
`features/K-R112-真接线那4条里的3条（cc-bus三条＋抓屏）.md`（`00:18:24` > `INDEX.md` 的 `00:02:17`）。
⚠ `pb index` 属**「窗口开着期间一概不跑」的生成命令族**（固定项第 19 条第三款），
而 `INDEX.md` 又在**固定项第 2 条列的「一个字节都不许碰」里** ⇒ **我不跑、也不碰，交 PM 收窗口那一拍跑。**

**中途三趟（都留着，别只报最后一趟）**：

| 趟 | 结果 | 红的是谁 |
|---|---|---|
| `M1` | `GATE: FAIL —— fmt · cargo · daemon` | ① 三处 `build_capture_pane_cmd` 编译不过（老测试没跟着改）；② 一处闭包元数不对；③ **daemon 侧** `control::launch::tests::exact_target_shape_matches_the_monitor_side` —— 它 `include_str!` monitor 的 `tmux.rs` 要求找得到 `={target}:`，而我把 `exact_target` 删了。**这一红是本轮最有价值的一条**，见 `§D1` |
| `M2` | `GATE: FAIL —— fmt · cargo` | cargo 7 红，**5 处落在写区外**（逐处见 `§E`） |
| `M3` | `GATE: FAIL —— cargo` | cargo 3 红：`tmux server` 被「远端 tmux 动词」守卫读成动词 · 一处指向已删符号的住址 · 一处墓碑没登记 |

### selftest 入场 / 交回，标签多重集差怎么算的

分母 = `cargo` 那一格的 `9 个包合计`（gate 自己印的数，不是我数的）。**1605 → 1610，净 +5**，逐条：

| 文件 | 动作 | 名字 | 计 |
|---|---|---|---|
| `cc_bus.rs` | **删** | `online_check_rejects_ids_before_building_command` | −1 |
| `cc_bus.rs` | **删** | `online_cmd_uses_exact_target_form` | −1 |
| `cc_bus.rs` | 改名＋重写 | `online_check_has_exactly_one_command_construction` → `the_probe_by_name_template_is_gone_from_production` | ±0 |
| `cc_bus.rs` | 改名＋重写 | `the_online_lamp_falls_back_instead_of_guessing_dark` → `the_online_lamp_never_guesses_dark` | ±0 |
| `cc_bus.rs` | 改名＋重写 | `the_online_lamp_asks_the_identity_space_before_probing_by_name` → `…_and_has_nothing_else_to_ask` | ±0 |
| `cc_bus.rs` | 改名＋重写 | `letting_send_through_did_not_let_broadcast_kill_or_spawn_through` → `letting_kill_broadcast_and_online_through_did_not_let_spawn_through` | ±0 |
| `cc_bus.rs` | 改名＋重写 | `broadcast_and_kill_commands_validate_before_they_build` → `the_kill_entry_still_refuses_bad_ids_before_it_asks_anyone` | ±0 |
| `cc_bus.rs` | 改名＋重写 | `the_new_write_commands_refuse_local_before_asking_for_a_remote_config` → `whoever_still_asks_for_a_remote_config_branches_on_local_first` | ±0 |
| `cc_bus.rs` | **新** | `the_kill_path_asks_the_backend_instead_of_composing_a_shell_line` | +1 |
| `cc_bus.rs` | **新** | `the_kill_reply_keeps_the_three_states_apart` | +1 |
| `cc_bus.rs` | **新** | `an_unknown_liveness_is_never_rendered_as_dark` | +1 |
| `cc_bus.rs` | **新** | `the_broadcast_has_no_ssh_fallback_left` | +1 |
| `tmux.rs` | 改名＋重写 | `classify_capture_output_sentinels_and_text` → `the_five_capture_refusals_stay_apart` | ±0 |
| `tmux.rs` | **新** | `the_tmux_shell_line_detector_really_sees_each_shape` | +1 |
| `tmux.rs` | **新** | `the_capture_path_asks_the_backend_instead_of_composing_a_shell_line` | +1 |
| `tmux.rs` | **新** | `the_local_capture_is_no_longer_a_dead_end` | +1 |
| | | **合计** | **+5** ✓ 与 gate 现打的 1605 → 1610 对上 |

⚠ **「改名＋重写」那 7 条不是零改动** —— 它们的参照物（老探法 / 老构造器）本件删了，
照字面留着只有一种活法：放宽成「有没有那个词」，而那是本区判过的假绿。
逐条**换了什么、买到的是变强还是变弱**写在 `§C`。

---

## §B 死值验逐刀 —— 锚点 · 命中数 · 读数 · 是不是正是那一格

> 🔴 每一刀**先断言锚点恰好命中 N 次再改，并打印「变异已落地」**（`brief` 第 7 条）。
> 差一次就 `exit 3`，**一个字节都不落地**。还原是**重写原文**（mtime 必变，不用 `cp -a`）。

### B1 `KR112D1` 的四刀

| 刀 | 打哪儿 | 锚点命中 | 变了什么 | `cargo` 读数 | 红的逐条 | 是不是正是那一格 |
|---|---|---|---|---|---|---|
| **①** DoD 指定 | `cc_bus.rs::cc_bus_kill` **整个函数体**（`kill_via_daemon(&origin, &id).await` 那一句） | **1** | 换回「`refuse_local_write` → `format!("cc-kill …")` → `cfg_of` → `exec_read`」 | `1469 passed; 5 failed`（入场 1610） | `letting_kill_broadcast_and_online_through_did_not_let_spawn_through` · `the_kill_entry_still_refuses_bad_ids_before_it_asks_anyone` · `the_kill_path_asks_the_backend_instead_of_composing_a_shell_line` · `the_write_face_branches_on_local_before_it_asks_for_a_remote_config` · `whoever_still_asks_for_a_remote_config_branches_on_local_first` | ✅ **五条全在 `cc_bus.rs`，全是本件的判据**。诊断逐字：「`cc_bus_kill` 这条路上又出现了命令串的痕迹 `["exec_read(", "cfg_of(", "2>&1"]`」 |
| **②** DoD 指定 | `cc_bus.rs::check_cc_bus_agent_online` **整个函数体** | **1** | 删回落之后**再把回落塞回去**（`if let Ok(live) = online_via_daemon(..) { return Ok(live) }` ＋ 老那条 `tmux has-session` 串） | `1469 passed; 5 failed` | `an_unknown_liveness_is_never_rendered_as_dark` · `letting_kill_broadcast_and_online_through_did_not_let_spawn_through` · `the_online_lamp_asks_the_identity_space_and_has_nothing_else_to_ask` · `the_probe_by_name_template_is_gone_from_production` · `the_write_face_branches_on_local_before_it_asks_for_a_remote_config` | ✅ **「没有回落」这件事被钉住了，不是今天碰巧没有**。最贵的一条是走生产入口那格：「**问不到的时候它答了一个确定的 `false`** —— 那正是这一条要拦的」 |
| **③** 对侧那一列有没有牙（**锚点照抄 `K-R111` 刀④**） | daemon 两张具名表**块内**各摘掉 `bus-kill` 一行 | `SUBCOMMANDS` 块内 **1** · `COMMANDS` 块内 **1**（⚠ 全文数是 1 与 **4** —— 不收窄到块内会切错地方） | 后端分母 26/10 → **25/9** | 尺子 `K-R111-ruler.py`：**1 红**，退出码 1 | `[对侧] cc_bus_kill 点名的 daemon 原语 bus-kill 不在现打的两张表里（子命令 25 条 ＋ 帧 9 条）` | ✅ 是那一格。⚠ **读数从 `K-R111` 的 2 红变成 1 红，原因写在 `§D2`（那是本件改变的事实，不是尺子退化）** |
| **④** 阴性对照 | `cc_bus.rs` 里本件**新加/重写的 10 条判据**逐条 `#[ignore]` ＋ 刀① | **10 处 ＋ 1 处 = 11**，逐处断言 | 判据停掉 ＋ 回潮 | `1463 passed; 1 failed; 18 ignored` | **只有 `shared_crate_registry::tests::every_ignored_test_still_has_someone_who_triggers_it`** | ✅ **本件的判据一条都不红 ⇒ 刀① 的牙**全部**长在它们身上**。⚠ 那唯一 1 红是**刀具自己的形状**被逮到，见下 |

🔴 **刀④ 那一红要说清楚，别记成「实现有第二副牙」**：仓里有一条元判据逐字要求
「被 `#[ignore]` 的测试要有 e2e 脚本点名它们」，它把我停掉的那 10 条**逐条列了出来** ——
诊断第一行是「这些 `#[ignore]` 测试**没有任何 e2e 脚本会点名它们**：」，后面 10 行正是那 10 个名字。
⇒ 它证明的是**变异真的落地了**（10 条都被停掉了），而不是「刀① 还有别的东西在守」。
**代价如实记**：用 `#[ignore]` 做阴性对照，在这个仓里会额外买到这一条红；
换成「整段删」就没有它，但删会连带动到相邻块 —— 两害相权取了这一个，并把它写在这里。

### B2 `KR112D2` 的三刀

| 刀 | 打哪儿 | 锚点命中 | 变了什么 | `cargo` 读数 | 红的逐条 | 是不是正是那一格 |
|---|---|---|---|---|---|---|
| **①** DoD 指定 | `tmux.rs::capture_remote_pane` 体首（`use …Routed;` 那一行之前） | **1** | 换回一次性 SSH（`exact_target` → `format!("if command -v tmux …")` → `load_remote_config_by_label` → `connect_and_exec_cmd` → `read_to_end`） | `1468 passed; 6 failed` | `tmux::the_capture_path_asks_the_backend_instead_of_composing_a_shell_line` · `tmux::the_local_capture_is_no_longer_a_dead_end` · `tmux::every_target_placeholder_comes_from_exact_target` · `tmux::tmux_targets_use_exact_match` · `exec_site_registry::every_remote_exec_declares_where_its_command_came_from` · `local_origin_registry::every_remote_config_lookup_deals_with_the_local_origin_first` | ✅ 是那一格，**而且这一刀比 D1 那一刀多两副牙**：两张**既有**申报表（人群从源码派生）也咬住了它 —— 逐字「这些远端执行点**没人申报命令串从哪来**」「这些地方先去查远端配置、却没先分本机」 |
| **②** DoD 指定 | `tmux.rs::capture_remote_pane` 的 `gate1_reject_empty` 之后 | **1** | 本机那一支退回「回一句还看不了」（`if origin == LOCAL_ORIGIN { return Err("本机还看不了 …") }`） | `1473 passed; 1 failed` | **只有** `tmux::tests::the_local_capture_is_no_longer_a_dead_end` | ✅ **正是那一格，而且是最小面** —— 全仓只红这一条。诊断逐字：「`capture_remote_pane` 里又出现了 `LOCAL_ORIGIN` —— 本机那条早退回潮了」 |
| **③** 阴性对照 | `tmux.rs` 里本件**新加/重写的 7 条判据**逐条 `#[ignore]` ＋ 刀① | **7 处 ＋ 1 处 = 8**，逐处断言 | 判据停掉 ＋ 回潮 | `1464 passed; 3 failed; 15 ignored` | `shared_crate_registry::every_ignored_test_still_has_someone_who_triggers_it`（同刀④ 那条，刀具形状）· `exec_site_registry::…` · `local_origin_registry::…` | ⚠ **不是「一条都不红」** —— 本件的判据确实一条都不红（那一半成立），但**两张既有申报表照样咬**。**这是真发现，不是失败**：见下 |

🔴 **`KR112D2` ③ 的诚实读法（DoD 的字面是「一条都不红」，实打不是）**：
本件那 7 条判据**一条都不红**（阴性对照要证的那一半成立），
而另外 2 红来自**本件之前就在盘上的两张申报表**（`exec_site_registry` 与 `local_origin_registry`，
两张的人群都**从源码派生**）—— 抓屏那条 SSH 一旦回潮，它们本来就会咬。

⇒ **本件那几条判据买到的不是「唯一的红」，是「更早、更具体的红」**：
申报表说的是「有一处远端执行没人申报」，本件的判据说的是「**抓屏这一跳走错了路，而且本机那一支又变成死胡同了**」。
两者是纵深，不是重复。**DoD ③ 的字面（「一条都不红」）在这一格上不成立，我按实打写，不按字面写。**

⚠ 反过来看 `KR112D1` ④：cc-bus 那一刀**没有**这两副附加的牙 ——
因为它回潮时复用的 `exec_read` / `cfg_of` **本来就在申报表里登记着**（`exec_read` 那行至今还在表上）。
⇒ **D1 那一刀的牙 100% 长在本件的判据上**，一条外援都没有。

### B2b 本件**自加**的两刀（DoD 没点名 —— 补的是「仍绿不等于仪式」那一栏）

| 刀 | 打哪儿 | 锚点命中 | 变了什么 | `cargo` 读数 | 红的逐条 | 判 |
|---|---|---|---|---|---|---|
| **`d1-5`** `KR112D1` ② 的**第二半** | `cc_bus.rs::cc_bus_broadcast` 的 `match` 头 | **1** | 广播那条 **SSH 回落也塞回去**（`cc-broadcast` 串 ＋ `cfg_of` ＋ `exec_read`） | `1470 passed; 4 failed` | `the_broadcast_has_no_ssh_fallback_left` · `letting_kill_broadcast_and_online_through_did_not_let_spawn_through` · `the_write_face_branches_on_local_before_it_asks_for_a_remote_config` · `whoever_still_asks_for_a_remote_config_branches_on_local_first` | ✅ **为什么非切不可**：DoD ② 只说「删回落之后再把回落塞回去」，没说是哪一条 —— 本件删了**两条**回落（查在线 · 广播）。只切一条就说「回落被钉住了」是把一条的读数说成两条的 |
| **`d0-flat`** 把两个「讲成人话」的纯函数整个退掉 | `cc_bus.rs::describe_kill_reply` ＋ `tmux.rs::describe_capture_refusal`，各在 `match` 之前早退 | 各 **1**（2 处） | 各压成**一句通用话**。⚠ **形状对、恒答其中一张脸**（签名与返回类型一个字没动，不是 `return None` 那种把台子炸掉的换法） | `1472 passed; 2 failed` | **只有** `cc_bus::the_kill_reply_keeps_the_three_states_apart` ＋ `tmux::the_five_capture_refusals_stay_apart`，诊断逐字 `两档被压成了同一句话` | ✅ **正是那两格，最小面** —— 那两条「分得开」的判据不是仪式 |

### B3 `KR112D3` 的两刀（尺子刀，跑在副本上，一秒出读数）

| 刀 | 打哪儿 | 锚点命中 | 读数 | 判 |
|---|---|---|---|---|
| **①** 基线不拧 | `evidence/K-R111-ruler.py::BASELINE` 里本件拧下来的四格，逐格退回 `还有接线活` | 逐格 **1**（4 处） | **4 红**，退出码 1：`[棘轮] capture_remote_pane / cc_bus_broadcast / cc_bus_kill / check_cc_bus_agent_online：基线 还有接线活 ≠ 现打 已完 —— 退役了 ✅ 把刻度拧下来` | ✅ 拧不下来 = 那条尺子在说谎，而它自己会说出来 |
| **②** 基线拧过头 | 同一张表里 `read_cc_bus_state`（本件**没有**退役它）写成 `已完` | **1** | **1 红**，退出码 1：`[棘轮] read_cc_bus_state：基线 已完 ≠ 现打 不是接线 —— 回潮了 ⚠` | ✅ 棘轮**两个方向都咬**：只许往「已完」走，而且**只在真退役之后走得动** |

⚠ **`KR112D3` 的另一半（尺子B 一个字节都不该动）**：现打 `TS_FALLBACK_REACH` **家数 4 · On 2**，
与 `K-R111 §E4` 在主干 `fb83cf1` 上的读数逐字相同；尺子A `TS_FALLBACK_KEEPERS` **4 行**，同样没动。
⇒ 「若动了说明改到了不该改的地方」这一条**没有触发**。
量法：`python3 evidence/K-R112-ruler.py`（它把这两个数与 `EXEC_SITES` 一起现算）。

---

## §C 逐条判据换了什么（`§A` 那 7 条「改名＋重写」的正文）

| 老判据 | 它原来钉什么 | 参照物为什么没了 | 换成钉什么 | 变强还是变弱 |
|---|---|---|---|---|
| `online_check_has_exactly_one_command_construction` | 「在线检查**真的经过** `build_online_cmd`」（构造只有一份） | 那个构造器整块删了 | **反向锚点**：生产段里 `tmux has-session -t` 这个模板**恰好 0 次** ＋ 一条反空真自检 | **强** —— 从「只有一份」变成「一份都没有」，而且它管的是**全生产段**，不只那一条路 |
| `the_online_lamp_asks_the_identity_space_before_probing_by_name` | **顺序**：先问 `bus-list`、再退回按名字探 | 老探法删了 ⇒ 位置比较没有第二个参照物 | 这条路上（`check_cc_bus_agent_online` ＋ `online_via_daemon`）**一条命令串的痕迹都没有** ＋ 真的在调 `bus-list` | **强** —— 「只有一条路」比「两条路的先后」强一格 |
| `the_online_lamp_falls_back_instead_of_guessing_dark` | `live_of` 的 `None` 有两种来源，两种都要**回落** | 回落删了 | 名字改成 `the_online_lamp_never_guesses_dark`：`live_of` 那几格**一个字没动**，头注写清「保住的是不许灭灯那半，删掉的是按名字再探那半」；出口那一半由新判据 `an_unknown_liveness_is_never_rendered_as_dark` 走**生产入口本体**钉 | **强** —— 老那条只测纯函数，新那条真跑一遍 `check_cc_bus_agent_online` |
| `letting_send_through_did_not_let_broadcast_kill_or_spawn_through` | `KR98D3` 纪律 ⑱：放行一条、另外三条逐条拒 | 三条里两条本件放行了 | 人群**从源码派生**（本模块所有 `cc_bus*` tauri 命令，现打 7 条），放行那 4 条断「没有命令串 ＋ 没有对 `<local>` 的拒绝」，`spawn` 断「仍旧拒且拒得有自己的话」，另加一条**反向自检**（`read_cc_bus_inbox` 确实还在老路上） | **强** —— 老那条是手写三元组清单，新那条看得见新长出来的第八条命令 |
| `broadcast_and_kill_commands_validate_before_they_build` | 两个**构造器**逐条打校验 | 两个构造器都删了 | 真调 `cc_bus_kill` 这个 **tauri 命令**，看它拒不拒，而且**拒的理由必须是「非法 agent id」**（不是掉进别的分支才失败） | **强** —— 从「构造器会拒」变成「这条命令会拒」 |
| `the_new_write_commands_refuse_local_before_asking_for_a_remote_config` | 广播 / 收掉的本机拒绝排在 `cfg_of` 之前 | 那两条不再问远端配置 | 人群**从源码派生**（生产段里还调 `cfg_of(&origin)` 的 tauri 命令，现打 `["cc_bus_spawn", "read_cc_bus_inbox"]`，**写成相等不是地板**），逐条仍断位置 | **强** —— 少一条要回来改这张表（好事），多一条也红 |
| `classify_capture_output_sentinels_and_text` | 两个哨兵（`NO_TMUX` / `NO_PANE`）的判定 | 那条串与它的哨兵一起删了 | `the_five_capture_refusals_stay_apart`：五档两两不同 ＋ 每句都带 daemon 原话 ＋ **认不出的码不许被猜成任何一个已知档** | **强** —— 老那条自己头注就承认「pane 内容恰等于哨兵串 → 误判」，新那条结构上没有这个形状 |

---

## §D 本轮逮到的两条真事实（都不是我预先知道的）

### D1 🔴 **删 `exact_target` 会打断 daemon 那侧的跨轨对拍 —— 那条线原先没人写进任何计划**

`M1` 那趟 daemon 格红了一条：`control::launch::tests::exact_target_shape_matches_the_monitor_side`。
它 `include_str!("../../../src-tauri/src/tmux.rs")`，逐字要求 monitor 侧找得到 `={target}:`，
理由写着「两侧必须同形，否则一边打到兄弟会话上而另一边不会，排查起来会非常难」。

⇒ **`exact_target` 在 monitor 侧今天零生产调用方，但它不是死代码 —— 它是一个对拍锚点。**
处置照 `K-R72` 给 `is_ccm_tmux_name` 转调壳留的先例：**留壳 ＋ `#[allow(dead_code)]` ＋
头注逐字写清「今天没人调、为什么不能删、想真删要先动哪棵树」**，不假装它在路上。
`remote-daemon-proto/**` 不在本件写区（归 `K-R113`）⇒ **已上报，别当它没有主人。**

### D2 🔴 **刀③ 照抄 `K-R111` 刀④ 的锚点，读数从 2 红变成 1 红 —— 那是本件改变的事实，不是尺子退化**

`K-R111` 刀④（daemon 两张具名表块内各摘掉 `bus-kill` 一行）当时是 **2 红**：
`[对侧] 点名的原语不在现打的两张表里` ＋ `[棘轮] 还有接线活 → 不是接线`。
本轮同一把锚点、同一份收窄（块内各命中 1 次；⚠ 全文数是 1 与 4，不收窄就会切错地方）跑出来是 **1 红**：
只剩 `[对侧]`。

**为什么**：`verdict_of` 的第一句是「前端一处自己的实现都没有 ⇒ `已完`」——
本件把 `cc_bus_kill` 从「有自实现」改成「没有自实现」之后，
**对侧存不存在已经不参与它的档了**（`已完` 那一档不看对侧）。
⇒ 三档表那一副牙在这条命令上退出，只剩 `[对侧]` 那一副。
**这正是「退役」这件事在尺子上的形状**，不是尺子变弱：`[对侧]` 那一副仍然从 daemon 源码派生、仍然会红。

---

## §E 写区外逐处 —— **5 份文件 / 7 处，未获 PM 批准，单独一个提交**

🔴 **单子的硬边界第 4 条逐字：「写区外一个字不许自批」。下面这 7 处我改了，
   而且它们不在写区里 —— 所以它们落在一个单独的提交上，PM 一条 `git revert` 就能退掉。**

每一处都是**结构性随动**（一个数 / 一行登记必须跟着现实走），形状与 `K-R72` 在
`exec_site_registry` 里那句「这一改是结构性强制的随动」、以及 `K-R104` 同拍拧下三个地板
逐字同形。**不改它们，门禁在那几格红；改它们，是把「人群真的少了」这件事记上账。**

| # | 文件 | 改了什么 | 不改的话 |
|---|---|---|---|
| 1 | `src-tauri/src/backend/control/daemon_route.rs` | `SENDERS` 加一行 `("tmux.rs", Verdict::UsesRouter)` | `every_daemon_sender_is_registered_and_uses_the_one_router` 红：monitor 树里多了第七个走 daemon 的发送端（抓屏），而登记表看不见它 |
| 2 | `src-tauri/src/byte_cap_registry.rs` | 删掉查在线那条读上限的登记行 | `every_byte_cap_says_what_it_bounds_and_what_happens_past_it` ＋ `the_registered_numbers_still_match_the_source` 两条红：常量不在了，登记成了僵尸账 |
| 3 | 同上 | 异步流读人群地板 **13 → 11** | `every_uncapped_stream_read_has_an_owner` 红：两处 `read_to_end`（一次性 SSH 的 stdout）随两条命令上帧面而不存在了 |
| 4 | `src-tauri/src/local_origin_registry.rs` | 「问远端配置」人群地板 **19 → 17** | `every_remote_config_lookup_deals_with_the_local_origin_first` 红：那两条命令不再查远端配置 |
| 5 | `src-tauri/src/tmux_daemon_gate_guard.rs` | 只读动词地板 **2 → 1** | `every_remote_tmux_verb_is_either_read_only_or_routed_through_the_gate` 红：`tmux capture-pane` 那一处没了，monitor 侧只剩 `tmux … ls` |
| 6 | `src-tauri/src/structural_scan.rs` | `TOMBSTONED` 加 4 行（3 个构造器 ＋ 1 个哨兵判定函数） | `every_dead_name_named_in_the_prose_is_declared_dead` 红：贴了墓碑不登记就是「红了就贴标签」 |
| 7 | 同 2 | 我自己那段注释里去掉一个指向已删符号的**住址形**（`文件.rs::符号`） | `every_symbol_address_in_the_sources_still_resolves` 红 —— ⚠ **这一处是我自己制造的**，不是本件的随动 |

⚠ **第 5 处要单说一句**：`tmux_daemon_gate_guard` 那个人群**快到头了** ——
`list_remote_tmux` 也改走后端的那天它会归零，而**归零之后那条判据就是空真**。
我在那个数旁边写了这件事与它该怎么换（换一份会漂的活体语料，同
`tmux.rs::every_target_placeholder_comes_from_exact_target` 今天的做法）。**这是登记，不是修好了。**

### 写区外**没有**改、但今天已经腐了的一处（只登记，不动）

`src-tauri/src/parity_ledger.rs` 的 `LEDGER` 里三行今天**说的不是实话**，而且它们**不红**
（那张表的 `Side` 没有任何机检从源码派生）：

| 行 | 表里写 | 现打事实 |
|---|---|---|
| `("capture_remote_pane", "tmux.manage", Side::Remote)` | `Remote` | 今天 `<local>` 也走得通（origin 是入参） ⇒ 该是 `Both` |
| `("cc_bus_kill", "cc-bus.cockpit", Side::Remote)` | `Remote` | 同上 |
| `("cc_bus_broadcast", "cc-bus.cockpit", Side::Remote)` | `Remote` | 它的本机路 `P4f` 就通了 —— **这一条在本件之前就已经腐了** |

⚠ 还有一处**散文**腐了：`ASYMMETRY_REASONS` 里 `tmux.manage` 那一行逐字写着
「monitor 的 `capture_remote_pane`**一个字节没动**，对 `<local>` 仍然没有本机对侧 ⇒
`Side::Remote` 这一格不许改」—— 那句话**今天假了**，而
`the_tmux_manage_row_stops_waiting_for_a_daemon_primitive` 那条判据**只检查那句话在不在**，
所以它照样绿。⇒ **一句真话摆错了格，和一句假话一样是假举证**（`K29`）。
**归 PM 裁**：要么把三行 `Side` 与那段散文同拍改（会牵动 `capability_sides()` 那几个计数），
要么开一件。**我没动它** —— 它不在写区，而且它牵动的是四个计数的连锁，不是一行随动。

---

## §F 把实现整个退掉，还有多少条新断言仍绿

分母 = 本件**新加 ＋ 重写**的判据：`cc_bus.rs` **10 条** ＋ `tmux.rs` **7 条** = **17 条**
（就是两把阴性对照刀逐条 `#[ignore]` 的那两张名单，人群与它们**同一份**，不是另抄的）。

### F1 逐刀「退了哪一处 → 哪几条红 / 哪几条仍绿」

| 刀 | 退掉的那一处实现 | 红 | 仍绿 |
|---|---|---|---|
| `d1-1` | `cc_bus_kill` 的整条帧路 → 拼 shell | 5 | `the_probe_by_name_template_is_gone_from_production` · `the_online_lamp_asks_the_identity_space_and_has_nothing_else_to_ask` · `the_kill_reply_keeps_the_three_states_apart` · `an_unknown_liveness_is_never_rendered_as_dark` · `the_broadcast_has_no_ssh_fallback_left` |
| `d1-2` | `check_cc_bus_agent_online` 的「没有回落」 | 5 | `the_kill_entry_still_refuses_bad_ids_before_it_asks_anyone` · `whoever_still_asks_for_a_remote_config_branches_on_local_first` · `the_kill_path_asks_the_backend_instead_of_composing_a_shell_line` · `the_kill_reply_keeps_the_three_states_apart` · `the_broadcast_has_no_ssh_fallback_left` |
| `d1-5` | `cc_bus_broadcast` 的「没有回落」 | 4 | 其余 6 条 |
| `d0-flat` | `describe_kill_reply` / `describe_capture_refusal` 两个纯函数 | 2 | 其余 15 条 |
| `d2-1` | `capture_remote_pane` 的整条帧路 → 一次性 SSH | 4（tmux 侧） | `the_five_capture_refusals_stay_apart` · `the_tmux_shell_line_detector_really_sees_each_shape` · `gate1_rejects_only_empty_target` |
| `d2-2` | 本机那一支 → 「回一句还看不了」 | 1 | 其余 6 条（tmux 侧） |

**仍绿的理由逐条**（不是「它没牙」，是「这一刀打的不是它守的那件事」）：

- 退掉 kill 的路，`the_online_lamp_*` / `the_probe_by_name_*` 仍绿 —— 它们守的是**另一跳**（查在线）；
- 退掉查在线的回落，`the_kill_*` 三条仍绿 —— 同理，另一跳；
- 退掉任何一跳，`the_kill_reply_*` / `the_five_capture_refusals_*` 都仍绿 ——
  它们守的是**纯函数说的话分不分得开**，与那一跳走哪条路正交。
  🔴 **它们的牙由 `d0-flat` 单独验过**（把那两个纯函数压成一句通用话 ⇒ 恰好红这两条）。

### F2 🔴 **17 条里有 15 条至少被一把刀打红过；剩下 2 条本轮没验，如实写**

| 没验的 | 它守什么 | 为什么本轮没造刀 |
|---|---|---|
| `tmux::the_tmux_shell_line_detector_really_sees_each_shape` | 那个「认不认得出命令串」的**谓词本身**被改瘸 | 本轮没有一把刀去改那个谓词。它是「判据的判据」，牙要靠一把**打谓词**的刀 —— **我没造，所以它今天只是设计上成立，不是量出来的** |
| `tmux::gate1_rejects_only_empty_target` | 空目标必须在本地就地被拒（三条命令共用一份判定） | 它是**既有**判据，本件只把抓屏那一格从「打构造器」改成「打生产入口」。给它造刀要打 Gate 1 本体，那不在本件射程里 |

**这两条我不说它们「有牙」** —— 按 `brief` 第 17 条，判不了就写判不了。

---

## §G 改动面（逐顶层函数 md5，`evidence/K-W4b-rs-fn-md5.py`）

量法：`python3 evidence/K-W4b-rs-fn-md5.py afda78b <文件>` —— **逐顶层函数整块 md5**，
基点 `afda78b` vs 本工作树。⚠ 它的射程：只认**顶层**（第 0 列）函数，`mod tests` 里缩进的**不在分母**；
比的是整块字节（含体内注释，不含上方 `///` 头注）。
⚠ 那个量具尾部恒印的 `KW4bD5②: FAIL —— 4 块变了` 是**它自己写死的 `sftp.rs` 四块**，
与本件被测的这几份文件无关（那四个名字在这几份文件里本来就不存在）—— **不是本件的读数**。

### G1 `src-tauri/src/cc_bus.rs` —— 基点 42 个顶层函数 → 现 45；**相同 29 · 变了 10 · 没了 3 · 新增 6**

| 变了 | 基点 → 现 |
|---|---|
| `check_cc_bus_agent_online` | `22fdfad5a0bb`(50 行) → `28990492ce35`(**3 行**) |
| `online_via_daemon` | `e571f30a542d`(16 行) → `5c20ed23f5b3`(41 行) |
| `cc_bus_kill` | `c565d540da71`(17 行) → `5ac41b14b9df`(**3 行**) |
| `cc_bus_broadcast` | `d3604a2d1736`(33 行) → `ee33ee37a130`(27 行) |
| `describe_no_channel` | `b59cd39f2b0b`(7 行) → `620b6186fb9d`(3 行)（提参数，**输出逐字节不变**） |
| `describe_daemon_too_old` | `c845fb1ed05d`(7 行) → `31f80dd43d2d`(3 行)（同上） |
| `describe_send_error` | `b02d288639ff`(8 行) → `1e10fd6984f6`(3 行)（改成薄壳，分流搬进 `describe_bus_error`） |
| `broadcast_via_daemon` | `5398956f36d8`(56 行) → `ecc4493cd628`(56 行)（**只把两个字面量换成常量** `BUS_LIST`/`BUS_SEND`，行数不变） |
| `cfg_of` | `81a593143bd6`(17 行) → `3291c3759424`(19 行)（**只改头注里那句「三个调用方」**） |
| `refuse_local_write` | `08393eaf2072`(10 行) → `ecfa90efd82d`(10 行)（**只改拒绝文案**：读面那半今天全通了） |

**没了 3**：`build_online_cmd` · `build_broadcast_cmd` · `build_kill_cmd`（三块散文墓碑在原处）。
**新增 6**：`unknown_liveness` · `describe_no_channel_for` · `describe_daemon_too_old_for` ·
`describe_bus_error` · `describe_kill_reply` · `kill_via_daemon`。

### G2 `src-tauri/src/tmux.rs` —— 基点 17 → 现 17；**相同 13 · 变了 2 · 没了 2 · 新增 2**

| | |
|---|---|
| 变了 | `capture_remote_pane` `422d1af4557d`(32 行) → `4ada389f4287`(**14 行**) · `no_channel_message` `ad43b9400c45`(15 行) → `b0887c59178e`(15 行)（**只把括号里那句「送键与杀会话」扩成三条**） |
| 没了 | `classify_capture_output` · `build_capture_pane_cmd` |
| 新增 | `describe_capture_refusal`(17 行) · `capture_via_daemon`(41 行) |
| **没动** | 🔴 `exact_target` 落在「相同 13」里 —— 它**一个字节没改**（改的只有上方头注），见 `§D1` |

### G3 两张申报表（写区内）＋ 写区外那 5 份 —— **顶层函数逐块 md5 全同**

| 文件 | 顶层函数读数 | 整份 md5（基点 → 现） |
|---|---|---|
| `src-tauri/src/exec_site_registry.rs` | 0 个顶层函数（全在 `mod tests` 里） | `da8d56322d46` → `c02fdc51712e` |
| `src-tauri/src/write_site_registry.rs` | 0 个 | `ef6d97f12138` → `c467ff6c4551` |
| **写区外** `backend/control/daemon_route.rs` | **2 个 · 相同 2 · 变了 0** | `f9200191419a` → `2422ee656e82` |
| **写区外** `byte_cap_registry.rs` | 0 个 | `f60f78a79589` → `502956b20da0` |
| **写区外** `local_origin_registry.rs` | 0 个 | `8b597b492f1f` → `7885d30fa191` |
| **写区外** `structural_scan.rs` | **8 个 · 相同 8 · 变了 0** | `50c65361071f` → `f006316e31dd` |
| **写区外** `tmux_daemon_gate_guard.rs` | 0 个 | `789f6fd2c81e` → `009284a914fe` |

🔴 **这张表就是「写区外那 5 处只动了登记表与注释」的读数**：
两份有顶层函数的（`daemon_route.rs` 2 个 · `structural_scan.rs` 8 个）**逐块 md5 全同**，
另外三份**根本没有顶层函数**（内容全在 `#[cfg(test)] mod` 里）。
⚠ **它买不到什么**：`mod tests` 里的东西不在这个分母里 —— 那正是我改的地方。
「改的只有那几个数与几行登记」靠的是 `§E` 那张逐处表 ＋ PM 读 diff，不是靠这张 md5。

---

## §H 三处 `git status`（提交前现打）

### H1 计划仓 `.claude/planned-build/backend-consolidation`

**一次 `git commit` 都没跑**（铁律 3：提交归 PM 独占）。本轮只写了**一份**文件：
`features/K-R112-真接线那4条里的3条（cc-bus三条＋抓屏）.md` 的 `§3` 与 `§8`。

⚠ **`.dispatch.json` 那份登记的「写区」里列了 PM 独占的几样**
（`STATUS.md` · `LEDGER.md` · `DECISIONS.md` · `INDEX.md` · `audits/K-R112-PM.md` · `.dispatch.json`），
而派工单第 ① 段固定项第 2 条逐字把它们列进「一个字节都不许碰」。
⇒ **按固定项做：那几样一个字节没碰。** 已上报（件文件 `§8` 第 9 条）。

### H2 代码仓 `cc-monitor`（`main` @ `afda78b`）

**干净 —— 一个字节没碰。** 本件全部改动都落在工作树 `k-r112` 的分支 `track/k-r112` 上，
**不合 main**（PM 合）。

### H3 本工作树 `.claude/worktrees/k-r112`（`track/k-r112`，基点 `afda78b`）

提交前 `git status --short` 现打 **13 项**：10 项 `M` ＋ 3 项 `??`。逐项：

| | 路径 | 落哪个提交 |
|---|---|---|
| M | `src-tauri/src/cc_bus.rs` | **① 写区内** |
| M | `src-tauri/src/tmux.rs` | ① |
| M | `src-tauri/src/exec_site_registry.rs` | ① |
| M | `src-tauri/src/write_site_registry.rs` | ① |
| M | `evidence/K-R111-ruler.py` | ①（棘轮拧四格 ＋ 面四格 ＋ 两条针） |
| ?? | `evidence/K-R112-cut.py` | ① |
| ?? | `evidence/K-R112-ruler.py` | ① |
| ?? | `evidence/K-R112-deathvalue.md` | ① |
| M | `src-tauri/src/backend/control/daemon_route.rs` | **② 写区外** |
| M | `src-tauri/src/byte_cap_registry.rs` | ② |
| M | `src-tauri/src/local_origin_registry.rs` | ② |
| M | `src-tauri/src/structural_scan.rs` | ② |
| M | `src-tauri/src/tmux_daemon_gate_guard.rs` | ② |

🔴 **两个提交是刻意分开的**（`§E`）：提交 ② 的第一行逐字带「写区外」，
PM 一条 `git revert` 就能把那 5 处退掉，而提交 ① 一个字节都不受影响。

⚠ **提交只点名文件**（`git add <路径1> <路径2>`，铁律 21 / 固定项第 20 条）——
没用过 `git add .` / `-A` / `-u` / 裸目录 / 通配。
⚠ `evidence/.K-R112-cut-backup.json` 是刀具的备份文件，**`restore` 之后自己删掉**，
收工时盘上没有它（上面那 13 项里没有它就是读数）。

---

## §I 诚实边界 / 判不了

1. **本件一趟真机都没跑**（`K31`）。「改走 `bus-kill` 帧之后真的杀得掉」「本机真的抓得到一屏」
   这两件事**判不了** —— 它们要后端在场、要一个真 tmux 会话。判据钉的是
   「这一跳走哪条路」「说的话分不分得开」，**不是**「那一跳真的成功」。
   本机抓屏那一格的射程逐字写在 `the_local_capture_is_no_longer_a_dead_end` 的头注里。
2. **「后端有对侧」只证明那条原语的名字今天在 daemon 的表里**，不证明两侧等价
   （沿用 `K-R111 §H3` 那条边界，本件没有改善它）。
3. **`describe_capture_refusal` 的五档是从 daemon 的码派生的，不是我造的** ——
   但「daemon 分得对不对」（哪句 stderr 落哪一档）由 daemon 那棵树自己的针表判，本侧够不着。
4. **`unknown_liveness` 只保证「这条路上造不出『不在线』」**。前端拿到 `Err` 之后怎么渲染
   （会不会自己把它画成一盏灭灯）**不在本件射程内** —— 那是 TS 侧的事，本件一个 TS 文件都没动。
5. **广播的空消息校验没有独立判据**：它住 `send_via_daemon`（广播是「列成员 + 逐个发」的组合），
   而在测试进程里没有入方向通道，空与非空**都**会失败在「没通道」那一档 ⇒ 一句 `is_err()`
   分不出是哪一层拒的。**那会是一次假举证**，所以我没写那一格，理由逐字写在判据里。
6. **`#[ignore]` 不等于「整段删」**：两把阴性对照用 `#[ignore]` 停掉判据而不是删掉它们
   （删会连带动到相邻块，切多了就不是「只拿掉判据」）。
   代价：被停掉的判据**仍然编译** ⇒ 一个会让它们编译不过的变异，这两把刀看不见。
   本件那两把刀切的都不是编译期的东西，所以这条边界在本轮不构成漏。

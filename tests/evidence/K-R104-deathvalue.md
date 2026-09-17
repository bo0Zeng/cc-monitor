# K-R104 死值验原文

四条 dod ＋ `7u` 各自的刀、逐刀**完整原文读数**住这里；件文件 `§3` 只放摘要。

## 规矩（缺一条这份证据不算数）

- **全部在沙箱里跑**：`PB_WS=backend-consolidation .claude/devbox/gate <本工作树绝对路径> <tag>`。**不许直接在本机跑**（`K31`/`R15`）。
  ⚙ 本轮**一次都没在宿主上跑过 cargo / npm / 任何测试**；宿主上只做过 `grep` / `git show` / `python3` 读写文件。
- **一趟一刀**，下一刀前还原干净，逐刀 `git diff --quiet` 自证。
- 🔴 还原**不用** `cp -a`、**不用** `git checkout <文件>`：刀具**逐处反向替换**，还原后自己再跑一次 `git diff --quiet`（每一刀的输出里都印着「干净」）。
- 🔴 刀具 fail-closed：锚点命中数不等于 1 ⇒ 一个字节都不写并非零退出。
- 🔴 门禁在跑的时候那棵工作树是它的：本轮**每一刀都是「落刀 → 跑完整趟门禁 → 还原」**，中途没有第二个人碰过这棵树。

## 刀具

住址 `evidence/K-R104-cut.py`（本轮实现方自建；**写区外，请 PM 裁**）。
被测对象 = **它自己所在的那棵工作树**（`__file__` 的上一级），不接路径参数 ——
免掉「同一个住址下先后住过两份被测对象不同的量具」那一形（`brief` 12 的 `5k`）。

🔴 **刀具自己在本轮出过一次 bug，如实登记**：第一版对同一份文件的多处编辑各自
`read()` 重算、再逐份 `write` ⇒ **后写覆盖前写**。`U1`（三处编辑同一份 `inbound.rs`）
因此只落地了第 3 处，而刀具照样印「变异已落地」——
**那正是本轮单子逐字禁的那一形**（`K-R87` 的 `cut.sh` 静默跳过）。
处置：① 把那一处已落地的变异**逐处反向掏空**（不整份回滚），`git diff --quiet` 自证干净；
② 刀具改成同一份文件的多处编辑**在同一份内存文本上累加**、锚点对着累加后的文本数；
③ 重跑 `U1`，读数以**重跑那一趟**为准（下面 `7u-a` 那一格）。
⚠ **第一趟 `U1` 的读数作废**，不进本表 —— 它量的是一个我以为没落地的变异。

## `M0` 基线

`GATE: OK —— 13 格全绿`，量于**基点 `19581ba`**（本树，未铺 `embedded-daemons/`）：

| 格 | 读数 |
|---|---|
| cargo | **1598** passed（9 个包合计） |
| daemon | **742** passed（单包） |
| npm | **1722** passed（取最大值 = `test:dom`） |
| e2e | ccm-print-parity **12** · ccm-rbind-title **8** · ccm-cli **46** · ccm-contract-parity **45** |
| 其余 | hooks 11 · fmt · fmt-daemon · winchk · generated · pb check（FAIL=0 BROKEN=0） |

## 收官读数（量于 `track/k-r104` 的 `cbcbcfc`）

`GATE: OK —— 13 格全绿`：cargo **1595**（−3）· daemon **745**（+3）· npm **1722**（±0）·
e2e **12 / 8 / 46 / 45**（±0）· hooks 11 · fmt · fmt-daemon · winchk · generated · pb check FAIL=0 BROKEN=0。

**两个增量各自的分母（不是只报数）**：

- **daemon +3** = 新增三条判据，逐条点名：
  `inbound::tests::every_registered_command_is_reachable_through_the_real_dispatch` ·
  `control::oneshot_session::tests::every_name_this_module_mints_passes_gate2_by_construction` ·
  `control::oneshot_session::tests::the_geometry_rides_on_the_new_session_call_and_only_when_asked`。
- **cargo −3**，🔴 **它不等于「少了 3 条判据」**。逐项：
  `account_usage.rs` **退役 18 条**（其中 2 条是 `#[ignore]`，不计 passed ⇒ 计 16）、
  **新增 12 条**（其中 1 条 `#[ignore]` ⇒ 计 11）⇒ 净 **−5**；
  `inbound_client.rs` **+1**（`the_two_tmux_primitive_arg_builders_match_the_daemon_parsers`）、
  `tmux.rs` **+1**（`the_oneshot_prefix_matches_the_daemon_side`）⇒ **+2**。
  合计 **−3**。✅ 与门禁读数对得上。
  **退役那 18 条逐条点名**（全部是「那条 shell 串」的契约，串没了契约就不存在了）：
  `probe_cmd_uses_dedicated_prefix_and_exact_target` · `probe_cmd_target_tracks_exact_target_and_prefix_keeps_gate1_unreachable` ·
  `probe_cmd_kills_stale_session_before_creating` · `probe_cmd_watchdog_is_setsid_detached_and_posix_only` ·
  `probe_cmd_sends_payload_then_usage_with_quiescence_waits_between` · `probe_cmd_cleans_up_session_after_capture` ·
  `probe_cmd_falls_back_to_no_tmux_sentinel` · `probe_quiescence_requires_the_screen_to_change_before_accepting_stability` ·
  `probe_payload_and_target_are_shell_quoted` · `poll_interval_string_is_derived_not_double_written` ·
  `empty_config_dir_never_produces_a_probe_command` · `the_local_and_remote_probes_use_the_very_same_command` ·
  `the_probe_payload_still_requires_tmux_so_the_windows_branch_is_still_right` ·
  `the_probe_is_a_composition_of_commands_the_daemon_already_has` · `the_orchestration_registry_still_describes_what_this_file_does` ·
  `every_monitor_side_tmux_creation_is_a_registered_d3_exception` ·
  `emit_usage_probe_cmd_for_e2e`〔ignore〕· `e2e_the_local_execution_surface_runs_the_probe_and_returns_output`〔ignore〕。
  ⚠ 其中三条**不是删掉、是翻面重写**：`the_probe_is_a_composition_…` → `…_of_frame_commands_…`（对拍面从 CLI 换成帧面）·
  `the_orchestration_registry_…` → 由 `the_probe_never_falls_back_to_rendering_a_shell_string` ＋ 登记表那一列的「必须是 daemon」接 ·
  `every_monitor_side_tmux_creation_is_a_registered_d3_exception` → `monitor_never_creates_a_tmux_session_of_its_own`（登记制翻成零命中）。

---

# 逐刀读数

每一刀：**落刀 → 跑完整趟门禁 → 记「哪几条判据红了」→ 还原并 `git diff --quiet`**。

## `KR104D1`

### ① 只加 `REGISTRY` 不加镜子 ⇒ 红（刀 `M1`）

变异：`inbound.rs` 的 `COMMANDS` 里拿掉 `"capture-pane"`（`REGISTRY` 不动）。锚点命中 **1** 次。

`GATE: FAIL —— cargo（101）；daemon（101）`，红 **4** 条：

| 树 | 判据 | 是不是正题 |
|---|---|---|
| daemon | `inbound::structure_guards::the_commands_mirror_matches_the_registry` | ★ **正题**（数据对数据） |
| daemon | `build_id_guard::tests::adding_a_subcommand_forces_a_build_id_bump` | 连带（指纹含通道面） |
| daemon | `plugin_walk_fixture::tests::adding_a_plugin_still_costs_the_host_something` | 连带 |
| monitor | `account_usage::tests::the_probe_is_a_composition_of_frame_commands_the_daemon_already_has` | 连带（它现读 `COMMANDS`） |

### ② 两张表都加、`dispatch` 查得到 ⇒ 绿

= 收官那一趟本身（`GATE: OK`）。**阳性对照**：`every_registered_command_is_reachable_through_the_real_dispatch`
里那条反向自检喂一个 `no-such-command-kr104` 进真 `dispatch`，必须被判 `unknown_command`——
不然「10 条都够得到」是空真。

### ③ 只加表不接分派臂 ⇒ 也要红（刀 `M3`）

变异：把 `lookup` 收窄成 `find(|s| s.name == name && s.name != "capture-pane")`
——**表里有、分派到不了**，正是 `K-R102` 记的那一形。锚点命中 **1** 次。

`GATE: FAIL —— fmt-daemon（1）；daemon（101）`，红 **2** 条：

- ★ **正题**：`inbound::tests::every_registered_command_is_reachable_through_the_real_dispatch`（本件新加的**总伞**）
- `inbound::tests::the_dispatch_table_puts_blocking_commands_on_the_blocking_arm`（逐条点名那一份）

⚙ `fmt-daemon` 那一格是变异自己没排版，噪声，不是判据。

⚙ **`K-R102` 顺带验的那一次，结论写清楚**：帧面上这一形**逮得住**，而且**买得起总伞**——
`REGISTRY` 是数据，遍历一遍就是全集，本件那条总伞 8 行。
CLI 面之所以逮不住，是因为它的判据**数字面量**、而 `SUBCOMMANDS` 自己就住在生产段里。
⚠ **别把这条读成「`K-R102` 可以关了」**：那一件问的是 CLI 那 25 条子命令，本件只答了帧面这 10 条。

### 失效方向（件文件点名的那个）：判「源码里出现了那两个命令名」⇒ **恒绿**

现打（`git grep -c`，量于**基点 `19581ba`**，人群 = `remote-daemon-proto/src`）：

- `capture-pane`：**35 处 / 9 份文件**
- `oneshot-session`：**12 处 / 2 份文件**
- 同一基点上 `inbound::COMMANDS` 逐字是
  `"bus-kill", "bus-list", "bus-send", "cancel", "kill", "launch", "ping", "resolve"` —— **两条都没有**。

⇒ 一条「源码里有这两个名字」的判据在**本件动手之前就已经是绿的**，它对帧面一格都证不了。

## `KR104D2`

### ① 编排退回 CLI 面 ⇒ 红（刀 `M5`）

变异：在 `probe_over_frames` 头上插一句 `format!("tmux capture-pane -p -t {}", "=x:")`
（= 生产段又渲染一条抓屏 shell 串）。锚点命中 **1** 次。

`GATE: FAIL —— cargo（101）`，红 **1** 条，正是本件新加的零命中守卫：
`account_usage::tests::the_probe_never_falls_back_to_rendering_a_shell_string`。
⚙ **最小面**：只有这一条红，没有把别的判据一起打红。

### ③ 握手次数有判据在数 ⇒ 红（刀 `M6`）

变异：`probe_over_frames` 里**再拨一次号**（`let again = dial()?`），用第二条通道跑后面几步。
这就是「每一段重新握一次手」那个形状的最小版本。锚点命中 **1** 次。

`GATE: FAIL —— cargo（101）`，红 **1** 条：
`account_usage::tests::the_whole_probe_dials_once_and_talks_many_times`，报文逐字

```
整条编排拨了 2 次号 —— `KR104D2` 要的正是「一条连接上多次往返」。
  left: 2 / right: 1
```

🔴 **「判几次连接」而不是「判花了多久」的证据就在这一格**：同一刀之下，
**往返次数与耗时都没变**（`cmds.len() > 6` 那一格没响、判据里 `interval_ms = 0`），
变的只有**拨号次数**。⇒ 一条按耗时判的判据在这一刀之下**结构上看不见它**。

## `KR104D3`

### ① 表动了而 `BUILD_ID` 没动 ⇒ 红（刀 `M8b`；🔴 `M8` 变异存活，成因见下）

- **`M8`（第一刀，存活）**：只把 `BUILD_ID` 改回 `p2h-oneshot-session`，
  而 `SUBCOMMAND_HISTORY` 里那一行 `p2i-frame-tmux-primitives` **留着**。
  `GATE: OK —— 13 格全绿`。
  🔴 **成因是「变异没生效」，不是「判据瞎」** —— 我切错了刀口：
  `adding_a_subcommand_forces_a_build_id_bump` 的判法逐字是「**当前指纹在不在表里**」
  （它的头注专门写了为什么**不是**「等于最后一行」），而指纹那一行还在表里 ⇒ 它按设计就是绿的。
  ⚙ 顺带：这一刀落在 `build_id_guard` **自己登记过的边界**上 ——
  头注逐字「**有人可以就地改最后一行的指纹而不 bump**。表在同一个文件里，护栏读不到 git 历史」。
  **本刀是那条登记的一个活体**，不是新发现。
- **`M8b`（重切）**：`BUILD_ID` 改回 `p2h` **且** 把 `p2i` 那一行历史用 `#[cfg(any())]` 关掉
  = 「加了两条帧面命令、既没 bump 也没追行」。锚点各命中 **1** 次。
  `GATE: FAIL —— daemon（101）`，红 **1** 条：
  `build_id_guard::tests::adding_a_subcommand_forces_a_build_id_bump`。★ **正题，最小面。**

### ② bump 了 ⇒ 绿

= 收官那一趟（`BUILD_ID = p2i-frame-tmux-primitives` ＋ 历史表追一行）。

### ③ 老 daemon 说得出「我不认得」，不静默失败、不挂住 ⇒ 红（刀 `M9`）

变异：把分流器的两档压成一档（`Routed::NoChannel(why) => ProbeStepError::Refused(why)`）。锚点命中 **1** 次。

`GATE: FAIL —— cargo（101）`，红 **2** 条：

- ★ **正题**：`account_usage::tests::an_old_backend_says_it_does_not_know_the_command`
  —— 它对 `capture-pane` 与 `oneshot-session` **各走一趟真编排**，断三件事：
  ① `captured=false`（不是假装成功）· ② 那句话里有「重装」与命令名与「旧版本」
  · ③ **当场返回**（假通道那一档不 await 任何东西；挂住的话这条会超时红）。
- `account_usage::tests::the_unsupported_case_is_a_bucket_of_its_own`（两档不许合成一档）。

⚙ **这一档的错是走真分流器造的**（`route_call_error(&CallError::Unsupported{..})`），
不是手搓一个长得像的 —— 本条要证的正是「那条 `CallError` 经**那唯一一份**分流规则之后，到用户面前是哪句话」。

## `KR104D4`

### ① 只读白名单多一条 ⇒ 红（刀 `M10`）

变异：`readonly_guard::WRITE_WHITELIST_MODULES` 加第 3 条。锚点命中 **1** 次。

`GATE: FAIL —— daemon（101）`，红 **2** 条：
`readonly_guard::tests::every_registered_write_module_is_really_on_the_tree_and_really_writes` ·
`readonly_guard::tests::daemon_write_capability_is_confined_to_the_registered_modules`。
⇒ **仍只许 2 条**这条性质没被这一刀带坏。

### ② 帧面那条抓屏改成会改 tmux 状态 ⇒ 红（刀 `M11`）

变异：`CAPTURE_SUBCOMMAND` 从 `"capture-pane"` 改成 `"new-session"`。锚点命中 **1** 次。

`GATE: FAIL —— daemon（101）`，红 **4** 条：

- ★ **正题**：`readonly_guard::capture_is_read_only::the_argv_this_site_emits_is_read_only_element_by_element`（**值级**，逐元素）
- ★ **正题**：`readonly_guard::capture_is_read_only::the_site_names_no_state_changing_tmux_verb`（**文本级**）
- `control::capture_pane::tests::the_argv_is_the_read_only_capture_form_in_this_exact_order`
- `control::capture_pane::tests::capturing_a_real_pane_brings_the_screen_back`（**真 tmux**，隔离 socket）

### ③ 轮询长进 daemon ⇒ 红（刀 `M12`）

变异：在 `capture_pane::capture_on` 生产段插一句
`std::thread::sleep(std::time::Duration::from_millis(500));`。锚点命中 **1** 次。

`GATE: FAIL —— daemon（101）`，红 **3** 条：
`no_timer_guard::tests::daemon_production_code_has_no_periodic_wakeups`（★ 正题）·
`no_timer_guard::tests::every_duration_use_is_registered_as_non_timer` ·
`no_timer_guard::g6_reach::the_counterexample_is_still_on_the_board_and_still_passes`。

⚙ 与它成对的正面读数：**轮询仍在调用方** —— `settle()` 那一处 `tokio::time::sleep`
登记在 `rust_timer_registry::REGISTERED` 的 `src/account_usage.rs / wait-for-condition / 1`，
双向断言（多一处红、少一处也红）。

---

# `7u`：把实现整个退掉，还有多少条新断言仍绿

本件的实现**有两半**（daemon 帧面 ＋ monitor 编排），两半各退一次。

## `7u-a`：退掉帧面那一半（刀 `U1`，重跑那一趟）

变异：`COMMANDS` 去掉两条 ＋ 两条 `CommandSpec` 用 `#[cfg(any())]` 关掉。三处锚点各命中 **1** 次。

`GATE: FAIL —— fmt-daemon（1）；cargo（101）；daemon（101）`，红 **4** 条：

| 树 | 判据 |
|---|---|
| monitor | `account_usage::tests::the_probe_is_a_composition_of_frame_commands_the_daemon_already_has` |
| daemon | `inbound::tests::the_dispatch_table_puts_blocking_commands_on_the_blocking_arm` |
| daemon | `fourth_face_tests::the_answer_is_a_function_of_the_machine_not_of_the_build` |
| daemon | `fourth_face_tests::the_windows_answer_is_confirmed_absent_not_unknown` |

**仍绿的新断言，逐条给理由**（都不是仪式，牙在别的刀上）：

| 仍绿的 | 为什么它这一刀该绿 |
|---|---|
| `every_registered_command_is_reachable_through_the_real_dispatch` | 两条从 `REGISTRY` 里整个拿掉了 ⇒ 没有「表里有、够不到」这回事。它的牙在 `M3`。 |
| `the_commands_mirror_matches_the_registry` | 镜子与注册表**同拍**都少了两条 ⇒ 它们仍然一致。它的牙在 `M1`。 |
| `every_name_this_module_mints_passes_gate2_by_construction` | 它判的是**铸名形状**，与「上不上帧面」无关。 |
| `the_geometry_rides_on_the_new_session_call_and_only_when_asked` | 同上，判的是 `new_session_argv` 的 argv。 |
| monitor 那 11 条编排判据 | 这一刀**一个字节都没碰 monitor**；它们的牙在 `7u-b`。 |
| 🔴 `the_two_tmux_primitive_arg_builders_match_the_daemon_parsers` | **这一条是真的没牙**：它从 `inbound.rs` 的**源码文本**里抠 `fields`，而 `#[cfg(any())]` 关掉的条目**文本还在** ⇒ 它看不见。**如实登记为射程边界**（它守的是「字段名两侧同形」，不是「那条命令还在不在」——后者由上表第一行那条守）。 |

## `7u-b`：退掉 monitor 编排那一半（刀 `U2`）

变异：`probe_over_frames` 开头 `return probe_failed("7u：实现退掉了")` ——
整条不拨号、不发一个字节。锚点命中 **1** 次。

`GATE: FAIL —— cargo（101）`，红 **4** 条：

- `account_usage::tests::the_whole_probe_dials_once_and_talks_many_times`
- `account_usage::tests::an_old_backend_says_it_does_not_know_the_command`
- `account_usage::tests::an_empty_screen_is_still_a_successful_capture`
- `account_usage::tests::no_channel_is_an_explicit_failure`

**仍绿的新断言，逐条给理由**：

| 仍绿的 | 为什么 |
|---|---|
| `the_probe_never_falls_back_to_rendering_a_shell_string` | 零命中守卫：退掉实现**不会长出** shell 串。它守的是反方向，牙在 `M5`。 |
| `settling_needs_the_screen_to_change_and_then_hold_still` | 它**直接调 `settle`**，而这一刀只短路了 `probe_over_frames`。⚠ 这是刻意的分工：编排接线由上面那四条守，稳定判据本身由它守。 |
| `the_local_and_remote_probes_are_the_same_code_path` | 源码形态守卫（两个 tauri 命令都经 `probe_account_usage`）——退掉函数体不改这个形态。**如实说：它是约定不是事实。** |
| `the_unsupported_case_is_a_bucket_of_its_own` | 直接喂 `CallError` 给 `route`/`describe`，不经编排。 |
| `the_probe_is_a_composition_of_frame_commands_the_daemon_already_has` | 🔴 **这一条对这一刀是真的没牙**：它查的是「daemon 帧面上有没有那几条 ＋ 登记表那一列是不是 daemon」，编排本体空掉它看不见。它的牙在 `7u-a`。 |
| `monitor_never_creates_a_tmux_session_of_its_own` · `time_budget_ordering_holds` · `probe_payload_is_byte_exact_for_both_account_states` · `slugify_keeps_only_safe_chars` · `bad_config_dir_never_dials_and_never_sends` · `the_two_tmux_primitive_arg_builders_match_the_daemon_parsers` | 各守各的面（起会话面 / 时间预算 / 载荷字节 / slug / 早退 / 字段名），与编排本体不同轴。 |

---

# 新判据的「有没有牙」总账（**分母写清楚，别只报好消息**）

本件新增判据 **16** 条（分母 = 逐个数出来的，不含那条 `#[ignore]` 的 e2e 输入源）：
daemon **3**（`inbound.rs` 1 · `control/oneshot_session.rs` 2）·
`src-tauri/src/account_usage.rs` **11** · `inbound_client.rs` **1** · `tmux.rs` **1**。

**被本轮某一刀实打红过的：8 / 16**，逐条点名（刀号在括号里）：

`every_registered_command_is_reachable_through_the_real_dispatch`（M3）·
`the_probe_is_a_composition_of_frame_commands_the_daemon_already_has`（M1 · 7u-a）·
`the_probe_never_falls_back_to_rendering_a_shell_string`（M5）·
`the_whole_probe_dials_once_and_talks_many_times`（M6 · 7u-b）·
`an_old_backend_says_it_does_not_know_the_command`（M9 · 7u-b）·
`the_unsupported_case_is_a_bucket_of_its_own`（M9）·
`an_empty_screen_is_still_a_successful_capture`（7u-b）·
`no_channel_is_an_explicit_failure`（7u-b）。

🔴 **另外 8 条本轮没有任何一刀打红过 —— 如实登记，别读成「它们有牙」**：

| 没被打红的 | 它守什么 · 它自带什么自检（自检 ≠ 变异验过） |
|---|---|
| `every_name_this_module_mints_passes_gate2_by_construction` | 铸名过不过 Gate 2。**自带反向对照**（没尾巴的形状必须过不了），但没有一刀去动那条尾巴 |
| `the_geometry_rides_on_the_new_session_call_and_only_when_asked` | argv 逐元素 ＋ 三个坏尺寸。判据里就是穷举，但没切过刀 |
| `bad_config_dir_never_dials_and_never_sends` | 坏 configDir 早退 |
| `monitor_never_creates_a_tmux_session_of_its_own` | 零命中守卫。**自带合成样本反向自检**（口径必须认得出一条建会话语句） |
| `settling_needs_the_screen_to_change_and_then_hold_still` | 稳定判据的**行为**（喂一串屏看它第几屏收手），自带一条「画面没变过不许早收手」的反向格 |
| `the_local_and_remote_probes_are_the_same_code_path` | 源码形态守卫。⚠ **它是约定不是事实**，头注已写 |
| `the_two_tmux_primitive_arg_builders_match_the_daemon_parsers` | 字段名两侧同形。`7u-a` 已实测**对「那条命令还在不在」没牙**（见上表 🔴） |
| `the_oneshot_prefix_matches_the_daemon_side` | 前缀两侧逐字比 |

⇒ **这份死值验证明的是那 8 条**；另外 8 条今天只有「自检 + 读一遍」，**没有变异证据**。

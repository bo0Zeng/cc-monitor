# K-R72 死值验留档 —— 送键与杀会话那两条桌面侧回落

量于工作树 `.claude/worktrees/k-r72`（分支 `track/k-r72`，基点 `cba446b`），
量具与门禁一律在沙箱里跑（`K31`）：`PB_WS=backend-consolidation .claude/devbox/gate <工作树> k-r72`。

---

## §1 那把普查尺子 —— 先说清它量的是什么，因为它在两个方向上都不准

PM 派工单给的口径逐字：**按 `#[test]` 切段，段内出现三个符号名之一即算，面 = `src-tauri/src/tmux.rs`**，
读数 **14**（`072d849` 与 `cba446b` 两边同值、集合逐条相同）。

实现方现打复了这一遍，**14 复现得出来**，但同时量了第二把尺子，两把不一样：

| 尺子 | 段怎么切 | `tmux.rs` | `tmux_daemon_gate_guard.rs` |
|---|---|---|---|
| **A**（PM 那把） | 从一行 `#[test]` 到**下一行 `#[test]`** | **14** | 4 |
| **B** | 从**前导 doc/属性块首行**到 `fn` 收尾（头注归**后面**那条判据） | **12** | 3 |

量具住址（本会话 scratchpad，两份都在）：
`…/scratchpad/kr72/census.py`（尺子 A）· `…/scratchpad/kr72/census2.py`（尺子 B）。
被测对象：`.claude/worktrees/k-r72/src-tauri/src/tmux.rs` 与同目录 `tmux_daemon_gate_guard.rs`。

**差在哪，逐条点名**（A 多出来的那两条，是**下一条判据的头注**溢进了上一条的段里）：
- `utf8_client_kou_jing_has_one_home_and_this_side_matches_it`
  —— 它自己一个符号名都没提；命中的是紧跟其后
  `the_prevention_layer_and_the_door_layer_coexist_on_the_same_command` 的头注。
- `the_local_send_keys_never_falls_back_to_ssh`
  —— 同理，命中的是紧跟其后 `the_ssh_fallback_always_probes_before_it_acts` 的头注。
- （`tmux_daemon_gate_guard.rs` 那边同一形：`the_two_command_bodies_are_actually_extracted`
  命中的是 `const GATE_BUILDER` 与下一条的头注。）

### 🔴 而两把尺子**在另一个方向上都漏**

「按符号名普查」这件事本身就量不到那些**靠 `connect_and_exec_cmd` 咬住回落**的判据 ——
它们一个符号名都不提，却必然要改。现打逐条：

| 判据 | 住哪 | 尺子 A | 尺子 B | 实际必须改吗 |
|---|---|---|---|---|
| `the_local_kill_never_falls_back_to_ssh` | `tmux.rs` | ✗ | ✗ | **是**（它 `find_pinned(&body, "connect_and_exec_cmd(")` 硬要求回落在） |
| `the_two_command_bodies_are_actually_extracted` | `gate_guard.rs` | ✓ | ✗ | **是**（自检哨兵是 `connect_and_exec_cmd`） |
| `daemon_kill::the_refusal_wording_matches_the_ssh_path` | `backend/control/daemon_kill.rs` | ✗ | ✗ | **是**（反向锚点断「SSH 那条路里那些串还在」） |
| `daemon_send_keys::the_refusal_wording_matches_the_ssh_path` | `backend/control/daemon_send_keys.rs` | ✗ | ✗ | **是**（同上） |
| `daemon_kill::the_doc_sentence_about_the_transitional_fallback_cannot_outlive_the_code` | `backend/control/daemon_kill.rs` | ✗ | ✗ | **是** —— 它是一条**为这一刻设计的前提触发器**，头注逐字写着「F11 删回落时它会主动红」 |
| `the_shell_gate_expression_agrees_with_the_golden_table` | `tmux.rs` | ✗ | ✗ | **是**（它调 `gate_guard_expr`，而那是「渲染 shell 条件串的执行面」，`KR72D1` 判它随刀走） |

⇒ **口径结论**：14（尺子 A）· 12（尺子 B）· **上界 23**（14 ＋ `gate_guard.rs` 整份 9）
这三个数都是**符号名普查**的读数，而本件真正要处置的人群**既不是它们中的任何一个**：
本件实际动到 **21 条判据**（见 `§3` 的分解），其中 **5 条**任何一把符号名尺子都点不出来。
**这不是订正谁的数，是这类尺子的作用域本来就对不上「删一条实现会牵到谁」这个问题。**

---

## §2 刀前 / 刀后 —— 生产段

### 走掉的（`src-tauri/src/tmux.rs`）

| 符号 | 是什么 | 为什么随刀走 |
|---|---|---|
| `build_guarded_tmux_cmd` | 把 Gate 2 远端半支 + Gate 3 + 动作折成一条原子远端 shell 串 | `K-R54` 表第 5 处逐字：「全部消费者就是 kill 与 send-keys 那两条回落；1、2 删完它自动成为死代码」 |
| `build_kill_session_cmd` | 上者的消费者之一 | `K-R54` 表第 2 处判「留 daemon」 |
| `build_send_keys_remote_cmd` | 上者的消费者之二 | `K-R54` 表第 1 处判「留 daemon」 |
| `gate_guard_expr` | **渲染那条 shell 守卫表达式**（`[ -n "$sid" ] && [ "$w" = "1" ]`） | `KR72D1` 逐字：「连同它那条渲染 shell 条件串的执行面**一起走**」 |
| `PROBE_ONLY_FMT` | 只探存在性那一格的格式串 | 唯一读者是 `build_guarded_tmux_cmd` |
| `kill_remote_tmux` / `tmux_send_keys` 的 SSH 尾巴 | `load_remote_config_by_label` → `connect_and_exec_cmd` → 解析 `NO_TMUX`/`CCM_NO_SESSION`/`CCM_GUARD_*` | 本件正题 |

### 新长出来的（同一文件）

| 符号 | 为什么 |
|---|---|
| `gate1_reject_empty` | Gate 1 的谓词从 `exact_target` 里**分出来**（不是复制）。`exact_target` 产的是给 shell 用的精确串，而两条后端命令今天不拼 shell 串 —— 让它们为一次校验去要一个用不上的串就是「一个值装了两件事」 |
| `no_channel_message` | 「后端通道不在」这一档的**唯一一份用户可见文案**，本机 / 远端两句不同的话。这是 `KR72D1` 那条边界（「把那三处的失败面写成用户看得见的话」）的出口 |

**主堆读数（`K-R54` 那张表第 1 · 2 · 5 处）**：**17 行 ⇒ 0 行**。
口径：`K-R54` 表把这三处记成「同一件事盘上有两份实现」的 17 行；本件删掉的是**第二份**，
今天 monitor 侧对送键 / 杀会话**零行**自己的实现 —— 两条命令各自只剩一个 `match` 三态分流。
⚠ 这个「17 ⇒ 0」量的是**那三处**，不是 `K-R54` 全表 16 处；全表其余 13 处一行未动（`§0d`）。

---

## §3 跟着死的判据 —— 逐条给去向（`KR72D2` 的三类）

### 分解（`KR72D3` 要的那个）

🔴 **这张表第二拍重算过 —— 第一拍那一版是错的**（错在把「跨包搬家」记成了「删 + 新钉」，
于是同一条判据被数了两次，净变化写成 `−9`）。**实测口径**：`cargo` 那一格
（`run_gate_sum cargo 8`，8 包合计）**1551 → 1543，净 −8**；`daemon` 那一格 **694 → 695，+1**。

| 档 | 条数 | 说明 |
|---|---|---|
| ② **性质随实现消失**（删） | **7** | 全在 `tmux.rs`，逐条理由见下表 |
| ① **性质还成立 ⇒ 换住址重钉**（条数不变） | **15** | `tmux.rs` 6（含 1 条原地改名）· `tmux_daemon_gate_guard.rs` 5 · 写区外 3 · **跨包搬家 1** |
| ③ **不是判据**（e2e 夹具产出，`#[ignore]`） | **1** | `emit_guarded_commands_for_e2e` —— 不计入 `passed`，对净数贡献 **0** |
| 新钉 | **0** | 先前记成「新钉 1」的那条是 ① 里的**搬家**，不是新增 |
| 未动 | — | `tmux_daemon_gate_guard.rs` 另 4 条（纯 daemon 面）· `tmux.rs` 其余 21 条 |

⇒ **`cargo` 净 −8 = −7（②）−1（第 8 行搬去 daemon 包）**；`daemon` 的 **+1** 就是那一条落地。
⇒ **两格合起来，真正「没人守了」的是 7 条**，其余每条都有显式去向。
**逐条的刀与读数住件文件 `§3-1` / `§3-3`（八刀全部实打，不是推得的）。**

### ② 性质随实现消失 —— 逐条给理由（**不许只报一个数**）

⚠ **这张表 9 行，而 ② 档的**判据**只有 7 条**（第 1–7 行）。
第 **8** 行 `run_door_with_info` 是**测试台子**、第 **9** 行是 `tmux_daemon_gate_guard`
里一段**派生逻辑**，两者都不是 `#[test]`、都不计入任何一格的 `passed`。
**行数与条数不是同一个数，这里分开写。**

| # | 判据 | 它守的性质 | 那条性质今天为什么不成立了 |
|---|---|---|---|
| 1 | `gate2_non_prefixed_safe_name_builds_remote_check_not_instant_reject` | 非 `cc-*` 但字符安全的名字，**不许在客户端一刀切拒绝**，要构造一条远端 `@ccm_sid` 核验 | 「构造一条远端核验命令」这件事**只发生在那条 SSH 串里**。今天 monitor 不构造任何远端核验；Gate 2 union 的判定本体在 `gate-core`（`gate_singleton_guard` 钉着「全仓只有一份」），执行面在 daemon `control/gate.rs::admit`，而「daemon 侧那道门必须在」由 `tmux_daemon_gate_guard::the_daemon_identity_gate_is_still_there` 钉着、「两侧对同一张金表答得一样」由 `gate2_parity` + `control/gate.rs::the_daemon_side_agrees_with_the_golden_table` 钉着 |
| 2 | `gate3_only_applies_to_kill_not_send_keys` | Gate 3（`windows==1`）**只给破坏性动作**，send-keys 不许有 | 它断的是 `gate_guard_expr` 产的那条 shell 表达式，而那个函数没了。**性质原地由 daemon 侧那条守着**：`the_daemon_now_has_gate3` 的第三格逐字断言「非破坏性 `admit` 里不许出现窗口数判断」——同一条性质，另一个家 |
| 3 | `reject_message_only_reports_fields_actually_gated_on` | 拒绝消息只报**真正参与判断**的字段（send-keys 的拒绝里不许混 `windows=`） | 那条消息是 `build_guarded_tmux_cmd` 里 `reject_msg` 拼的 shell `printf`。daemon 侧的拒绝文案是**另写的**（`admit` 回 `wrong_owner`、`admit_destructive` 回 `too_many_windows`），本来就按档分开，不存在「混进未参与字段」这一形 |
| 4 | `the_prevention_layer_and_the_door_layer_coexist_on_the_same_command` | `K-R12`（`-u` 预防）与 `K-R23`（拆不出字段就拒）**两层在同一条命令串上共存** | 「那条命令串」没了。⚠ **两层各自都还活着，只是不再共处一串**：`-u` 那层由 `the_surviving_cross_ssh_tmux_read_asks_for_a_utf8_client_before_the_subcommand` 守（`ls` 那条）· 下溢判废那层由 `a_dirty_line_underflows_and_an_overflowing_line_is_still_dropped_today` 守（`parse_tmux_ls`）· daemon 本机那次 `display-message` 的同族处置由 `control/gate.rs` 的 `CCM_TMUX_UNPARSABLE` ＋ `the_underflow_predicate_catches_the_real_dirty_bytes` 守 |
| 5 | `a_dirty_channel_that_lost_the_tab_must_not_open_the_door` | **通道脏（TAB 没了）时这道门必须拒绝**，喂脏输入真跑 `/bin/sh` | 「这道门」就是 `build_guarded_tmux_cmd` 拼的那条串。⚠ 那个洞的成因（跨 SSH 通道被改写）在 monitor 侧今天只剩 `ls` 一条读路，而它的处置是 `tmux_tab_underflow`（有判据）。daemon 侧不跨 SSH 读（它在目标机器本地跑 tmux），同族处置仍在 `control/gate.rs` |
| 6 | `send_keys_cmd_construction` | `enter=true` 尾附 ` Enter`、`false` 不附；target/keys 经 `shell_quote` | 断的是那条 shell 串的形状。**`enter` 那一维原地换住址**：`send_keys_now_routes_through_the_daemon` 里那格逐字断言 `enter` 真的传给了 daemon 调用（它有过一次「恒绿被变异复验抓出来」的病史，头注记着）；两个 mode 名由 `both_mode_names_are_ones_the_daemon_actually_parses` 钉。`shell_quote` 那一维在这条路上**不再存在**（不拼 shell） |
| 7 | `the_shell_gate_expression_agrees_with_the_golden_table` | 那条 shell 判定与金表 `gate2-golden.tsv` 逐行一致 | 没有那条 shell 判定了。**金表今天仍有两个读者**（daemon `control/gate.rs::the_daemon_side_agrees_with_the_golden_table` · monitor `backend/control/gate2_parity.rs`），本条当初立的理由逐字就是「那条 shell 表达式谁也没在读金表」—— 表达式没了，理由跟着没了 |
| 8 | `run_door_with_info` | 不是判据，是上面 5/7 共用的**台子**（假 tmux shell 函数 + 真 `/bin/sh`） | 没有调用方了。⚠ 手法本身没丢：`e2e/daemon-gate2-acceptance.sh` 用同一路数（真 daemon + 隔离 `-L` socket）在真机上做同一件事 |
| 9 | 〔`tmux_daemon_gate_guard`〕上一版「走 Gate 的函数集」那段**派生 + 求闭包**逻辑 | 「委托给受托者也算走 Gate」 | 没有受托者了。留一段跑不到的派生代码，就是 `KR72D1` 禁止的那件事的测试侧变体。**判准同拍收紧成更强的一条**（见 ① 第 8 行） |

### ① 性质还成立 ⇒ 换住址重钉（**条数不变，15 条**）

⚠ 第二拍从 10 改到 15：补上了**写区外那 3 条**（下表 11–13 行）与**跨包搬家那 1 条**
（第 14 行 —— 第一拍把它记成「② 删 + 新钉」，那是同一条判据数了两次），
另加第二拍**真加强过**的那一条（第 15 行）。

| # | 判据 | 原来钉在哪 | 今天钉在哪 |
|---|---|---|---|
| 1 | `gate1_rejects_only_empty_target` | 三个构造器的产物 | 谓词本体 `gate1_reject_empty` ＋ `capture-pane` 构造器 ＋ **真跑两条后端命令的生产入口**（断言空目标在任何 IO 之前就地被拒；报「后端通道不在」就说明 Gate 1 跑到 IO 后面去了）。⇒ **人群没缩，从 3 变 4** |
| 2 | `both_cross_ssh_tmux_reads…` → **改名** `the_surviving_cross_ssh_tmux_read_asks_for_a_utf8_client_before_the_subcommand` | 两处跨 SSH 的 tmux 读 | 只剩 `ls` 一处。**名字必须改**：留着「both」就是一句假话。加了反向自检（尺子要分得出「`-u` 在子命令前」与「在后」） |
| 3 | `every_target_placeholder_comes_from_exact_target` | 4 处 `-t {…}`，判准含「委托给受托者也算」 | 1 处（`capture-pane`）。🔴 **分母掉到 1 之后「地板 ≥ N」这种抽取器自检买不到东西** ⇒ 换成**喂一份合成的坏语料**，同一把尺子必须在坏语料上红 |
| 4 | `tmux_targets_use_exact_match` | 三个构造器的产物 ＋ `exact_target` 本体 | 一个构造器 ＋ `exact_target` 本体（后半**一个字没动** —— 那半与有几个构造器无关） |
| 5 | `the_local_kill_never_falls_back_to_ssh` | 「本机早退在 SSH 回落**之前**」（比位置） | **回潮闸**（生产段里再出现 `connect_and_exec_cmd` 就红）＋ **真跑生产入口**看它报的是不是真实原因。⇒ 从「一条纪律」变成「一条结构事实 ＋ 一条行为读数」 |
| 6 | `the_local_send_keys_never_falls_back_to_ssh`（`K-R56` 09-11 买的） | 真跑生产入口，看错误串 | 原来那格**判法一个字没改**，另加两格：回潮闸 ＋ 「本机与远端的话不许一样」 |
| 7 | `the_two_command_bodies_are_actually_extracted` | 自检哨兵 = `connect_and_exec_cmd` | 哨兵换成 `DAEMON_CHANNEL_MARKERS`。🔴 记下这一形：**自检的哨兵长在被测实现上，实现一变自检先假** |
| 8 | `every_remote_tmux_verb_is_either_read_only_or_routed_through_the_gate` | 动词 ∈ 只读表 **或** 所在函数走 Gate | **只读表这一条**（那个「或」的右半没有成员了，而留一个空的或分支给「重新长出受托者再挂破坏性动词」留着合法路）。⇒ **判准变严**：原来允许「走 Gate 的破坏性动词」，今天一个都不允许。另加一格：`GATE_BUILDER` 再出现就红 |
| 9 | `kill_now_routes_through_the_daemon` | 主路走 daemon **且回落必须还在**（`C7`） | 主路走 daemon **且不许再有第二条路**。⚠ **两次翻面方向相反**：先前逐字要求 `connect_and_exec_cmd` **在**，今天逐字要求它**不在** |
| 10 | `send_keys_now_routes_through_the_daemon` | 同上 | 同上（`enter` 那格原样留着） |
| 11 | `daemon_kill::the_refusal_wording_matches_the_ssh_path`（写区外） | 反向锚点断「那条 SSH 回落里这几句话还在」 | **对照面换成兄弟命令** `daemon_send_keys` ⇒ 改名 `…_matches_the_sibling_command`。性质（**同一个拒绝只许有一种说法**）没消失，只是「两条路」今天指的是 kill 与 send-keys 这两条后端命令。⚠ 顺带收紧：对照面过 `production_code`（原来是整份源码 `contains`，对面测试里抄一份就能糊弄） |
| 12 | `daemon_send_keys::the_refusal_wording_matches_the_ssh_path`（写区外） | 同上 | 同上（对照面指回 `daemon_kill`） |
| 13 | `daemon_kill::the_doc_sentence_about_the_transitional_fallback_cannot_outlive_the_code`（写区外） | 「耐久文档里那句『过渡期回落』不许比代码活得久」 | **判据一个字没动** —— 它是为这一刻设计的**前提触发器**，本件让它**真红了一次**，逼着把 `doc/IPC-PROTOCOL.md` 那两处改成「已删」。今天两侧都是 `false`，**两个方向仍然咬** |
| 14 | `the_ssh_fallback_always_probes_before_it_acts`（`K-R56` 09-11 买的） | 「探不到就不动手」 | **跨包搬家** ⇒ `remote-daemon-proto/src/control/gate.rs::both_gates_always_probe_before_they_act`。它是 `cargo` 那格 `−1` 与 `daemon` 那格 `+1` 的**同一条**。⚠ 与 `§0d` 冲突，挂 `〔R72a〕` 交 PM |
| 15 | `a_gate_rejection_is_never_laundered_into_the_ssh_fallback` | **三态不许压成两态** | 名字刻意不改（它记着这条判据当初为什么立）。🔴 **第二拍实打发现第一拍加的那两格是空的** —— 定长窗口换成按 `=>` 切臂、人群从 1 个命令扩到 2 个、另配反向自检。读数与证伪过程住件文件 `§3-1`，请裁记号 `〔R72d〕` |

### ③ 不是判据

`emit_guarded_commands_for_e2e` —— `#[ignore]`，产 e2e 夹具（把真 builder 的生产命令串打到
stdout 喂 `e2e/tmux-guarded-acceptance.sh`）。它的**输入源被删了**，无法改写成不依赖那两条回落的
形式：它要 emit 的东西不存在了。⇒ 走 ③。真机那一面的等价覆盖在
`e2e/daemon-gate2-acceptance.sh`（真 daemon 二进制 + 真 tmux server，用例逐行来自同一张
`gate2-golden.tsv`）。⚠ 它的清理**溢出写区**，见 `§4`。

### ~~新钉的那一条~~ **搬家的那一条**（第二拍订正：它不是新增，是同一条判据换了包）

`remote-daemon-proto/src/control/gate.rs::both_gates_always_probe_before_they_act`
—— **就是** `K-R56` 那条 `the_ssh_fallback_always_probes_before_it_acts`，它守的性质
（**探不到就不动手**）。今天只剩 daemon 一处实现 ⇒ 钉在那里：
`admit` / `admit_destructive` 的生产段里 `probe(target)?` 都在动手之前、探不到那一支回
`no_such_session`、放行交出的是**探回来的句柄**而不是调用方给的名字。
⚠ **约定型守卫**（扫源码形态），行为那一半在 `e2e/daemon-gate2-acceptance.sh`。
⚠ 件文件 `§0d` 写着「不碰 `K-R56` 那条刚买到的判据（要动就交回 PM）」，而派工单 ② 逐字要求
这 5 条「改写成不依赖那两条回落的形式活下来」——**两句话在这一条上是冲突的**，
本拍按派工单做，并挂 `〔R72a〕` 交 PM 裁。

---

## §4 溢出写区的（**第二拍全部改掉了，逐处上报请认**）

🔴 **第一拍在这里写的是「没做，交回 PM 定夺」，第二拍改了这个处置。**
依据是本区自己的先例，不是我自己放宽的：`DECISIONS.md#R24` 裁定四
（`structural_scan::TOMBSTONED` **就在那次被追认的四处里**）· `#R25` 裁定四 ·
`MASTERPLAN` 纪律 ⑯ 逐字「**本条不放宽越界闸**：越了照样当场点名上报；
本条改的是**划写区那一刻**」。⇒ **结构性强制的随动当场改掉并逐处点名**是本区既定做法；
把门禁留成红的交回**不是**。

**12 处逐条住件文件 `§8` 的 `〔R72b〕`**（每处都写了「为什么非改不可」与「改了什么」）。
这里只补一条第一拍没写、而它把「能不能在写区内绕过去」这个问题**关死**了的事实：

> `every_dead_name_named_in_the_prose_is_declared_dead` 的诊断文案**自己逐字禁掉了那条绕法** ——
> 「⚠ **不许**为了让本条变绿就把那句话删掉了事：**删掉的是线索，不是病。**」
> ⇒ 「不在写区内的文件里点名死符号，于是不需要登记」这条路，**是那条判据明文不许走的**。

---

## §5 门禁读数

住件文件 `§3c`（**`GATE: OK —— 13 格全绿`**）。这里只留一条属于**环境**、不属于本件改动的事实：

工作树 `k-r72` 起手时**没有 `node_modules` 那条软链**（`k-r71` / `k-r82` 都有，
指向 `cc-monitor/node_modules`）⇒ 第一趟门禁在 `npm`（`tsx: not found`）与两套 ccm e2e
（`--print: command not found`）上红了三格。**那三格红的不是本件的改动**，补上软链自绿。
⇒ 摘工作树那一步漏了这一条，值得记在派工前的清单里。

## §6 那把「按符号名普查」的尺子，第二拍又逮到它一次

`§1` 说的是它**在两个方向上都不准**。第二拍补一条**它根本量不到**的东西：

**判据的「牙」不在名字里。** `a_gate_rejection_is_never_laundered_into_the_ssh_fallback`
这个名字第一拍照旧在盘上、条数一条没少、门禁全绿 —— 而它当时**一格都没在守**
（两格都被同一刀绕过，人群还只覆盖两个命令里的一个）。
⇒ **只有真切一刀才分得出「判据在」与「判据在守」。** 读数住件文件 `§3-1` 那张对照表。

# `K-R115` 死值验与读数 —— 量具与门禁自己的四笔账

> 量于 **2026-09-14**，工作树 `.claude/worktrees/k-r115`（分支 `track/k-r115`，基点主干 `f6780ea`）。
> 门禁一律走**唯一许可命令**：`PB_WS=backend-consolidation .claude/devbox/gate <工作树> k-r115`
> （沙箱 `ccmon-devbox:latest`，现打 `sha256:85f5e313d4e6`，建于 09-13T22:23）。
> ⚠ 每一个数**量于哪一刻 · 用什么量的**都写在它旁边；带轮次 / 分支尖的句子下一轮自动变成假话。

## §A 门禁逐格读数（`M0` 基线 → `M4` 终态）

| 格 | `M0` 基线（本件动手之前） | `M4` 终态 | 分母怎么数的 |
|---|---|---|---|
| `hooks` | 11 passed | 11 passed | 每个被跟踪的 hook 文件 3 条 ＋ 8 条阳性对照；现打 `hooks/` 下 1 份 ⇒ 11 |
| `copy2` | **不存在这一格** | **11 passed** | `evidence/*.py` 现打 176 份里 `shutil` 保元数据复制族的**调用点** 11 处，逐处判目的地 |
| `fmt` | 1 passed | 1 passed | 绿/红两态，分母 = `src-tauri` 那个 workspace 全体成员 |
| `fmt-daemon` | 1 passed | 1 passed | 绿/红两态，分母 = `remote-daemon-proto` 唯一成员 |
| `winchk` | 1 passed | 1 passed | 绿/红两态，`-p monitor` 一个包 |
| `cargo` | **1610** passed（9 个包合计） | **1611** passed（9 个包合计） | ＋1 = `KR115D4` 新立的 `the_remote_side_column_is_signed_off` |
| `deadcode` | **不存在这一格** | **41 passed** | `cargo check -p monitor` 非 test 构建里 `never used` 的条数，**恒等钉在 41** |
| `generated` | ok | ok | `git diff --quiet -- src/generated/` |
| `daemon` | **761** passed | **762** passed | ＋1 = `KR115D3` 新立的 `the_two_strip_clean_notes_stay_one_sentence` |
| `npm` | 1726 passed | 1726 passed | 取最大值 ⇒ 恒是 `test:dom` 那个数；本件一个 TS 文件都没动 |
| `ccm e2e/ccm-print-parity` | PASS=12（地板 12，恒等） | 同 | — |
| `ccm e2e/ccm-rbind-title` | PASS=8（地板 8，恒等） | 同 | — |
| `ccm e2e/ccm-cli` | PASS=46（地板 46，恒等） | 同 | — |
| `ccm e2e/ccm-contract-parity` | PASS=45（地板 45，恒等） | 同 | — |
| `pb check` | `FAIL=0 BROKEN=0` | `FAIL=0 BROKEN=0` | 共享计划仓 `backend-consolidation` |
| **裁决** | `GATE: OK —— 13 格全绿`（4 分 45 秒） | `GATE: OK —— 15 格全绿`（1 分 24 秒） | 墙钟不可比：`M0` 是冷 target 的第一趟 |

⚠ **两趟的墙钟别相减**：`M0` 跑在冷 `k-r115` target 上（4 分 45 秒），`M4` 跑在同一个已经热了的
target 上（1 分 24 秒）。差的 3 分 21 秒里有多少是 target 冷热、多少是本件加的两格，
**本轮没有拆开量** —— 加一格的代价另有一个**现打**的数，见 `§C`。

## §B `KR115D1` —— 人群现打 · 逐处定性 · 三刀

### B1 人群：`grep` 的数与「调用点」的数不是同一个数

PM 现打给的是 `grep -c copy2 evidence/*.py` = **7 份文件命中**。本轮在同一棵树上复打，两个口径一起给：

- `grep -n copy2 evidence/*.py` ⇒ **10 行**（7 份文件）。
- `ast` 枚举 `shutil` 保元数据复制族（`copy2` · `copytree` · `copystat`）的**调用点**
  ⇒ **12 处**（分母 = `evidence/*.py` 现打 176 份，全部 `ast` 解析得动）。

两个数都不是错的，**它们数的不是一回事**：`grep` 数行（注释行也算，`copytree` 一行都不算），
`ast` 数调用点（`copytree` 收得到，注释收不到）。

### B2 逐处定性（12 处，量具 `evidence/K-R115-ruler.py --census`）

| 处 | 那一跳在做什么 | 裁词 |
|---|---|---|
| `kg3-c1-cuts.py:203` | 把 `scripts/gate.sh` **备份**进 `mkdtemp` | 树外 |
| `kg3-c1-cuts.py:214` | 🔴 把备份**还原回被测树**的 `scripts/gate.sh` | **树内·有跟踪 ⇒ 红** |
| `K-R102-cut.py:149` | 备份进 `<树>/.k-r102-cut-backup/`（那个前缀下 `git ls-files` 一份都没有） | 树内·零跟踪 |
| `K-R112-cut.py:339/340` | `copytree` 把两棵源码树拷进临时台子 | 树外 ×2 |
| `K-R2-kr22-which-side.py:92/106` | 拷源文件进临时夹具仓 | 树外 ×2 |
| `K-R30-etxtbsy-window.py:128` | 拷 `/bin/true` 进 `mkdtemp` 造 ELF 目标 | 树外 |
| `K-R30-inotify-probe.py:172` | 同上 | 树外 |
| `K-R48-native-vs-bash-parity.py:108` | 拷二进制进 `mkdtemp` 的 `bin/` | 树外 |
| `K-W2D-wire-mutations.py:104` | `copytree` 拷整棵树进 `pm-targets` 下的副本（本工作树之外） | 树外 |
| `K-P6-ruler-mutations.py:76` | 拷 10 份 `.rs` 进 `gettempdir()` 下的台子 | 树外 |

⇒ **12 处里只有 1 处是「还原被测源码那一跳」**（`kg3-c1-cuts.py:214`）。本件把它改成
`shutil.copyfile` ＋ `os.utime(GATE, None)`，人群随之 12 → **11 处**，`SITE_FLOOR` 同拍 12 → 11。

### B3 🔴 `K-R102-cut.py` 那 2 处（派工单点名的已知样本 · 纪律 ⑳ 的回测）

**答案：一处是注释，一处是备份方向，它自报的「已改成 `copyfile` ＋ `os.utime`」是真的。**

- `:172` 是一行**注释**（🔴 逐字警告「`copy2` 不许用在这一侧」）—— `grep` 数得到，`ast` 数不到。
- `:149` 是 `do_apply()` 里的**备份**方向（被测树 → `BACKUP/`），不是还原方向。
  它保住的是备份副本的旧 mtime，而 `do_revert()` 用 `copyfile` ＋ `os.utime(..., None)`
  重新推 mtime ⇒ **那份旧 mtime 落不回被测树**，无害。
- 还原那一跳（`:176–179`）今天确实是 `copyfile` ＋ `os.utime`。

⇒ **尺子数得到它，并且判它不红**。这正是本条要的：判的是「还原被测源码那一跳用了它」，
不是「源码里有没有 `copy2` 这个词」。

### B4 三刀

| 刀 | 切在哪 · 锚点命中 | 门禁读数 | 是不是那一格 | 最小面？ |
|---|---|---|---|---|
| `d1-1` 把还原那一跳改回 `copy2` | `evidence/kg3-c1-cuts.py::cmd_oldgate`，锚点（`copyfile` ＋ `utime` 那两行）**命中 1 次** | `GATE: FAIL —— copy2（退出码 1）`；其余 14 格全绿（`cargo 1611` · `daemon 762` · `deadcode 41` 逐格照旧） | ✅ 正是 `copy2` 那一格 | ✅ 只红一格 |
| `d1-2` 阴性对照：`d1-1` ＋ 把门禁那一格整格拿掉 | 同上 ＋ `scripts/gate.sh` 的 `run_gate copy2 '` 锚点**命中 1 次** | `GATE: OK —— 15 格全绿` | ✅ **一条都不红** | — |
| `d1-3` 假红方向：新增一处**正当用途**的 `copy2`（拷一份读数 `.md` 进 `mkdtemp`） | 新建 `evidence/kr115-probe-reading-copy.py`（559 B，中性名） | `GATE: OK —— 15 格全绿`，`copy2` 那一格读数 11 → 12（人群涨了一处，判词「树外」） | ✅ **必须不红，实测不红** | — |

⚠ `d1-2` 的「判据整段拿掉」拿掉的是**门禁那一格**（把 `run_gate copy2 …` 换成 `: skip-cell …`），
不是把尺子文件删掉 —— 删掉尺子的话「判据没了」与「判据绿了」在门禁输出上一模一样。

## §C `KR115D2` —— 选**甲**（收进门禁），代价现打

### C1 代价（**实测，不是估的**）

`deadcode` 那一格自己印墙钟（`gate.sh` 里 `deadcode_t0` 那两行）：

- **冷 target 的第一趟：71 秒**（`M1`，`cargo check -p monitor` 从头编一遍非 test 构建）。
- **热 target 的后续每一趟：2–3 秒**（`M2` 3 秒 · `M3` 3 秒 · `M4` 2 秒）。

⇒ 加这一格的**常态代价是每趟 2–3 秒**；一次性代价是第一趟 71 秒。
分母：同一沙箱、同一 target 目录 `k-r115`、同一棵树。

### C2 读数与钉法：**41 条，恒等**

现打 `never used` **41 条**（`cargo check -p monitor --message-format=short`，非 test）。

🔴 **第一版把它写成「≤ 54」，而那是一道假门**：54 抄自 `K-R109` 09-13 的读数，
今天现打是 41 ⇒ 造一处 `dead_code` 只会到 42，**离 54 还很远，那一刀不红**。
一个宽了 13 的上限，长得和一道门一模一样。⇒ 改成**恒等钉在 41**（多了红、少了也红）。

⚠ **41 与 `K-R109` 的 54 分母不同，别相减**：那一趟是
`touch src/history.rs src/lib.rs && cargo check -p monitor`（默认 message-format），
本格是 `--message-format=short`、不 touch，而且量于另一个主干尖。

### C3 两刀

| 刀 | 切在哪 · 锚点命中 | 读数 | 是不是那一格 | 最小面？ |
|---|---|---|---|---|
| `d2-1` 造一处 `dead_code`（`src-tauri/src/paths.rs` 末尾追加一个没人调的 `fn`，67 B） | 追加式，无文本锚点 | `GATE: FAIL —— deadcode（退出码 1）`；`copy2 11` · `cargo 1611` · `daemon 762` 逐格照旧 | ✅ 正是 `deadcode` 那一格 | ✅ 只红一格 |
| `d2-2` 加了格而**自述格数不跟**（`〔自述·格数〕15 格` → `13 格`），锚点**命中 1 次** | `scripts/gate.sh` 头注自述节 | `evidence/K-R80-gate-cell-coverage.py` rc=1，**红 2 条**：`C5b 自述节自称 13 格 / 现打 15 格` ＋ `C5b 自述节 13 vs 裁决行 15` | ✅ 「同拍改三处，少一处必红」实测成立 | ✅ 只红这两条 |

⚠ `d2-2` 的判官**不是门禁**（那把尺子不在 `gate.sh` 里跑），是 `K-R80` 那把登记的机检。
本刀在**宿主**上跑（纯静态 python，不编译、不起进程）；`d2-1` 在沙箱里跑。

## §D `KR115D3` —— 两个住址一句话

处置：两处的措辞都改成指向 `guard_core::assert_test_module_ranges_are_brace_balanced`，
并把那一段**收成一段带标记的共用段**（`⟦KR115D3 共用段·起/止⟧`，两处逐字相同，现打 **800 B**），
新立判据 `guard_support::tests::the_two_strip_clean_notes_stay_one_sentence` 钉住它们同步。

| 刀 | 切在哪 · 锚点命中 | 门禁读数 | 是不是那一格 | 最小面？ |
|---|---|---|---|---|
| `d3-1` 只改共用段的**其中一处**（`structural_scan.rs` 那一份里去掉两个 `**`），锚点**命中 1 次** | `src-tauri/src/structural_scan.rs` 共用段内一行 | `GATE: FAIL —— daemon（退出码 101）`，红**恰好一条**：`guard_support::tests::the_two_strip_clean_notes_stay_one_sentence`。`copy2 11` · `cargo 1611` · `deadcode 41` 照旧 | ✅ | ✅ 只红一条 |
| `d3-2` 阴性对照：`d3-1` ＋ 把同步判据整段拿掉（79 行） | 同上 ＋ `guard_support.rs` 的 `fn the_two_strip_clean_notes_stay_one_sentence` | `GATE: FAIL —— fmt-daemon（1）；cargo（101）`。🔴 **`daemon` 那一格回绿（762 → 761）** —— 本笔那条判据**不再是红的来源**；剩下的两条红是**别人**：`structural_scan::every_dead_name_named_in_the_prose_is_declared_dead` ＋ `every_symbol_address_in_the_sources_still_resolves`（共用段里点名的那个函数名被刀删掉了 ⇒ 指空），`fmt-daemon` 是刀切掉 79 行留下的排版伤 | ⚠ **半通过** | — |

🔴 **`d3-2` 如实写：它不是「一条都不红」。** 本笔要证的那一半成立
（`daemon` 那一格回绿 ⇒ `d3-1` 的红确实来自本笔那条判据，不是别人顺手接住的），
但**「拿掉之后全绿」不成立** —— 本仓另有两条名字完整性判据钉着这个函数名，删掉它它们当场红。
⇒ 这不是本笔的判据在装样子，是**删一条判据本身在本仓有代价**。别把这一格读成阴性对照通过。

## §E `KR115D4` —— 先量后判

### E1 现打：今天有几行 `Side` 在说假话

派生器（`parity_ledger.rs::command_dispatch_class`，同文件深度 2 的调用闭包 ＋ 列 0 收尾取函数体）
在 **55 行 `Side::Remote`** 上现打：

| 派生类 | 行数 | 意思 |
|---|---|---|
| `RemoteOnly` | **39** | 生产段里有只有远端用得上的东西（SSH / 远端配置 / 拒绝 `<local>`） |
| `FramePlane` | **6** | 只走 origin 无关的帧面 ⇒ `<local>` 也走得通 |
| `Mixed` | **0** | 两样都有 |
| `Unclassified` | **10** | 两样都没有 —— **这把尺子够不着**，不是「它安全」 |

⚠ 深度 2 / 3 / 4 **三个深度答案完全相同**（6 条 `FramePlane` 逐字同一批）⇒ 取最小的那个。

6 条候选逐条人裁之后：**🔴 今天有 5 行 `Side` 在说假话**（不是 `K-R112` 点的那 3 行）：

| 命令 | 裁词 | 现打的理由 |
|---|---|---|
| `capture_remote_pane` | 🔴 说假话 | `K-R112` 09-13 改走帧面 `capture-pane`（`daemon_route.rs::SENDERS` 第七个发送端） |
| `cc_bus_kill` | 🔴 说假话 | `K-R112` 改走 `bus-kill` 原语 |
| `cc_bus_broadcast` | 🔴 说假话 | `K-R112` 逐字「在本件之前就已经腐了」 |
| `kill_remote_tmux` | 🔴 说假话 · **`K-R112` 没点到** | `P3 刀 2`（**08-12**）就改走 `daemon_kill` 了 |
| `tmux_send_keys` | 🔴 说假话 · **`K-R112` 没点到** | 同上，`K-R56`（09-11）还补了本机专属早退 |
| `account_usage` | 不算假话 | 本机那一侧另有命令名 `account_usage_local`（逐字拿 `LOCAL_ORIGIN` 调它）⇒ `usage.per-account` 已是 `{Local, Remote}` 对称 |

🔴 **多出来那两行最狠的地方**：同一行的 `ASYMMETRY_REASONS` 散文**早就逐字写着它们「本机已通」**
（`tmux.manage` 那条理由里的「② `kill_remote_tmux` ⇒ **本机已通**」「③ `tmux_send_keys` ⇒ **本机已通**」，
P3b 08-12 写下、`K-R56` 09-11 复核过）——
**散文说通了，同一行的 `Side` 栏说没通，两边打了一个月的架，而没有任何东西在数它。**

### E2 为什么落**乙**（签字 ＋ 派生的触发器），不落甲（让 `Side` 从源码派生）

三条现打的理由，都写进了 `parity_ledger.rs` 那段头注：

1. **本模块头注自己裁过**：「这项能力在两侧都有吗」是**判断**，不是能从代码推出来的东西；
   同一段还写着**反向那条刻意没做**，实测 11 条合法例外。
2. **反向方向现打就是一堆假阳**：拿同一套标记判「声明 `Local`/`Both` 而其实只走远端」，
   09-14 现打 **7 条命中、7 条全是假阳**（`read_cc_bus_state` · `read_cc_bus_inbox` ·
   `list_local_tmux` · `daemon_start` · `load_subagent` · `read_skill_file` · `write_skill_file`
   —— 它们**两条路都有**，标记只看得见远端那条）。
3. **正向也有一条假阳**：`account_usage`（见上表）。

### E3 🔴 那 5 行**本件刻意不翻**，理由两条

① **连锁现打是 4 处**：`EXPECTED_LOCAL_OR_BOTH`（93 → 98）· `ORIGIN_TAKING_BOTH`（＋5 条，
它们都吃 `origin:`）· `tmux.manage` 那条 `ASYMMETRY_REASONS` 的散文 ·
钉着那句散文的 `the_tmux_manage_row_stops_waiting_for_a_daemon_primitive`
（它逐字断言那句理由里**必须**含 `Side::Remote`）⇒ **翻 `Side` 要同拍改掉一条现行判据的断言**。
这一处**实测过**：`d4-1` 把 `capture_remote_pane` 改成 `Both`，
`local_or_both_commands_take_no_remote_only_parameter` 当场红，逐字
「`capture_remote_pane` 在对账表里声明为 Both，签名里却有远端专用参数 `origin:`」。

② **「本机真的抓得到一屏 / 真的杀得掉」今天判不了** —— 一趟真机都没跑过（`K31` 之下），
`K-R112` 交回时逐字承认过同一条。`Side::Both` 的字面是「这条命令自己就把两侧都办了」，
在没跑过的前提下写上去，是拿一句没验过的话换一格好看的表。

⇒ **登记成欠账**（`FRAME_PLANE_VERDICTS` 里 5 条 `LiesTodayOwedACorrection`），
本笔保证的是**它从此不会静默**。

### E4 四刀

| 刀 | 切在哪 · 锚点命中 | 门禁读数 | 是不是那一格 | 最小面？ |
|---|---|---|---|---|
| `d4-1` 把 `capture_remote_pane` 那行 `Side::Remote` → `Both`（候选那一档），锚点**命中 1 次** | `parity_ledger.rs::LEDGER` | `GATE: FAIL —— cargo（101）`，红 **2 条**：`the_remote_side_column_is_signed_off`（本笔）＋ `local_or_both_commands_take_no_remote_only_parameter`（**本仓早就有的**） | ⚠ 是那一格，但**不只有它** | ❌ 粗刀 |
| `d4-1b` 换 `list_remote_tmux`（`RemoteOnly` 那一档）→ `Both`，锚点**命中 1 次** | 同上 | 同 `d4-1`：红同样那 2 条 | ⚠ 同上 | ❌ |
| `d4-2` 阴性对照：`d4-1` ＋ 本笔判据整段拿掉（139 行） | 同上 ＋ `fn the_remote_side_column_is_signed_off` | `GATE: FAIL —— fmt（1）；cargo（101）`，cargo 只剩 `local_or_both_commands_take_no_remote_only_parameter` | 🔴 **阴性对照不通过：拿掉本笔判据，那一刀照样红** | — |
| `d4-3` 🔴 **派发跳换档而 `Side` 一个字不动**（`hooks_diag.rs::diagnose_remote_cc_bus_hooks` 里加一句 `client_for(&origin)` ⇒ 它从 `RemoteOnly` 变 `Mixed`），锚点**命中 1 次** | `src-tauri/src/hooks_diag.rs` | `GATE: FAIL —— cargo（101）`，红**恰好一条**：`the_remote_side_column_is_signed_off` | ✅ **本笔独有的那颗牙** | ✅ 只红一条 |
| `d4-4` 阴性对照：`d4-3` ＋ 本笔判据整段拿掉 | 同上 ＋ 那 139 行 | `GATE: FAIL —— fmt（1）`，**`cargo` 回绿** | ✅ 阴性对照成立（`fmt` 是刀删 139 行留下的排版伤，不是判据） | — |

🔴 **`d4-1`/`d4-2` 这一对如实写清它证明了什么**：
它们证明的是「**把 `Side` 改成假的会红**」——**而那颗牙本仓早就有一半**
（`local_or_both_commands_take_no_remote_only_parameter`：声明 `Local`/`Both` 的命令不许吃
远端专用参数 ＋ `EXPECTED_LOCAL_OR_BOTH` 恒等 93）。
**本笔真正补上的是另一半**：`Remote` 那一栏的**派发跳变了而 `Side` 没跟** ——
那一形今天之前**一个字都没人数**，`d4-3`/`d4-4` 是它的正反两刀。

## §F 把实现整个退掉，还有多少条新断言仍绿

逐处退（就是上面那几把阴性对照刀），逐条给读数：

| 退掉的实现 | 本件哪条新断言仍绿 | 为什么仍绿 |
|---|---|---|
| 门禁 `copy2` 那一格（`d1-2`） | 无 —— `KR115D1` 只有那一格，退掉之后**它就不判了**（`GATE: OK`） | 判据整段拿掉 ⇒ 零残留，这是它该有的样子 |
| `KR115D3` 同步判据（`d3-2`） | 无本件的断言仍绿；但**别人的两条**接住了删除（死名 ＋ 符号地址） | 那两条钉的是「函数名指不指得到」，不是「两处同不同步」——**它们买不到 `d3-1` 那一刀** |
| `KR115D4` 判据（`d4-2` / `d4-4`） | `d4-2` 里 `local_or_both_commands_take_no_remote_only_parameter` 仍红 | 它守的是**参数面**（`Both` 不许吃 `origin:`），与本笔的**派发面**是两条不同的牙；`d4-4` 那一刀它就够不着了 |
| `deadcode` 那一格 | 未单独退 —— 它就是一格，退掉即不判 | — |

## §G 改动面

### G1 Python（`ast` 逐函数 md5）

| 文件 | 函数数 | 变了的函数 | 整份 md5 |
|---|---|---|---|
| `evidence/kg3-c1-cuts.py` | 11 → 11 | **只有 `cmd_oldgate`**：`94b063907b` → `d0913900f0`（另加一行模块级 `import os`） | `91e8f447a3` → `acd09a6c4f` |
| `evidence/K-R80-gate-cell-coverage.py` | 16 → 16 | **只有 `main`**：`c89b025094` → `6a08c52275`（`order` 加两格）；其余是模块级 `cell(...)` 与 `NO_GATE_NEEDED` | `60abf4b13d` → `4120029795` |

⇒ `kg3-c1-cuts.py` 那份**逐函数证明了「只改还原那一跳，别的量具行为一个字节没动」**
（写区那句「只许改还原那一跳，别顺手重构量具」）。

### G2 全部改动（`git diff --stat`，量于交回前）

```
 evidence/K-R80-gate-cell-coverage.py     |  92 ++++--     ← 🔴 写区外
 evidence/kg3-c1-cuts.py                  |   9 +-
 remote-daemon-proto/src/guard_support.rs | 100 ++++++-
 scripts/gate.sh                          |  71 ++++-
 src-tauri/src/parity_ledger.rs           | 472 +++++++++++++++++++++++++++++++
 src-tauri/src/structural_scan.rs         |  21 +-
 6 files changed, 729 insertions(+), 36 deletions(-)
```

新建：`evidence/K-R115-ruler.py` · `evidence/K-R115-cut.py` · 本文件。

## §H 🔴 写区外 —— 一份文件，四处，我改了但没自批

`evidence/K-R80-gate-cell-coverage.py`（门禁分格覆盖的登记 ＋ 它的机检）。
**单独放在一个可整块 `revert` 的提交上**，提交信息第一行逐字带「写区外」。

1. **加两条 `cell(...)` 登记**（`copy2` · `deadcode`）＋ `main()` 里 `order` 加两格。
   —— **结构性随动**：不加，那把尺子的 `C1` 当场红（「`gate.sh` 里有格没登记」）。
2. **`cargo` 那条锚点 `run_gate_sum cargo 8 bash -c` → `cargo 9`** ＋ 覆盖理由里「8 个成员」→「9 个」。
   🔴 **这是本件之前就红着的一条存量**，不是本件弄红的：现打于 `HEAD`（`f6780ea`）的原版尺子
   `K_R80_ROOT=<工作树> python3 <HEAD 版尺子> <HEAD 版 gate.sh>` ⇒ **rc=1，红 1 条**，
   逐字「`C2` `cargo` 的逐字锚点在 `gate.sh` 里命中 0 次」。
   （workspace 长到 9 个成员那天没人回来改这条锚点。）
3. 🔴 **删掉 `NO_GATE_NEEDED["evidence/"]` 整段（1347 B）—— 这一条不是机械随动，是推翻了一条理由。**
   那段话逐字写着「`0 格覆盖`**正是它的用途，不是它的缺陷**」，理由是 `[J3 陈账]` 死锁的**泄压口**。
   本件的 `copy2` 格是**第一格盖到 `evidence/`** 的门 ⇒ 那个前提当场不成立，
   `C6b` 会说「这条说明陈了，回来删」。
   ⚠ **泄压口在实质上还在**：`copy2` 那一格只在两种情况下红（某处保元数据复制的目的地落在
   被 git 跟踪的树内内容上 · 某份 `.py` 连 `ast` 都解析不了），改一份 `.md` / 加一份读数一格都不动。
   **但那句话的字面不成立了** ⇒ 删它，不许改成一句还罩得住的话。
   🔴 **实现方不自批 —— 交回 PM 裁。** 退掉这个提交，那把尺子会红在 `C1`（两格没登记）
   与 `C2`（`cargo` 锚点）上；`C6b` 那条会回到「`evidence/` 0 格覆盖」的旧账。

4. **死值验用的刀碰过两份写区外文件，但一个字节都没留下**：
   `src-tauri/src/paths.rs`（`d2-1`）· `src-tauri/src/hooks_diag.rs`（`d4-3`/`d4-4`）。
   两份都由 `evidence/K-R115-cut.py --revert` **重写原文**还原（不是 `copy2`，mtime 已推到现在），
   交回时 `git status` 里它们**不出现**。

## §I 诚实边界（写死，别把绿读宽）

1. **`copy2` 那一格只看 `evidence/` 下的 `.py`**，只看 `shutil` 那三个名字。
   `subprocess` 里的 `cp -a` / `cp -p`、`tarfile.extractall`、手写 `os.utime(dst, 旧时间)`
   —— **一概看不见**。现打：`evidence/*.py` 里字面 `cp -a` 出现在容器 / 临时目录的 shell 串里，
   目的地是容器路径，本尺子解析不了那种串。**这是已知的漏，不是「没有」。**
2. **`copy2` 判的是落点，不是意图**：把备份目录建成一个 git 跟踪着的目录 ⇒ 误红；
   还原进一个**尚未被跟踪**的新文件 ⇒ 漏。
3. **`copy2` 不证明「这把量具的还原真的让 cargo 重编了」** —— 那要跑一趟。
   它只证明**没人再用「连 mtime 一起搬回去」那一跳**。
4. **`deadcode` 射程只有 `-p monitor` 一个包的生产段**。`remote-daemon-proto` 那棵树的
   `dead_code` **今天仍然没有格**；`#[cfg(test)]` 里的死代码任何非 test 构建都看不见。
   本格**不修**任何一条，只是从此有人在数。
5. **`KR115D3` 不判那句话对不对** —— 两处一起改成同一句假话，它照样绿；
   人群写死是**两处**，第三处地方再抄一遍同一句话它看不见。
6. **`KR115D4` 射程只有 `Side::Remote` 那一栏**（现打 55 行）。`Local` ↔ `Both` 之间改来改去
   本条看不见（那个方向的派生现打 7 条全假阳）。直方图**拦不住**同一拍里一进一出、
   且恰好同一档的那一形；候选那一档另有逐条点名的人裁表在对拍，不吃这个亏。
7. **`KR115D4` 的派生器只跟同一份文件里的调用，深度 2**。跨文件的 helper 看不见 ⇒
   落 `Unclassified`（今天 10 行）。**`Unclassified` 不是「安全」，是「这把尺子够不着」。**
8. **本件一趟真机都没跑**（`K31`）：那 5 行「说假话」说的是**这一跳走哪条路**，
   不是「本机真的抓得到一屏」。后者今天**判不了**。
9. **`d3-2` 与 `d4-2` 两个阴性对照没有做到「一条都不红」**，逐条读数与原因见 `§D` / `§E4`。
10. **本件在宿主上跑过纯静态 python**（人群普查 · `K-R115-ruler.py` · `K-R80` 那把尺子 · `d2-2` 那一刀）。
    宿主上**没有跑过 `cargo` / `npm` / 任何构建或测试**；一切编译面的读数都出自沙箱里的
    唯一许可命令。这一条如实登记，不自批。

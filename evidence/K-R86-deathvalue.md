# K-R86 死值验原文

本件的三条 dod 各自的刀、逐刀的**完整原文读数**住这里；件文件 `§3` 只放摘要。

## 规矩（每一刀都要满足，缺一条这份证据不算数）

- **全部在沙箱里跑**：`PB_WS=backend-consolidation .claude/devbox/gate <本工作树绝对路径> <tag>`。
  **不许直接在本机跑**（`K31` / `R15`）。
- **一趟一刀**：下一刀之前先把上一刀还原干净，`git status --porcelain` 贴出来自证盘上没留变异。
- 🔴 **还原不许用 `cp -a`** —— 它连 mtime 一起还原，`cargo` 会判「没变」而沿用上一刀的产物，
  于是你会读到**上一刀的红**。`K-R88` 那轮真踩了。用 `cp` ＋ `touch`。
- **点名**：每一刀要写清「**哪一条判据红了**」，不是只写「红了」。
  阴性对照（本该绿的那一刀）同样要贴读数。

## 量具住址（本轮的，只属于本件）

- 刀本体：`/tmp/claude-1000/-home-zbl----claudecode-frontend/f80a7aea-0ccf-4854-9b7a-12961a9c55aa/scratchpad/kr86/`
  下的 `cut.sh`（跑一趟：备份 → 变异 → 打印「变异已落地」的 diff → `touch` → 沙箱门禁 →
  `cp` 还原 ＋ `touch` → 打 `git status --porcelain`）与 `mut_M*.py`（逐刀一份，
  每份开头 `assert s.count(old)==1` = **锚点恰好命中 1 次**，命中不到 1 次脚本直接失败、刀不落地）。
  ⚠ 那个目录是**会话私有**的临时目录，不随仓走；**被测对象恒是**
  `/home/zbl/文档/claudecode-frontend/.claude/worktrees/k-r86`（`track/k-r86`）。
- 门禁：`PB_WS=backend-consolidation .claude/devbox/gate <上面那棵树> k-r86`，
  target 目录 `.claude/pm-targets/k-r86`。

## `M0` 基线

**`GATE: OK —— 13 格全绿`**，量于 **`28181aa`**（`track/k-r86` 的基点，本轮一行代码都还没改；
只铺了 `node_modules` 软链 —— 见下面那条）。

| 格 | 读数 |
|---|---|
| hooks | 11 passed |
| fmt | 1 passed（绿/红两态） |
| fmt-daemon | 1 passed（绿/红两态） |
| winchk | 1 passed（绿/红两态） |
| cargo | **1594 passed**（9 个包合计） |
| generated | 与 Rust 源一致 |
| daemon | **714 passed**（单包 `remote-daemon-proto`） |
| npm | **1714 passed** |
| ccm e2e ×4 | print-parity **12** · rbind-title **8** · cli **46** · contract-parity **45** |
| pb check | FAIL=0 BROKEN=0 |

⚠ **两条要连着读的分母**：

1. **本树未铺 `src-tauri/embedded-daemons/`** ⇒ `embedded_daemons` cfg 不置 ⇒ 上面那个
   cargo 合计里**少了「本地后端真的能起来吗」那一族（4 条）**。门禁自己每趟都印这一行。
2. 🔴 **第一趟 M0 是红的，而那与本件无关**：`GATE: FAIL —— npm（退出码 127）；
   ccm e2e/ccm-print-parity（退出码 1；实得 PASS=0，地板 12）`。根因是这棵工作树**没有
   `node_modules`**（`sh: 1: tsx: not found`；`ccm-print-parity` 那套要 `npx tsx` 现渲一遍
   生产命令行，拿不到就整套 0）。处置：照别的工作树的成例补一条软链
   `node_modules -> /home/zbl/文档/claudecode-frontend/cc-monitor/node_modules`
   （`.gitignore:10` 盖着它，**不进版本控制、不是写区项**）。补完重打就是上表。
   **这条如实记**：`M0` 不是一趟打出来的。

## 逐刀

〔往下写〕

### `M1` · `KR86D1` 刀①：**把原语从分派臂摘掉**（⇒ 该红）

- 刀：`mut_M1.py`，锚点 = `main.rs` 里那一行分派臂，**命中恰好 1 次**（脚本 `assert` 住）。
  变异已落地的 diff（原文）：`1375d1374 < Some("--capture-pane") => control::capture_pane::run(&args),`
- 读数：**`GATE: FAIL —— daemon（退出码 101）`**；daemon **727 passed / 1 failed**（基线 728/0）。
- **红的是哪一条**（点名）：`stream_flag_tests::the_capture_pane_subcommand_is_actually_reachable`，
  报错逐字：「一次性查询的分派里没有那条把 `--capture-pane` 交给原语本体的臂：
  没有任何一行 trim 之后等于 `Some("--capture-pane") => control::capture_pane::run(&args),`。
  包含它但**不等于**它的行（这正是「事实被撑大了」的样子）：`[]`」
- 其余 12 格全绿（cargo 1595 · npm 1714 · e2e 12/8/46/45 · pb check FAIL=0）。
- 🔴 **顺带逮到一条既有判据的盲区，值得单独记**：`argv_table_guard::every_listed_subcommand_is_actually_dispatched`
  （名字里写着「真的被分派」）**没有跟着红**。原因：它的人群来自
  `protocol_doc_guard::dispatched_subcommands()` —— 那是「`DISPATCH_FILES` 的生产段里出现过的
  `--token` **字面量**」，而 `SUBCOMMANDS` 那张表**自己就住在 `main.rs` 的生产段里**
  ⇒ 臂删了、表还在 ⇒ 字面量还在 ⇒ 它照常绿。
  **这正是件文件点名的那个失效方向的另一张脸**（「判源码里有没有那个字面量」）。
  ⚠ 本轮**没有**去改那条既有判据（不在写区，且改它要重判它的整个人群）—— 如实登记，进 `§8`。

### `M2` · `KR86D1` 刀②：**在 ⇒ 绿**（阴性对照）

见本文件末尾的「收官那一趟」：同一棵树、同一条门禁命令，**`GATE: OK —— 13 格全绿`**。
⚠ 阴性对照与上面每一刀共用同一个 target 目录（`.claude/pm-targets/k-r86`），
每一刀还原都用 `cp` ＋ `touch`（**不是 `cp -a`**）⇒ 下一趟 `cargo` 真的重编那一份，
不会沿用上一刀的产物（`K-R88` 那轮踩过）。

### `M3` · `KR86D1` 刀③-a：**把它改成会改 tmux 状态的形状（`send-keys`）**（⇒ 该红）

- 刀：`mut_M3.py`，锚点 = `CAPTURE_SUBCOMMAND` 那一行常量声明，**命中恰好 1 次**。
  `capture-pane` → `send-keys`。
- 读数：**`GATE: FAIL —— daemon（退出码 101）`**；daemon **724 passed / 4 failed**。
- **红的是哪四条**（点名）：
  1. 🔴 `readonly_guard::capture_is_read_only::the_argv_this_site_emits_is_read_only_element_by_element`
     —— **件文件 `KR86D1` 刀③ 逐字要求的「`readonly_guard` 必须红」在这一条上兑现**；
  2. 🔴 `readonly_guard::capture_is_read_only::the_site_names_no_state_changing_tmux_verb`
     —— 报错逐字：「抓屏那一处的生产段里出现了会改 tmux 状态的动词：`["send-keys"]`」；
  3. `control::capture_pane::tests::the_argv_is_the_read_only_capture_form_in_this_exact_order`；
  4. `control::capture_pane::tests::capturing_a_real_pane_brings_the_screen_back` ——
     **真 tmux 打回来的原话**：`command send-keys: unknown flag -p`。
- ⚠ **`readonly_guard::spawn_registry` 那两条一条都没红** —— 那不是缺陷，是它自陈的边界：
  `ALLOWED` 的键是 `(文件, 起什么程序)`，**分不出被调的是哪条 tmux 子命令**
  （`plugin/invoke.rs` 那条豁免理由里逐字写着这个 `K6b` 盲区）。
  **本件立 `capture_is_read_only` 的全部理由就是这一格**，这一刀是它的活体证明。

### `M4` · `KR86D1` 刀③-b：**顺手写一个文件**（⇒ 该红）

- 刀：`mut_M4.py`，锚点 = `spawn_capture` 里那两行，**命中恰好 1 次**；
  插入 `let _ = std::fs::write("/tmp/kr86-mutation-probe", target);`。
- 读数：**`GATE: FAIL —— daemon（退出码 101）`**；daemon **725 passed / 3 failed**。
- **红的是哪三条**（点名）：
  1. 🔴 `readonly_guard::tests::daemon_write_capability_is_confined_to_the_registered_modules`；
  2. 🔴 `readonly_guard::tests::every_fs_call_in_daemon_production_is_read_only`；
  3. `platform::cfgless_guard::tests::platform_assumptions_outside_a_gate_are_each_signed_for`
     （那个 `"/tmp/` 字面量落在它的 `posix-abs-tmp-sub` 信号上 —— **顺带命中，不是本刀的正题**）。
- ⇒ 「写文件」这一半由**既有的** `readonly_guard` 默认层与白名单层接住，本件一个字都不用加。

### `M5` · `KR86D2` 刀①：**会话不存在时回空串 / 假成功**（⇒ 该红）

- 刀：`mut_M5.py`，锚点 = `classify` 里 `no_such_session` 那一支，**命中恰好 1 次**；
  整支换成 `return Ok(String::new());`。
- 读数：**`GATE: FAIL —— daemon（退出码 101）`**；daemon **724 passed / 4 failed**。
- **红的是哪四条**（点名）：
  1. `control::capture_pane::tests::an_empty_screen_is_a_success_and_a_failure_is_never_an_empty_string`；
  2. `control::capture_pane::tests::every_registered_needle_lands_in_its_own_bucket`；
  3. `control::capture_pane::tests::missing_tmux_and_missing_session_are_two_different_answers`；
  4. `control::capture_pane::tests::capturing_a_real_pane_brings_the_screen_back`
     —— 它里面那条**反向对照**（真 server 上问一个不存在的会话）当场逮到。
- ⚠ **这一趟 `pb check` 也红了 1 条，而它与本刀无关**，如实记：
  `FAIL [J3 陈账] INDEX.md 比源文件旧 —— 重跑 pb index 落盘`。
  成因是**计划仓那边 PM 正在同拍写东西**（同一趟的【派工登记读数】现打「在跑清单 K-R72 K-R86」）；
  刀后现打 `find . -name '*.md' -newer INDEX.md` ⇒ **0 份**（PM 已重跑 `pb index`）。
  ⇒ 那是一个**跨仓的时间窗**，不是本刀的读数；`pb index` 是 PM 的落点，实现方不许在计划仓跑生成命令。

### `M6` · `KR86D2` 刀③：**把「没 tmux」与「会话不存在」压成同一句**（⇒ 该红）

- 刀：`mut_M6.py`，锚点 = `tmux_unavailable` 的返回，**命中恰好 1 次**；
  码换成 `no_such_session`、话换成与那一档**逐字相同**的一句。
- 读数：**`GATE: FAIL —— daemon（退出码 101）`**；daemon **727 passed / 1 failed** ——
  **最小面：只红了该红的那一条**。
- **红的是哪一条**（点名）：`control::capture_pane::tests::missing_tmux_and_missing_session_are_two_different_answers`，
  报错逐字：「`assertion left != right failed`：「这台机没有 tmux」与「会话不存在」共用了同一个码
  `no_such_session` —— 压成一句之后，拿到这条错的人没法判断该去装 tmux 还是该去看会话名」。
- ⚠ 它同时断了**两样**（码不同 ∧ 话不同）：只改码会红在第一条断言，只改话会红在第二条。

### `M7` · `KR86D3` 刀①：**在生产段里长出轮询**（⇒ 该红）

- 刀：`mut_M7.py`，锚点 = `capture_on` 的头两行，**命中恰好 1 次**；
  插入一个 `for _ in 0..6 { std::thread::sleep(Duration::from_secs(1)); … }` 的「隔 1 秒再抓一次」。
- 读数：**`GATE: FAIL —— daemon（退出码 101）`**；daemon **724 passed / 4 failed**。
- **红的是哪四条**（点名）：
  1. 🔴 `no_timer_guard::tests::daemon_production_code_has_no_periodic_wakeups`
     —— **件文件 `KR86D3` 刀① 逐字要求的「`no_timer_guard` 红」在这一条上兑现**；
  2. 🔴 `no_timer_guard::tests::every_duration_use_is_registered_as_non_timer`（`Duration::from_*` 的相等断言）；
  3. `no_timer_guard::g6_reach::the_counterexample_is_still_on_the_board_and_still_passes`；
  4. `readonly_guard::capture_is_read_only::the_capture_site_is_one_shot`（本件新立的那条）。
- ⇒ `KR86D3` 刀② 的「只出一条抓一次的命令 ⇒ 绿」= 收官那一趟。

### `7u` · **把实现整个退掉，还有多少条新断言仍绿**

- 刀：`mut_7u.py`，**逐处掏空、签名留着**（🔴 不是 `git checkout <base> -- <file>` 整份回滚），
  四处、每处锚点各**命中恰好 1 次**：
  ① `capture_argv` 的返回退成 `["", "", "", "", ""]`（形状仍是 `[&str; 5]`）；
  ② `classify` 恒答「成功、空屏」那一张脸；
  ③ `tmux_unavailable` 的话退成空串；
  ④ `capture_on` 不再走形状校验 / 不再拼精确目标 / 不再判，恒 `Ok(String::new())`。
- 读数：daemon **719 passed / 9 failed** ⇒ 本件新增的 **14** 条断言里 **9 条红 · 5 条仍绿**。
  （新增条数的分母：收官那一趟 daemon `728` − 基线 `714` = **14**。）
- **仍绿的 5 条，逐条给理由 —— 每一条的牙都在别的刀上，已各自实测过**：

| 仍绿的那一条 | 为什么这一刀咬不到它 | 它的牙在哪一刀（已实测） |
|---|---|---|
| `readonly_guard::capture_is_read_only::the_site_is_really_being_read` | 它是**抽取器自检**：只断「剥完还剩真代码 ＋ 那一处起进程还在」。掏空不动这两样，本来就该绿 | 文件搬家 / 剥法坏掉那一形（`M1` 同族：`M1` 删臂时它绿，正说明它不越界） |
| `…::the_site_names_no_state_changing_tmux_verb` | 掏空只**减**东西，不会长出改状态的动词 | **`M3`**（`["send-keys"]` 当场红） |
| `…::the_verb_scan_actually_bites_on_every_needle` | 它跑在**合成样本**上，与实现无关（它是上一条的反向自检） | 把某一条针拼错 ⇒ 它自己红（结构上必然，本轮未单切） |
| `…::the_capture_site_is_one_shot` | 掏空后仍是「恰好一处起进程 · 零循环」 | **`M7`**（长出 `for` + `sleep` 当场红） |
| `stream_flag_tests::the_capture_pane_subcommand_is_actually_reachable` | 它断的是 `main.rs` 的闸门与分派臂，掏空只动 `control/capture_pane.rs` | **`M1`**（删臂当场红） |

⇒ **真空真 0 条**：五条各自都有一刀实测过它会红（第三条那一格是结构上的自指，如实标注「本轮未单切」）。

## 收官那一趟（阴性对照 ＝ `M2` / `KR86D2` 刀② / `KR86D3` 刀②）

**`GATE: OK —— 13 格全绿`**（hooks · fmt · fmt-daemon · winchk · cargo · generated · daemon ·
npm · 四套 ccm e2e · pb check）。

| 格 | `M0` 基线（`28181aa`） | 收官 | 差 |
|---|---|---|---|
| hooks | 11 | 11 | 0 |
| fmt / fmt-daemon / winchk | 1 / 1 / 1 | 1 / 1 / 1 | 0 |
| cargo | 1594 | **1595** | **+1** |
| generated | 一致 | 一致 | — |
| daemon | 714 | **728** | **+14** |
| npm | 1714 | 1714 | 0 |
| ccm e2e ×4 | 12 / 8 / 46 / 45 | 12 / 8 / 46 / 45 | 0 |
| pb check | FAIL=0 BROKEN=0 | FAIL=0 BROKEN=0 | — |

**新增 15 条断言，逐条点名**（`+1` 在 monitor 那棵、`+14` 在 daemon 那棵）：

- monitor（`src-tauri/src/parity_ledger.rs`，`+1`）：
  `the_tmux_manage_row_stops_waiting_for_a_daemon_primitive`
- daemon `control/capture_pane.rs::tests`（`+8`）：
  `capturing_a_real_pane_brings_the_screen_back` ·
  `a_socket_with_no_server_says_so_in_its_own_words` ·
  `missing_tmux_and_missing_session_are_two_different_answers` ·
  `an_empty_screen_is_a_success_and_a_failure_is_never_an_empty_string` ·
  `an_unrecognised_failure_carries_the_real_words_instead_of_a_guess` ·
  `every_registered_needle_lands_in_its_own_bucket` ·
  `a_name_that_would_break_the_target_never_reaches_tmux` ·
  `the_argv_is_the_read_only_capture_form_in_this_exact_order`
- daemon `readonly_guard::capture_is_read_only`（`+5`）：
  `the_site_is_really_being_read` ·
  `the_argv_this_site_emits_is_read_only_element_by_element` ·
  `the_site_names_no_state_changing_tmux_verb` ·
  `the_verb_scan_actually_bites_on_every_needle` ·
  `the_capture_site_is_one_shot`
- daemon `main.rs::stream_flag_tests`（`+1`）：
  `the_capture_pane_subcommand_is_actually_reachable`

⚠ **量于哪个提交**：`track/k-r86` 上本件的**第二个**提交（`evidence/` 这份文件写完之后的那一个）。
上表的「收官」列是那一趟门禁的现打读数。

⚠ **本树未铺 `src-tauri/embedded-daemons/`**（门禁每趟自印那一行）⇒ cargo 那个合计里
**少了「本地后端真的能起来吗」那 4 条**。本件 bump 了 `BUILD_ID`（p2f → p2g），
**在铺了内嵌 daemon 的树上，bump 必须同拍 re-embed**，否则 `src-tauri/build.rs` 当场 panic
（那是它有意的：内嵌 id ≠ 源码 id ⇒ 装出去会被永远判 stale ⇒ 无限重装）。
本轮**没有** re-embed，归发版那一拍 —— 与 `p2d` / `p2e` 两次的登记逐字同一句。

## 逐刀之后：盘上没留变异（自证）

每一刀跑完都 `cp` 还原（**不是 `cp -a`**）＋ `touch`，随后 `git -C <工作树> status --porcelain`：
`M1` 输出**空**；`M3` / `M4` / `M5` / `M6` / `M7` / `7u` 输出恒是 ` M evidence/K-R86-deathvalue.md`
一行（**那是本文件自己在被写**，不是残留的变异）。
⚠ **`M1` 那一趟的空输出是有信息量的**：它证明「还原干净」这件事在本轮至少有一趟是**逐字节**成立的，
后面几趟的那一行差异**只来自本文件**（被改的源文件一份都没出现在里面）。

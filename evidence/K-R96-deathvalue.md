# `K-R96` 死值验读数

**量法**：每一刀 = 在工作树 `.claude/worktrees/k-r96` 上把**一处**生产代码改坏 →
跑那条唯一许可的沙箱门禁（`.claude/devbox/gate <树> k-r96`，13 格）→ 记退出码与**哪几条判据叫了**
→ 还原。九刀**逐刀独立**（改一处、跑一趟、还原，再改下一处），不是一次改一堆再看总账 ——
那样归因不了。

**未改坏时的基线**：`GATE: OK —— 13 格全绿`
（`hooks 11` · `cargo 1567` · `daemon 713` · `npm 1708` · e2e `12/8/46/45` · `pb check FAIL=0`）。

🔴 **读法**：`门禁退出码 0` ＝ 这一刀**没被逮到** ＝ 判据在这一格上是空的。
下面每一刀都必须是**非 0**，`§1` 那三条 dod 才算真的有判据撑着。

## ⚠ 量具自己错过一次 —— 照实记在最前面

第一趟跑完之后核读数，发现**第 7、8 刀（都只改 TS）的 daemon 那一栏，红的是 `plan.rs` 里
一堆与 TS 毫不相干的判据，而且与第 6 刀的失败清单逐字相同**。查出来是量具的两处病：

1. **还原用 `shutil.move`，它保留原 mtime** ⇒ 还原后的源文件比上一刀编出来的产物还**旧**
   ⇒ cargo 认为「没变」，**复用上一刀那份改坏了的测试二进制**。
   只改 TS 的那两刀不触发任何 Rust 重编 ⇒ daemon 那一栏读到的是**第 6 刀的红**。
2. **备份文件写成 `<源文件>.kr96bak` 放在树里**，而 monitor 侧
   `structural_scan` 的死名语料是 `scan_tree!(root, &[])` —— **不按扩展名筛**
   ⇒ 那份副本被当成第二个文件读进语料，凭空多出一批「只活在散文里的名字」。
   （第 1 刀与第 7 刀里那条 `every_dead_name_named_in_the_prose_is_declared_dead`
   就是这么红的 —— **那一条与被测代码无关**。）

⇒ 处置：备份改放**仓外**、还原后 `os.utime(path, None)`、并 touch 两棵树各一处源码逼全量重编，
**第 7、8 刀整个重跑**（下面登的是重跑那一份）。其余七刀每一刀都动了 Rust 源
⇒ 全量重编过，不受第 1 病影响；第 2 病只会**多**红一条与被测对象无关的判据，
不会让该红的不红 ⇒ **那七刀的结论不受影响**，但读的时候要把
`every_dead_name_named_in_the_prose_is_declared_dead` 那一行当噪声划掉。

（这一族的名字：**量具的作用域对不上事实**。记在这里是因为下一个人用同一套脚本量别的东西时会再踩。）

## §3-1 ①  Gate 仍自己起 tmux list-sessions

- 门禁退出码 **1** ⇒ ✅ **红了**
- 叫出来的：
  - `| thread 'structural_scan::tests::every_dead_name_named_in_the_prose_is_declared_dead' (2235) panicked at src/structural_scan.rs:3065:9:`
  - `| test result: FAILED. 1446 passed; 1 failed; 9 ignored; 0 measured; 0 filtered out; finished in 10.85s`
  - `| thread 'control::gate::tests::both_tmux_call_sites_ask_for_a_utf8_client_before_the_subcommand' (2886) panicked at src/control/gate.rs:424:9:`
  - `| thread 'control::gate::tests::listing_every_session_is_no_longer_this_modules_job' (2888) panicked at src/control/gate.rs:318:9:`
  - `| thread 'control::gate::tests::the_liveness_answer_comes_from_this_moment_not_from_a_cache' (2892) panicked at src/control/gate.rs:366:9:`
  - `| thread 'readonly_guard::spawn_registry::every_process_spawn_in_production_is_registered' (3384) panicked at src/readonly_guard.rs:1021:9:`
  - `| test result: FAILED. 709 passed; 4 failed; 2 ignored; 0 measured; 0 filtered out; finished in 6.11s`
  - `GATE: FAIL —— cargo（退出码 101）；daemon（退出码 101）`

## §3-1 ②  读快照但不触发更新（拿陈值）

- 门禁退出码 **1** ⇒ ✅ **红了**
- 叫出来的：
  - `| thread 'common::session_snapshot::tests::a_warmed_value_is_never_what_the_query_hands_back' (2815) panicked at src/common/session_snapshot.rs:267:9:`
  - `| thread 'common::session_snapshot::tests::asking_the_snapshot_refreshes_it_so_the_answer_is_from_this_moment' (2816) panicked at src/common/session_snapshot.rs:247:9:`
  - `| thread 'common::session_snapshot::tests::the_taken_name_table_is_also_refreshed_by_the_asking' (2819) panicked at src/common/session_snapshot.rs:288:9:`
  - `| thread 'control::gate::tests::the_liveness_answer_comes_from_this_moment_not_from_a_cache' (2891) panicked at src/control/gate.rs:368:9:`
  - `| test result: FAILED. 709 passed; 4 failed; 2 ignored; 0 measured; 0 filtered out; finished in 8.85s`
  - `GATE: FAIL —— daemon（退出码 101）`

## §3-2 ①  铸名另起一份名字集合（不问快照）

- 门禁退出码 **1** ⇒ ✅ **红了**
- 叫出来的：
  - `| error[E0603]: tuple struct constructor `TakenNames` is private`
  - `GATE: FAIL —— daemon（退出码 101）`

## §3-2 ②  同一份快照喂两次而 --print 输出不同（纯性）

- 门禁退出码 **1** ⇒ ✅ **红了**
- 叫出来的：
  - `| thread 'control::ccm::plan::tests::a_different_snapshot_moves_the_name' (2846) panicked at src/control/ccm/plan.rs:1107:9:`
  - `| thread 'control::ccm::plan::tests::only_two_of_the_three_naming_paths_step_aside_on_a_collision' (2852) panicked at src/control/ccm/plan.rs:1046:9:`
  - `| thread 'control::ccm::plan::tests::printing_twice_against_the_same_snapshot_gives_the_same_line' (2855) panicked at src/control/ccm/plan.rs:1083:9:`
  - `| thread 'control::ccm::plan::tests::the_session_name_reads_like_a_project_and_the_sid_rides_the_tmux_option' (2864) panicked at src/control/ccm/plan.rs:1138:9:`
  - `| test result: FAILED. 709 passed; 4 failed; 2 ignored; 0 measured; 0 filtered out; finished in 4.92s`
  - `| FAIL | deriveTmuxName 对拍: /home/pi/proj`
  - `| FAIL | deriveTmuxName 对拍: /home/pi/a  b`
  - `| FAIL | deriveTmuxName 对拍: /home/pi/proj///`
  - `| FAIL | deriveTmuxName 对拍: /`
  - `| FAIL | deriveTmuxName 对拍: /home/pi/.hidden.dir`
  - `GATE: FAIL —— fmt-daemon（退出码 1）；daemon（退出码 101）；ccm e2e/ccm-cli（退出码 1；实得 PASS=41，地板 46，判法 exact。诊断原文见上方本套件自己的输出）`

## §3-2 ③  快照变了而名字不跟

- 门禁退出码 **1** ⇒ ✅ **红了**
- 叫出来的：
  - `| thread 'control::ccm::plan::tests::a_different_snapshot_moves_the_name' (2839) panicked at src/control/ccm/plan.rs:1101:9:`
  - `| thread 'control::ccm::plan::tests::only_two_of_the_three_naming_paths_step_aside_on_a_collision' (2845) panicked at src/control/ccm/plan.rs:1039:9:`
  - `| thread 'control::ccm::plan::tests::printing_twice_against_the_same_snapshot_gives_the_same_line' (2848) panicked at src/control/ccm/plan.rs:1077:9:`
  - `| test result: FAILED. 710 passed; 3 failed; 2 ignored; 0 measured; 0 filtered out; finished in 4.94s`
  - `GATE: FAIL —— daemon（退出码 101）；pb check[backend-consolidation]（===== pb check: FAIL=1 BROKEN=0 =====）`

## §3-3 ①a 名字里出现 sid 片段（daemon 铸名）

- 门禁退出码 **1** ⇒ ✅ **红了**
- 叫出来的：
  - `| thread 'control::ccm::plan::tests::a_different_snapshot_moves_the_name' (2842) panicked at src/control/ccm/plan.rs:1104:9:`
  - `| thread 'control::ccm::plan::tests::every_value_that_reaches_a_shell_is_quoted' (2846) panicked at src/control/ccm/plan.rs:1436:9:`
  - `| thread 'control::ccm::plan::tests::only_two_of_the_three_naming_paths_step_aside_on_a_collision' (2848) panicked at src/control/ccm/plan.rs:1041:9:`
  - `| thread 'control::ccm::plan::tests::the_container_path_carries_every_intent_inward' (2854) panicked at src/control/ccm/plan.rs:1198:9:`
  - `| thread 'control::ccm::plan::tests::the_session_name_reads_like_a_project_and_the_sid_rides_the_tmux_option' (2860) panicked at src/control/ccm/plan.rs:1135:9:`
  - `| test result: FAILED. 708 passed; 5 failed; 2 ignored; 0 measured; 0 filtered out; finished in 5.19s`
  - `| FAIL | tmux 名 cc-p1 出现在 new-session`
  - `| FAIL | 会话名 cc-proj 正确出现`
  - `| FAIL | deriveTmuxName 对拍: /home/pi/proj`
  - `| FAIL | deriveTmuxName 对拍: /home/pi/a  b`
  - `| FAIL | deriveTmuxName 对拍: /home/pi/proj///`
  - `| FAIL | deriveTmuxName 对拍: /`
  - `| FAIL | deriveTmuxName 对拍: /home/pi/.hidden.dir`
  - `GATE: FAIL —— daemon（退出码 101）；ccm e2e/ccm-print-parity（退出码 1；实得 PASS=10，地板 12，判法 exact。诊断原文见上方本套件自己的输出）；ccm e2e/ccm-cli（退出码 1；实得 PASS=41，地板 46，判法 exact。诊断原文见上方本套件自己的输出）；pb check[backend-consolidation]（===== pb check: FAIL=1 BROKEN=0 =====）`

## §3-3 ①b 名字里出现 sid 片段（前端把 sid 当 cwd 传）（量具修好后重跑）

- 门禁退出码 **1** ⇒ ✅ **红了**
- 叫出来的：
  - `|      × ★★ 基名被占 ⇒ 载荷里的 `tmuxName` **让到了 `-2`**（证明它真过了铸造口，不是拼出来的） 84ms`
  - `|      × ★ 没被占 ⇒ 就是基名本身（反过来钉住：它不是**恒**加后缀） 50ms`
  - `|      × ★★ KR96D3：名字读得出是哪个项目，且**一个 sid 片段都没有** 35ms`
  - `|  FAIL  src/views/history-actions.vitest.ts > K-R46：历史页 resume 的 tmux 名（行为） > ★★ 基名被占 ⇒ 载荷里的 `tmuxName` **让到了 `-2`**（证明它真过了铸造口，不是拼出来的）`
  - `|  FAIL  src/views/history-actions.vitest.ts > K-R46：历史页 resume 的 tmux 名（行为） > ★ 没被占 ⇒ 就是基名本身（反过来钉住：它不是**恒**加后缀）`
  - `|  FAIL  src/views/history-actions.vitest.ts > K-R46：历史页 resume 的 tmux 名（行为） > ★★ KR96D3：名字读得出是哪个项目，且**一个 sid 片段都没有**`
  - `GATE: FAIL —— npm（退出码 1）`

## §3-3 ②  撞名不避让（量具修好后重跑）

> ⚠ **本刀只留下了格级读数（`npm` 红），没留下判据名。** 成因是抓取器的射程：
> 它认的是 vitest 那种 `×` / `FAIL ` 行，而这一刀先撞上的是 **tsx / tsc 那一层的响错**
> （`mintTmuxName` 改成恒回基名之后，`let i = 2;` 那几行成了不可达代码）。
> **红是真的红**（退出码 1），但「是哪几条判据叫的」这一格本刀**没量到** —— 照实登记，不补写。

- 门禁退出码 **1** ⇒ ✅ **红了**
- 叫出来的：
  - `GATE: FAIL —— npm（退出码 1）`

## §3-3 ④  把 @ccm_sid 也一起去掉（sid 必须还在）

- 门禁退出码 **1** ⇒ ✅ **红了**
- 叫出来的：
  - `| thread 'control::ccm::plan::tests::the_container_path_carries_every_intent_inward' (2856) panicked at src/control/ccm/plan.rs:1199:9:`
  - `| thread 'control::ccm::plan::tests::the_session_name_reads_like_a_project_and_the_sid_rides_the_tmux_option' (2862) panicked at src/control/ccm/plan.rs:1151:9:`
  - `| test result: FAILED. 711 passed; 2 failed; 2 ignored; 0 measured; 0 filtered out; finished in 5.24s`
  - `| FAIL | @ccm_sid_expect 打标用了 p1（F04：通道A写意图，非事实 @ccm_sid）`
  - `GATE: FAIL —— daemon（退出码 101）；ccm e2e/ccm-print-parity（退出码 1；实得 PASS=11，地板 12，判法 exact。诊断原文见上方本套件自己的输出）`


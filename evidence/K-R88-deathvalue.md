# K-R88 死值验留档

全部在沙箱里跑（`K31`/`R15`）：`docker run --rm --network none` + 与 `.claude/devbox/gate`
**逐字相同的挂载集**（项目目录 · 具名卷做 cargo registry · `CARGO_TARGET_DIR` 指 `pm-targets/k-r88`）。
**一趟一刀、跑完还原**，逐刀 `git status --porcelain` 自证盘上没留变异。

基线 `M0` = `GATE: OK —— 13 格全绿`（cargo **1594** · daemon **714** · npm **1714** ·
hooks 11 · e2e 12/8/46/45 · generated 绿 · pb check FAIL=0），
**量于分支 `track/k-r88`（基点 `eaaf41a`）**。

## ⚠ 一条量具自己的坑，写在最前面（本轮真的踩了一次）

**还原变异不许用 `cp -a`。** `-a` 连 mtime 一起还原 ⇒ 还原后的源文件比上一次构建产物还老，
`cargo` 判它「没变」，**沿用上一刀那份编译结果**。
本轮第一次跑 `M3` 时因此多红了两条（`an_unknown_…` / `branch_impl_leaves_…`）——
那两条是**上一刀 `M2`（深度 2→1）残留在产物里的红**，不是 `M3` 的效果。
处置：还原一律 `cp`（不带 `-a`）＋ `touch`，`M2` 与 `M3` **都重跑了一遍干净的**，下面给的是重跑读数。
🔴 如实记：这一坑与本区那条「量具的作用域对不上事实」同族 —— **量具的时间维**也会对不上。

## 刀与读数

| 刀 | 变异 | 落在哪 | 实测 |
|---|---|---|---|
| `M0` | 无 | — | cargo **1594** / daemon **714** 全绿 |
| `M1` | 本机那侧自己再写一份「找文件」（裸 `read_dir` 两层） | `history.rs::branch_impl` | **红**：monitor `1459 passed; 3 failed` |
| `M2` | 🔴 **改共享那一份一处**：`SESSION_LOOKUP_DEPTH` 2 → 1 | `branch-core` | **两边一起红**：monitor `1460 passed; 2 failed` ＋ daemon `711 passed; 3 failed` |
| `M3` | 本机那条命令退回收路径（`source_jsonl_path` + 从路径反推 sid） | `history.rs::create_branch_session` | **红**：monitor `1461 passed; 1 failed`（只红一条） |
| `M4` | 🔴 **共享那一份「查不到就静默拿树上第一份」** | `branch-core::look_down` | **两边各红一条、且是同名那条**：monitor `1461 passed; 1 failed` ＋ daemon `713 passed; 1 failed` |
| `M5` | daemon 只读白名单加第三条（`observe/watcher.rs`） | `readonly_guard.rs` | **红**：daemon `712 passed; 2 failed` |

### `M1` 点名（`KR88D1` ①）

- `history::tests::finding_a_session_file_by_sid_now_lives_in_exactly_one_place`
- `history::tests::branch_source_guard_rejects_dotdot_traversal`
- `history::tests::the_branch_entry_point_actually_goes_through_the_fence`

⚠ 后两条一起红**是对的、也是想要的**：自己再写一份的人**不会顺手把 sid 形状闸也抄过去**，
于是 `..` 那一族当场从那一侧漏进去。**「长出第二份」与「围栏破一个口」在这里是同一件事。**

### `M2` 点名（🔴 `KR88D1` ③ —— 本件最贵的那一刀）

- monitor：`history::tests::an_unknown_session_id_is_refused_not_silently_substituted` ·
  `history::tests::branch_impl_leaves_source_untouched_and_writes_native_branch`
- daemon：`control::fork_write::tests::an_unknown_session_id_is_refused_not_silently_substituted` ·
  `control::fork_write::tests::fork_writes_new_file_and_leaves_source_untouched` ·
  `control::fork_write::tests::sidechain_reject_reaches_daemon_path`

**一处改动、两棵树一起红** —— 这是「同一份实现」的**行为**证据，
不是「两边源码文本一样」那种写法证据（`KR88D1` 逐字点名的失效方向）。

### `M3` 点名（`KR88D2` ①）

只红 `history::tests::both_branch_commands_take_a_session_id_not_a_path`。
**只红这一条**是本刀该有的形状：它判的正是那两个 `#[tauri::command]` 的**签名**。

### `M4` 点名（🔴 `KR88D2` ③ —— 「两边处置一致」）

monitor 与 daemon **各红一条，而且是同名的那条**
（`an_unknown_session_id_is_refused_not_silently_substituted`）。
两条各自的夹具里**都真有别的会话可被「随手挑」**（反向自检 `assert!(… .is_ok())` 先跑），
所以它不是在空树上零命中地绿。

### `M5` 点名（`KR88D3` ①）

- `readonly_guard::tests::daemon_write_capability_is_confined_to_the_registered_modules`
- `readonly_guard::tests::every_registered_write_module_is_really_on_the_tree_and_really_writes`

`KR88D3` ②（仍 2 条）：现打 `WRITE_WHITELIST_MODULES` = **2**
（`control/fork_write.rs` · `platform/landing.rs`），本件**一个字没改那张表**。

## 交回时的三个数（现打，量于 `track/k-r88` 工作树）

| 问 | 现打 |
|---|---|
| 「找文件」剩几份实现 | **1** —— `pub fn find_session_file` 全仓 1 处声明（`crates/branch-core/src/lib.rs`）；生产段调用点两侧各 1（`history.rs` · `control/fork_write.rs`） |
| daemon 只读白名单几条 | **2**（与现打一致，押注命中） |

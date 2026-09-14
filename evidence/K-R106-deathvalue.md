# K-R106 死值验原文（`C` 阶段填）

四条 dod ＋ `7u` 的刀与**完整原文读数**住这里；件文件 `§3` 只放摘要。

## 规矩（每条都是前面几件真踩出来的）

- **全部在沙箱里跑**：`PB_WS=backend-consolidation .claude/devbox/gate <本树绝对路径> <tag>`。
  🔴 `K-R101` 的实现方自陈破过这条，**违规已登记**。别重蹈。
  ⚙ 本件实测：**那条命令的第一个参数必须是绝对路径**（给相对路径 ⇒ `docker: the working
  directory '…' is invalid` ⇒ **退出码 125，一格都没跑**。它长得不像门禁红，别读成绿）。
- **一趟一刀**，下一刀前还原干净，贴 `git status --porcelain` 自证。
- 🔴 **还原不许 `cp -a`**（`K-R88`：连 mtime ⇒ cargo 判「没变」⇒ 读到上一刀的红）。
- 🔴 **刀具取不到该取的东西必须拒跑（fail-closed）**（`K-R87` 的 `cut.sh` 静默跳过）。
- 🔴 **同文件多处编辑当心后写覆盖前写**（`K-R104`：首趟只落地一处，**而它照印「变异已落地」**）。
- 🔴 **门禁在跑的时候，那棵工作树是它的**（`K-R101` 撞出来；`K-R89` 自己接住过）。
- ⚙ **「变异存活」两种成因别混**：判据瞎 ⇒ 补判据 · 变异没生效 ⇒ 重切刀。
- ⚙ **「自检 ≠ 变异验过」**（`K-R104`）：新判据默认是**没验过的**。
- ⚙ 写 shell 刀具会被 `shell_lint_registry` 拦；先例是**折成 `.py`**。

## 量具住址（两份，别与别的工作树混）

| 量具 | 住址 | 被测对象指向哪棵树 |
|---|---|---|
| 尺子 | `<本工作树>/evidence/K-R106-ruler.py` | `ROOT = 本文件所在树的仓根`（不读 `cwd`） |
| 刀 | `<本工作树>/evidence/K-R106-cut.py` | 同上 |

「本工作树」= `/home/zbl/文档/claudecode-frontend/.claude/worktrees/k-r106`（分支 `track/k-r106`）。
⚠ 与 `K-R105-cut.py` / `K-R105-ruler.py` 是**两份不同的量具**（两棵不同的树），**别互相照搬读数**。

## 🔴 本件特有：**那两条被绕过的判据，你要再切一次证明加固真挡住了**

- `K-R89`：对拍**在比 `x == x`**（`want` 被写死成 `got`）—— 改动看得见。
- `K-R105`：在 `if got != want {` **前面**插 `let want = got.clone();`（**原行一字不动**）
  ⇒ **同名遮蔽**，对拍变成 `got != got`，**13 格全绿一声不吭**。
  守它的判据当时量的是**子串存在性**，头注自陈「防顺手不防决心」——★ **而遮蔽恰恰是顺手。**

⇒ `KR106D4` 第③刀要求：**再切一次遮蔽那一刀，必须红。** 证明加固买到的是**行为**，不是换了个说法。
**下面 `D4-shadow` / `D4-unharden` 两行就是这一对读数（一红一绿）。**

## `M0` 基线 —— **本件开工那一刻现打，没抄**

量于基点 `674c2e8`（`track/k-r106` 未做任何改动，工作树只有一份 `A evidence/K-R106-deathvalue.md` 骨架）：

```
GATE: OK —— 13 格全绿
hooks 11 · fmt 1 · fmt-daemon 1 · winchk 1 · cargo 1599（9 个包合计）
generated 一致 · daemon 745 · npm 1722
ccm e2e: ccm-print-parity 12 · ccm-rbind-title 8 · ccm-cli 46 · ccm-contract-parity 45
pb check[backend-consolidation]: FAIL=0 BROKEN=0
分母 cargo：本树**未铺** `src-tauri/embedded-daemons/` ⇒ 少了「本地后端真的能起来吗」那一族 4 条
```

⚙ 与单子给的参照（1599 / 745 / 1722 / 12·8·46·45）**逐格相同** —— 但这一份是自己打的。

## `M1`：第一趟落地 —— **红了四样，每一样都是读数**

```
GATE: FAIL —— fmt（退出码 1）；cargo（退出码 101）；npm（退出码 1）
```

| 红的是谁 | 现打说了什么 | 处置 |
|---|---|---|
| `fmt` | 三处 rustfmt 排版 | 按它印的 diff 逐处改 |
| `doc_claim_registry::every_repo_path_named_in_the_docs_still_resolves` | `doc/INVARIANTS.md` 点名了 `evidence/K-R106-ruler.py`，而它**不在 `git ls-files` 里** | 量具入库（`git add`） |
| `history::…::every_one_of_the_six_cells_is_measured_not_narrated` | 「Windows」格**现打 Closed、表上写 Structural** | 见下方「量具事故」 |
| `src/ipc/commands.vitest.ts` 的 `C04a` 两条 | `render_local_attach` **声明了却没注册** ＋ 「TS 静态看不见的命令集变了」 | 见下方「接线的真实代价」 |

### 🔴 量具事故：**闸放错位置，把一条不相干的观测口截断了**

第一版把 attach 那道 fail-closed 放在 `launch_local` 的**函数入口**，并给它挂了 `#[cfg(not(windows))]`。
六格表「Windows」格的观测口逐字是「`fn launch_local(` 到**第一个** `#[cfg(not(windows))]`
之间有没有 `let _ = tmux_name;`」——**新属性成了那个「第一个」**，于是那一格从
`Structural` 翻成 `Closed`。

⇒ **改闸的位置，不改那条观测口。** 闸挪进 `#[cfg(not(windows))] let base = { … }` 块的头上，
现打回到 `Structural`。〔改观测口去迁就实现，就是本工作区反复治的那一形。〕

### 🔴 接线的真实代价：**本仓不接受「声明了不注册」这个中间态**

第一版给 `render_local_attach` 挂了 `#[tauri::command]`，想着「注册那一行归 PM」。
**`src/ipc/commands.vitest.ts` 的 `C04a` 当场红两条**：
「这些命令声明了却没注册 ⇒ 前端调不到: expected [ 'render_local_attach' ] to deeply equal []」
与「TS 静态看不见的命令集变了」。⇒ 属性摘掉，函数留下（`#[cfg(not(windows))] pub(crate) fn`）。

**把它接出去要动四处 ＋ 前端一处，逐处见件文件 `§8`。**

## `M2` 基线（第一轮：attach 实现落地、门禁复绿）

```
GATE: OK —— 13 格全绿
hooks 11 · fmt 1 · fmt-daemon 1 · winchk 1 · cargo 1600（+1，就是本件那条新判据）
generated 一致 · daemon 745（±0）· npm 1722（±0）
ccm e2e: 12 / 8 / 46 / 45（四格地板恒等）
pb check[backend-consolidation]: FAIL=0 BROKEN=0
```

## `M4` 基线（**第二轮**：`KR106D4` ① 的反向棘轮落地）

```
代码侧 12 格全绿
hooks 11 · fmt 1 · fmt-daemon 1 · winchk 1 · cargo 1601（M2 1600 +1，就是那条反向棘轮）
generated 一致 · daemon 745（±0）· npm 1722（±0）
ccm e2e: 12 / 8 / 46 / 45
第 13 格 pb check：FAIL=1 —— `[J5 审计正文] audits/K-R103-PM.md 交回之后正文变了`。
⚠ **那是 PM 自己在计划仓的在飞编辑**（`audits/` 是我一个字节都不许碰的面），不是代码侧的红。
```

⚠ **`M2` 之后计划仓那侧被 PM 动过一次**：`.dispatch.json` 的 mtime 是 `18:15:07`，
而 `INDEX.md` 是 `18:06:15` ⇒ 此后每一趟门禁的第 13 格都印
`FAIL [J3 陈账] INDEX.md 比源文件旧`。**那不是任何一把刀的牙**，也不是代码侧的红；
生成命令（`pb index`）**窗口开着不许跑**，本件一趟都没跑（同 `K-R105` 交回时那一格）。
⇒ 下面每一行的「门禁」列里那句 `pb check FAIL=1` 一律是这同一条，**不重复解释**。

## 变异表 —— 一趟一刀，逐行原文读数

**分母怎么数的**：`cargo` 那一列是 `-p monitor --lib` 那一行 `test result:` 的原文
（门禁的「cargo N passed」是**9 个包合计**，两者分母不同，别混）。
每一行的「锚点命中」是 `K-R106-cut.py` 落刀前 fail-closed 断言的那个数。

| # | 刀 | 锚点 · 命中 | 门禁 | `-p monitor --lib` | 红的是谁（逐条点名） |
|---|---|---|---|---|---|
| 1 | `D1-newarm` | `history.rs` 的 attach 渲染臂 · **1** | `FAIL —— cargo(101)` | `1467 passed; 1 failed` | **只红一条**：`history::…::the_local_backend_renders_an_attach_that_lands_on_the_session_it_just_created`（`history.rs:3133` = 「动作不是 attach」那一格）|
| 2 | `D1-shape` | `render_local_ccm_with` 里 `NO_TMUX_NAME` 之后那三行 · **1** | `FAIL —— cargo(101)` | `1466 passed; 2 failed` | 本件那条（`:3158` = `gate_core` 不认这个名字）＋ `history::…::the_rendered_local_command_really_carries_the_container`（`:3244`）。⚠ **第二条是刀的副作用，不是第二颗牙**：这一刀把容器名对**所有动作**都换了，那条老判据钉的是 `--tmux=<名>` 的逐字内容 |
| 3 | `D1-oldpath` | 旧路那道 fail-closed 的 `return Err` · **1** | `FAIL —— cargo(101)` | `1467 passed; 1 failed` | **只红一条**：本件那条（`:3191` = 旧路居然给 attach 渲出了东西）|
| 4 | `D1-spawn` | `launch_local` 里那道 attach 闸整段 · **1** | `FAIL —— cargo(101)` | `1467 passed; 1 failed` | **只红一条**：本件那条（`:3195` = `launch_local` 收下了 attach）|
| 5 | `D2-droppath` | `CREATION_PATHS` 那一行 · **1**；删 `src/session-backend.ts` · **1** | `FAIL —— cargo(101)；npm(1)；ccm e2e/ccm-cli(1)` | `1464 passed; 4 failed` | ① `daemon_kill::no_creation_path_can_mint_a_name_the_main_path_cannot_kill`（`:425`）· ② `launch_wire::f07_main_path_tests::the_ts_fallback_renderer_now_stands_on_its_own_consumers`（`:954`）· ③ `doc_claim_registry::each_registered_status_still_matches_reality`（`:1234`）· ④ `doc_claim_registry::the_outer_layer_producers_are_in_the_state_the_doc_claims`（`:718`）；**npm**：`remote-launch.test.ts` `ERR_MODULE_NOT_FOUND —— launch-render-fallback.ts` 找不到座；**e2e**：`ccm-cli` 实得 `PASS=41`（地板 46），挂的是 5 条 `deriveTmuxName 对拍` |
| 6 | `D2-orphan` | 那条创建路径理由里点校验器名的那半句 · **1** | `FAIL —— cargo(101)` | `1467 passed; 1 failed` | **只红一条**：`daemon_kill::no_creation_path_can_mint_a_name_the_main_path_cannot_kill`（`:430` = ③b **校验器落单**那一格）|
| 7 | `D3-writeback` | `doc/ARCHITECTURE.md` 那句 · **1** | `FAIL —— cargo(101)` | `1467 passed; 1 failed` | **只红一条**：`daemon_kill::the_doc_sentence_about_the_transitional_fallback_cannot_outlive_the_code`（`:582`）|
| 8 | `D3-narrow`（**阴性对照**）| 判定那一行退回「只看 `IPC-PROTOCOL.md`」· **1**；同一句写回 `ARCHITECTURE.md` · **1** | `cargo/npm/e2e **全绿**`（只剩那条计划仓陈账）| `cargo 1600 passed` | **一条都没红** ⇒ 🔴 **「只改文档不扩人群」买不到任何东西**：同样的话照样溜得进去 |
| 9 | `D4-shadow` | `launch_payload_parity.rs` 的 `if got != want {` · **1** | `FAIL —— cargo(101)` | `1467 passed; 2 failed` | ① `launch_wire::…::the_byte_for_byte_parity_still_has_two_independent_sides`（`:1395` = 判定本体报 ②「`let want` 出现了 2 次」）· ② `launch_wire::…::the_parity_guard_counts_bindings_it_does_not_merely_look_for_them`（`:1254`）。⚠ **第二条不是重复的牙**：那是棘轮的**反空真前置**（「盘上那份对拍语料自己就过不了判定 ⇒ 下面三份变异体的红证明不了任何事」）—— 这一刀确实把真语料弄脏了，它喊得对 |
| 10 | 🔴 `D4-unratchet`（**`KR106D4` ① 的落点**：只把匹配单位退回子串存在性，别的一个字不动）| 1 | `FAIL —— cargo(101)` | `1468 passed; 1 failed` | **只红一条**：`launch_wire::…::the_parity_guard_counts_bindings_it_does_not_merely_look_for_them`（`:1291` = 那份**同名遮蔽**变异体不再被认出来）⇒ 🔴 **① 从「一条都不红」变成「当场红」** |
| 11 | `D4-unharden`（**阴性对照**）| 退回子串存在性 · **1**；同一把遮蔽刀 · **1** | `FAIL —— cargo(101)` | `1468 passed; 1 failed` | **只红棘轮那一条**（`:1291`）；`the_byte_for_byte_parity…` **不红** ⇒ 两件事同时被证：① 那一红原本就是「计数」买的（退掉它，遮蔽刀就溜过去了）· ② **退掉它这一步今天有人接住了** |
| 12 | `7u` | 6 处（渲染臂 · 旧路闸 · spawn 闸 · 三份文档）· 各 **1** | `FAIL —— cargo(101)` | `1467 passed; 2 failed` | `daemon_kill::the_doc_sentence…`（`:582`）＋ 本件那条（`:3130`）。⚠ 新棘轮**仍绿**（它守的是判定本体，与 attach 那一臂无关）|

**CRASH：0 条**（每一趟的判定行数都在，`test result:` 那一行逐趟印得出来；
没有 `SyntaxError` / 运行期炸这一族）。
**每一刀落地后 `git status --porcelain` 与落刀前逐行相同**（`restore` 之后回到同一张单子），
`.K-R106-cut-backup.json` 每趟都被 `restore` 删掉；收工时它不在盘上。

## 🔴 `D1-shape` 第一版活了下来 —— 而那不是判据瞎，是**一条关于 `gate_core` 的现打**

第一版这把刀的替换是 `format!("ccm-oneshot-{name}")`。它**全绿**。
成因不是判据没看，是 **`ccm-oneshot-s1abcdef-cc` 这个名字仍然过得了 Gate 2**：
`gate_core::is_ccm_tmux_name` 的后缀形那一支只看「以 `-cc` 结尾、且前面非空」，
**不看前面挂了什么**。⇒ 加一个 `ccm-oneshot-` 前缀**不足以**让一个名字掉出 Gate 2。

⇒ 刀换成**整名替换**（`ccm-oneshot-usage`，`K-R87` 那个真形状），当场红。
★ 这一条本身是给下一个人的读数：**「名字看起来像 `K-R87` 那一形」不等于「它掉出了 Gate 2」。**

## 🔴 `KR106D4` ①：**第一轮量到它不成立，第二轮把闸补上了**

**第一轮现打**：`§1` 的 ① 逐字是「把加固退回『子串存在性』⇒ **红**」，而实测**一条都不红** ——
成因是结构性的：**没有任何判据在守那段加固本身**
（`the_retired_premise_left_a_tombstone_that_is_still_on_the_board` 看的是
`TS_FALLBACK_KEEPERS` / `TS_FALLBACK_REACH`，够不着它）。

**PM 09-13 裁**：**不改 dod 字面，回去补一条反向棘轮，让它真的能红**
（纪律 ⑯：dod 对不上现打时先问能不能把闸补上，补不上才改字面）。

**第二轮做了什么**（`src-tauri/src/backend/control/launch_wire.rs`，写区内，**没新建文件**）：

1. 判定**从判据体里搬出来**，成为一份具名的判定本体
   `f07_main_path_tests::the_two_sides_are_still_independent(body) -> Result<(), ParityBypass>`
   —— **判据与棘轮共用这一份**（就地写在判据里的判定，自己不可被驱动，
   `references/testing.md` 四之 6 逐字：「判据不许 inline 在测试里，否则它自己不可被变异」）。
   三格：① 左边取自入库夹具 · ② **两边各只许被绑一次（计数）** · ③ 右边跑生产命令本体。
   绕过形状做成**枚举** `ParityBypass`，棘轮**按格认不按文字认**（`brief` 第 12b 条）。
   ⚙ 顺带收紧一格：① ③ 此前读的是整份 `PARITY_SRC`，现在读**函数体**
   —— 文件别处提一句同样的话不再能替它兑现。
2. 反向棘轮
   `f07_main_path_tests::the_parity_guard_counts_bindings_it_does_not_merely_look_for_them`。

**🔴 棘轮怎么钉的 —— 刻意不是「文件里有没有出现某个词」**：
那种写法正是本族（`needle_anchor_registry`：**匹配单位比事实小**）自己要治的病的同形，
换个等价写法就绿，而且它证不出行为。⇒ 棘轮**从真语料现造三份变异体**
（不是手写夹具 —— 手写的会与真语料漂开），逐份断言判定本体**指名点姓地**报出哪一格：

| 变异体 | 复刻的是哪一刀 | 必须报 |
|---|---|---|
| 左边被写死成右边 | `K-R89` | ① `LeftNoLongerFromTheFixture` |
| **原行不动、前面再绑一次** | `K-R105` | ② `ASideIsBoundMoreThanOnce{side:"let want",times:2}` —— **只有计数看得见** |
| 右边改成自己重搭 | 复盘那一刀 | ③ `RightNoLongerRunsTheProductionCommand` |

外加两条反空真：**干净语料必须过**（不过则下面三条一律作废）·
造变异体前逐处断言锚点**恰好命中一次**（fail-closed，锚点漂了当场炸，不许零命中地绿）。

**验它的刀（第 10 · 11 行）**：`D4-unratchet` 只把 ② 那一格换成 `body.contains(side)`
（循环、变体、诊断一律不动 —— **最小面**）⇒ **只红棘轮那一条**。
`D4-unharden`（退计数 ＋ 同一把遮蔽刀）⇒ 也只红棘轮，而 `the_byte_for_byte_parity…` **不红**
⇒ 「那一红原本是计数买的」与「退计数今天有人接住」**同时**被证。

**⚠ 它买不到什么（如实写，两侧都说）**：

- 棘轮守的是**判定本体**，不是那条判据的**调用点**。有人把调用删掉、就地再写一个更弱的检查，
  棘轮看不见（那会让判定本体变成死代码、编译器喊 `dead_code`，**但没有判据会红**）。
  ⇒ 它把「顺手」那条路封了（改判定本体是最省事的写法，而那一步现在当场红），**不封「决心」**。
  两处头注都写了这一条。
- 判定仍是**文本**：改变量名、把比较搬进 helper 之类的等价重写躲得过。

## `7u`：把实现整个退掉，还有多少条新断言仍绿

本件新增/加宽的断言共 **5 条**（第二轮加了棘轮那一条）：

| # | 新断言 | `7u` 之后 | 仍绿的话，理由 |
|---|---|---|---|
| 1 | `history::…::the_local_backend_renders_an_attach_that_lands_on_the_session_it_just_created` | 🔴 **红**（`:3130`）| — |
| 2 | `daemon_kill::the_doc_sentence_about_the_transitional_fallback_cannot_outlive_the_code`（人群加宽那一半）| 🔴 **红**（`:582`）| — |
| 3 | `cross_half_edge_registry::CROSS_EDGES` 那两条新登记（monitor→daemon，`argv.rs` / `plan.rs`）| ⚪ **仍绿** | 它们钉的是「这条编译期边登记了没有」，**与 attach 那一臂在不在无关** —— `7u` 掏空的是实现，`include_str!` 那两行住在判据里，`7u` 不碰。⚠ **仍绿不等于仪式**：它们的牙在 `D1-shape`/`D2-droppath` 之外的另一个方向（谁新长一条跨半边边而不登记），本轮没有单独给它们切刀，**如实登记为「本轮没验过」** |
| 4 | `launch_wire::…::the_parity_guard_counts_bindings_it_does_not_merely_look_for_them`（反向棘轮）| ⚪ **仍绿** | 它守的是**对拍那条判据的匹配单位**，与本件 attach 那条实现**正交** —— `7u` 掏空 attach 掏不到它。**它自己的牙在第 10 · 11 行**（`D4-unratchet` / `D4-unharden` 各只红它一条），不是没验过 |

⇒ **仍绿 3 条**（两条跨半边登记 ＋ 棘轮），且已逐条给出理由；其中棘轮**另有自己的刀验过**。

## `git status --porcelain` 三处（收工时）

- **本工作树**（`.claude/worktrees/k-r106`）：见件文件 `§8`。
- **代码仓主树**（`cc-monitor`）：本件一个字节没碰。
- **计划仓**：只写了件文件的 `§3` / `§8`（本文件住代码仓 `evidence/`，不是计划仓）。

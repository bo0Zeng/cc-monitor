# K-R105 死值验原文（`C` 阶段填）

四条 dod ＋ `7u` 的刀与**完整原文读数**住这里；件文件 `§3` 只放摘要。

## 规矩（每一条都是前面几件真踩出来的）

- **全部在沙箱里跑**：`PB_WS=backend-consolidation .claude/devbox/gate <本树绝对路径> <tag>`。
  🔴 `K-R101` 的实现方自陈破过这条；PM 实测无害但**违规已登记**。别重蹈。
- **一趟一刀**，下一刀前还原干净，贴 `git status --porcelain` 自证。
- 🔴 **还原不许 `cp -a`**（连 mtime ⇒ cargo 判「没变」⇒ 读到上一刀的红）—— `K-R88`。
- 🔴 **刀具取不到该取的东西必须拒跑（fail-closed）** —— `K-R87` 的 `cut.sh` 静默跳过。
- 🔴 **刀具在同文件多处编辑时当心后写覆盖前写** —— `K-R104`：首趟只落地一处，**而它照印「变异已落地」**。
- 🔴 **门禁在跑的时候，那棵工作树是它的** —— `K-R101` 撞出来，`K-R89` 自己接住过（重跑了一趟干净的）。
- ⚙ **「变异存活」两种成因别混**：判据瞎 ⇒ 补判据 · 变异没生效 ⇒ 重切刀。
- ⚙ **「自检 ≠ 变异验过」**（`K-R104`）：新判据默认是**没验过的**。
- ⚙ 写 shell 刀具会被 `shell_lint_registry` 拦；先例是**折成 `.py`**，别去改 CI 那两张表。
- ⚙ 本树 `node_modules` 软链已铺（不铺 `M0` 必红），gitignore 盖着、**不是写区项**。

## ⚠ 本件特有的一条

🔴 **`K-R89` 刚逮到一个自洽夹具活体**：那条对拍**在比 `x == x`**，所以它**看起来一直是绿的**。
⇒ **你给 `KR105D4` 切刀时，先确认被守的那条判据本身不是这一形** —— 否则你的刀红了也证明不了什么。

## `M0` 基线（量于主干 `38168d7`，本树 `track/k-r105` 基点，**自己现打**）

```
GATE: OK —— 13 格全绿（hooks · fmt · fmt-daemon · winchk · cargo · generated · daemon · npm · 四套 ccm e2e · pb check）
hooks 11 · fmt 1 · fmt-daemon 1 · winchk 1 · cargo 1598（9 包合计） · generated ok
daemon 745 · npm 1722 · e2e 12 / 8 / 46 / 45 · pb check FAIL=0 BROKEN=0
```
⚠ **分母照抄门禁自己印的那一行**：本树**未铺** `src-tauri/embedded-daemons/` ⇒ `embedded_daemons` cfg 不置
⇒ cargo 那个合计里少了「本地后端真的能起来吗」那一族 4 条。
（`M0` 与 `K-R89` 收官读数逐格相同 —— 参照对上了，但**这一份是重打的**，不是抄的。）

## 交付态 `M1`

第一趟 `M1` **红了两格，红得对，都不是本件的正题**，如实登记（`brief` 第 8 条：CRASH / 副作用要单列）：

| 红的格 | 原文 | 成因 | 处置 |
|---|---|---|---|
| `fmt`（退出码 1） | `launch_wire.rs` / `doc_claim_registry.rs` 多处 `Diff in …` | 我手写的 Rust 没过 rustfmt（`K31` 禁止在宿主上跑 `cargo fmt`，只能照门禁印的 diff 手改） | 把两处手写遍历换成既有原语 `guard_core::scan_tree!`，链式调用逐个展开 |
| `cargo`（退出码 101）`1465 passed; 2 failed` | ① `doc_claim_registry::every_repo_path_named_in_the_docs_still_resolves` ② `needle_anchor_registry::bare_contains_on_disk_corpora_only_goes_down`（34 > 棘轮 33） | ① 我在 `INVARIANTS.md` 里点名了 `evidence/K-R105-ruler.py` 而它**没进 git** ⇒ 文档指着一个 `git ls-files` 找不到的路径；② 我新写的反向锚点用了 `launch_rs.contains("…")` —— **裸子串够语料，正是那条棘轮在治的族** | ① `git add` 那份量具；② 换成 `guard_core::pin_line`（钉整行，本仓对这一形的规定形） |

🔴 **这两红都是「判据逮到了我」，不是噪音**：②尤其 —— 我在一条「治匹配单位比事实小」的模块旁边，
自己写了一处裸子串。**如实登记，不写成「顺手修了」。**

第四趟 `M1`（`M1d`）：`GATE: OK —— 13 格全绿`
`hooks 11 · fmt 1 · fmt-daemon 1 · winchk 1 · cargo 1599 · generated ok · daemon 745 · npm 1722 · e2e 12/8/46/45 · pb check FAIL=0 BROKEN=0`
**加固之后**（`D4-selffix` 逼出来的那两处，见下）重跑 `M1e`：**同样 13 格全绿，cargo 仍 1599。**
⇒ **cargo 1598 → 1599**：本件只加了**一条**新 `#[test]`（`the_three_questions_in_33b_have_todays_answers`）；
另外三处是**加在既有判据里的段**，不涨条数。

## 逐刀读数（**一趟一刀**，逐刀 `git status --porcelain` 自证，逐刀 `restore` 后再切下一刀）

刀具 `evidence/K-R105-cut.py`（`ROOT` = 它所在的那棵树）。
每处编辑**先断言锚点恰好命中 N 次，差一次 `exit 3`、一个字节都不落地**；
同一份文件的全部替换**在内存里做完再写一次盘**；写完**回读逐处核对**；
`restore` 是**重写原文**（不是 `cp -a`，mtime 必变）。
🔴 **fail-closed 当场兑现过一次**：整词加固之后 `TS_FALLBACK_REACH` 从 9 处变 10 处，
`D2-unregister` **拒跑并原样退出**（`❌ … 锚点命中 10 次，期望 9 —— 锚点漂了，整刀不落地`）。

| # | 刀 | 门禁 | 判定行 | 红的判据（逐条） |
|---|---|---|---|---|
| 1 | `D1-behave1` | `FAIL —— cargo(101)` | `1465 passed; 2 failed` | `doc_claim_registry::the_three_questions_in_33b_have_todays_answers`：「文档里写着 `"〔现打①〕部分切"`，现打是 `"〔现打①〕全切"`」· `launch_wire::…::the_two_reasons_u8c3_cannot_delete_the_ts_renderer_still_hold`（既有）|
| 2 | `D1-behave3` | `FAIL —— cargo(101)` | `1465 passed; 2 failed` | 同上第一条：「文档里写着 `"〔现打③〕已退役"`，现打是 `"〔现打③〕那一档还在"`」· `launch_wire::…::the_ts_fallback_renderer_now_stands_on_its_own_consumers`（既有回潮闸）|
| 3 | `D1-say` | `FAIL —— cargo(101)` | `1466 passed; 1 failed` | **只红 1 条**：同上第一条，方向相反 |
| 4 | `D2-say` | `FAIL —— cargo(101)` | `1466 passed; 1 failed` | **只红 1 条**：`…now_stands_on_its_own_consumers`「`src/remote-launch.ts` 的**尺子B** 读数是 `OffProductionPath`，登记的是 `OnProductionPath`」|
| 5 | `D2-behave` | `FAIL —— cargo(101)` | `1466 passed; 1 failed` | **只红 1 条**：同上，方向相反（读数 `On`、登记 `Off`）|
| 6 | `D2-unregister`（**加固前**）| `FAIL —— fmt(1)；cargo(101)` | `1466 passed; 1 failed` | ⚠ 只红 `doc_claim_registry::every_code_symbol_named_in_the_docs_still_resolves` —— **不是那条墓碑**。见下「加固二」 |
| 6′ | `D2-unregister`（**加固后**）| `FAIL —— fmt(1)；cargo(101)` | `1465 passed; 2 failed` | `launch_wire::…::the_retired_premise_left_a_tombstone_that_is_still_on_the_board`：「`TS_FALLBACK_REACH` 不在本文件里了 —— **`U8c-3` 的那份换人手续被撕掉了**」＋ 上面那条。⚠ `fmt` 是**改名的副作用**（行宽变了），不是牙 |
| 7 | `D4-orphan` | `FAIL —— cargo(101)` | `1466 passed; 1 failed` | **只红 1 条 · 最小面**：`daemon_kill::…::no_creation_path_can_mint_a_name_the_main_path_cannot_kill`（本件新增的 ③b 段）：「校验器 `src/shell-quote.ts` **今天没有任何创建路径指着它**」|
| 8 | `D4-droppath` | `FAIL —— cargo(101)；npm(1)；ccm-cli(PASS=41<46)` | cargo `1463 passed; 4 failed` | ① ③b 段（校验器落单）· ② `…now_stands_on_its_own_consumers`（承接方文件没了）· ③ `doc_claim_registry::each_registered_status_still_matches_reality`（§33b 状态表 U8c-3 还写「待做」）· ④ `doc_claim_registry::the_outer_layer_producers_are_in_the_state_the_doc_claims`。⚠ npm / e2e 两格是**编译不过的连带**（`launch-render-fallback.ts` 还 import 它）⇒ **按 CRASH 记，不算牙** |
| 9 | `D4-selffix`（**加固前**）| 🔴 **`GATE: OK —— 13 格全绿`** | — | **一条都没红。见下「加固一」** |
| 9′ | `D4-selffix`（**加固后**）| `FAIL —— cargo(101)` | `1466 passed; 1 failed` | **只红 1 条**：`launch_wire::…::the_byte_for_byte_parity_still_has_two_independent_sides`：「对拍函数体里 `let want` 出现了 2 次（只许 1 次）」|
| 10 | `7u` | `FAIL —— cargo(101)` | `1466 passed; 1 failed`；daemon / npm / e2e **全绿** | **只红 1 条**：`the_three_questions_in_33b_have_todays_answers`（走「一个判词都没有」那个分支）|

## 🔴 加固一：`D4-selffix` 第一趟**全绿** —— 本轮最贵的一个读数

**刀**：在 `launch_payload_parity.rs` 的 `if got != want {` **前面插一行** `let want = got.clone();`。
原来那一行 `let want = c.payload.clone();` **一个字节没动**。
⇒ 逐字节对拍变成 `got != got`，**永远相等，永远绿**。

**为什么没人喊**：`the_byte_for_byte_parity_still_has_two_independent_sides` 的 ② 段量的是
`PARITY_SRC.contains("let want = c.payload")` —— **子串存在性**。那一行还在，所以它绿。

★ **这是 `needle_anchor_registry` 治的那一族（匹配单位比事实小）长在那条判据自己身上**：
事实是「`want` 只许被绑一次」，它量的是「那一行出现过没有」。
⚠ 它头注的诚实边界写着「按文本判 ⇒ 换个等价写法躲得过，**防顺手不防决心**」——
**同名遮蔽恰恰是顺手**（让一条挡路的对拍过去，最省事的写法就是它），那句边界把这一形放错了边。

⇒ **处置**：匹配单位从「有没有」换成「**有几处**」——切出那条对拍的函数体（带长度地板防空转），
断言 `let want` 与 `let got` **各恰好 1 次**。复跑 `D4-selffix` ⇒ **只红这一条**（第 9′ 行）。

⚠ **与 `K-R89` 那次的关系**：`K-R89` 的 `D4-selffix` 是**替换**左边（`let want = got.clone();` 顶掉原行），
本轮这一刀是**遮蔽**（原行留着）。同一个语义、两种写法，而那条判据**只挡得住第一种**。
「同一把刀换个写法就活了」正是 `brief` 第 10 条要的那个复核。

## 🔴 加固二：`D2-unregister` 第一趟没红到墓碑

墓碑 `the_retired_premise_left_a_tombstone_that_is_still_on_the_board` 原来只看着
`TS_FALLBACK_KEEPERS`（尺子A），而尺子B 那张表**不在它的人群里** ——
第一趟只有 `every_code_symbol_named_in_the_docs_still_resolves` 红（因为我在 `INVARIANTS.md` 里点了那个符号的名）。
⇒ **那不是「量具受保护」，那是「碰巧文档提过它」。**

⇒ 处置两条：① 把尺子B 加进墓碑的人群；② 墓碑的命中判据从 `contains` 换成
`guard_core::contains_word`（**整词**）—— 实测：只加 ① 时，把表名**加个后缀**（拿走它最省事的写法）
裸 `contains` **照样命中**，needle 被撑大就溜过去了。两条都上之后，第 6′ 行那一红才出来。

## `git status --porcelain` 三处（收工时）

- **工作树**（`.claude/worktrees/k-r105`）：见件文件 `§8`；每一刀 `restore` 之后都回到同一张单子。
- **代码仓主树**：本件一个字节没碰。
- **计划仓**：件文件 `§3`/`§8` ＋ 本文件（本文件住代码仓 `evidence/`，不是计划仓）。

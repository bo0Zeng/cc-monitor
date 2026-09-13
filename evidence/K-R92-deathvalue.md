# K-R92 死值验留档

〔实现方 C 阶段填。**全部在沙箱里跑**（`K31`/`R15`）：
`PB_WS=backend-consolidation .claude/devbox/gate <工作树> k-r92`，一趟一刀，跑完原样还原。〕

## 0 量具与口径

- **变异台架**：`run.sh <标签> <变异脚本>` —— 落变异 → 跑整趟门禁 → 抓失败名单 → **用落刀前的
  原件覆盖回去**（`cp -a`，不走 `git checkout <base> -- <file>`）→ `git status` 自证还原干净。
  每一刀的门禁原文落在 scratchpad 的 `gate-<标签>.txt`。
- **判红判绿的口径**：整趟门禁的 `GATE:` 那一行 ＋ `failures:` 段里被点名的判据。
  ⚠ **只看「门禁红了」不算数** —— 要看**红的是哪一条**：红的若不是本刀要验的那一条，
  这一刀就没验到（只是撞到了别的判据）。下面每一刀都点名。
- **基线（`M0`）**：`GATE: OK —— 13 格全绿`。cargo **1560** · npm **1697** · daemon **699** ·
  hooks 11 · e2e 12/8/46/45 · fmt/fmt-daemon/winchk/generated 绿 · pb check FAIL=0 BROKEN=0。
  **量于提交 `e707acd`**（分支 `track/k-r92`，基点 `f1f89f5`）。

## 1 `KR92D1` 的四刀

### 刀 ① 把 `Unknown` 压成 `0` 过线 ⇒ **必须红** —— 实测**红**

变异（`M1`）：`remote_history::fanout_list_projects` 里那两行

```
starred_count: counts.starred.known(),      →  starred_count: Some(counts.starred.known().unwrap_or(0)),
hidden_count:  counts.hidden.known(),       →  hidden_count:  Some(counts.hidden.known().unwrap_or(0)),
```

读数：`GATE: FAIL —— cargo（退出码 101）`；`test result: FAILED. 1439 passed; 1 failed`。
点名：**`remote_history::kr83_tests::an_unknown_count_reaches_the_wire_as_unknown_not_as_zero`**
（`src/remote_history.rs:1336` 那条 `starred_count == None`）。

⚠ **这一刀专门砍在「整条路」上**：`history::tests::the_three_counts_can_say_i_do_not_know`
判的是**类型装不装得下**第三态（它直接造一个 `HistoryProject`），砍这一刀它**不会红** ——
09-12 那半修正是「类型分得开、过线那一步自己丢掉」。两条缺一条都有一整类漏网。

### 刀 ② 换一种等价表示 ⇒ **必须绿** —— 实测**那两条性质判据绿**

变异（`M3`）：给 `HistoryProject::starred_count` 加 `#[serde(rename = "starCount")]`
（＝「换字段名 / 换编码」那一档）。

读数：`GATE: FAIL —— cargo；generated`，而 cargo 的 `failures:` 段**只有一条**：
`history::tests::history_project_camel_case_contract`。

⇒ 逐字结论：
- **本刀要验的那两条判据一条都没红**（现打 `grep -c` 失败名单 = **0**）：
  `an_unknown_count_reaches_the_wire_as_unknown_not_as_zero` ·
  `the_three_counts_can_say_i_do_not_know`。**它们不判名字，只判「分不分得开」。**
- 红的那条是**专门判名字的**（`P1.2` camelCase 契约，`K-R92` 之前就在），
  与 `generated`（改了 `ts_rs` 类型必须重跑生成）—— **两条都是「本来就该红」，不是本刀的靶。**

### 刀 ③ 只修 star/hide、不修 `has_live` ⇒ **必须红** —— 实测**红**

变异（`M2`）：**只**把 `has_live` 那一行压回去（star/hide 一字不动）

```
has_live: counts.has_live.known(),  →  has_live: Some(counts.has_live.known().unwrap_or(false)),
```

读数：`GATE: FAIL —— cargo（退出码 101）`；`1439 passed; 1 failed`。
点名：同一条判据，**红在另一行**（`src/remote_history.rs:1342`，`has_live == None` 那条断言）
—— 与刀 ① 红的行号不同，证明**三格是逐格断的**，不是一条断言蒙混过关。

### 刀 ④ TS 侧把「不知道」当 `0` 参与排序或求和 ⇒ **必须红** —— 实测**红**

变异（`M4`）：`src/views/history.ts` 两处一起改回去 ——
排序回 `Number(b.hasLive) - Number(a.hasLive)` ＋ `(a.starredCount as number) > 0`；
加减回 `proj.starredCount = (proj.starredCount as number) + 1`。

读数：`GATE: FAIL —— npm（退出码 1）`，`Test Files 2 failed | 124 passed`。
点名 `src/views/history-counted.vitest.ts` **三条**（逐条带实测差值）：
- 排序·活：`expected [ '确定活着', '确定没活', '不知道' ] to deeply equal [ '确定活着', '不知道', '确定没活' ]`
- 排序·星标：`expected [ '查过了没星标', '星标不知道' ] to deeply equal [ '星标不知道', '查过了没星标' ]`
- 求和：`求和：一次 star 操作不许把「不知道」变成 1`

⚠ **如实记一处附带红**：`src/eslint-baseline.vitest.ts` 也红了
（`eslint 全仓错误数变了：实测 9，基线 7`）—— 那是**本刀为了让变异编译得过而加的两个 `as number`**
撞上的 eslint 基线，**不是** `KR92D1` 的判据。写在这里免得下一个人把它读成「④ 由 eslint 守着」。

⚠ **为什么第 ④ 刀要打在 `history-counted.vitest.ts`（真 `HistoryView` 实例）上**：
`counted.vitest.ts` 判的是读法口（`liveRank`/`starRank`/`bumpCounted`）本身。
把 `history.ts` 改回去，**那一份照样全绿** —— 夹具绿了、被测对象没人管。
本刀的实测正是这个形状的反证：红的四条里有三条来自那份 DOM 判据，`counted.vitest.ts` 一条没红。

## 2 `KR92D2` 的三刀

### 刀 ① 恢复写死 ⇒ **必须红** —— 实测**红（两条判据一起红）**

变异（`M5`）：`remote_history::remote_session_entry` 里 `is_live: live,` → `is_live: Some(false),`
（＝`〔R83c〕` 那处 `is_live: false` 的今天形态）。

读数：`GATE: FAIL —— cargo（退出码 101）`，`failures:` 两条：
- **`the_streamed_remote_entry_does_not_hardcode_its_liveness`**（行为侧，`:1451`）
- **`there_is_no_place_left_that_flattens_unknown_into_a_wire_value`**（源扫描侧，`:1568`），
  报错原文：`🔴 生产代码里又出现了 is_live: Some(false)`

### 刀 ② 真值 ⇒ **绿** —— 由基线那一趟兑现

`M0`（13 格全绿）里 `the_streamed_remote_entry_does_not_hardcode_its_liveness` 与
`a_real_oracle_makes_liveness_a_real_value_all_the_way_to_the_wire` 两条**同时**断着：
喂 `Oracle(&["s1"])` ⇒ `is_live == Some(true)`；喂 `Oracle(&[])` ⇒ `Some(false)`；
喂生产绑定 `NoLivenessOracleYet` ⇒ `None`，且 `Some(false) != None`。
⇒ **这条路端得动真值**，「答不出」与「查过了没活」是两个不同的答案。

### 刀 ③ 算不出时退化成 `false` ⇒ **必须红** —— 实测**红**

变异（`M6`）：`let live = liveness.is_live(origin, &session_id).known();`
→ `... .known().or(Some(false));`（**不写字面量**，专门绕开源扫描那一半）

读数：`GATE: FAIL —— fmt；cargo（退出码 101）`，`failures:` **一条**：
`the_streamed_remote_entry_does_not_hardcode_its_liveness`。

★ **这一刀最值钱的读数是「源扫描那一条没红」**：它证明了那两条判据**不是重复的** ——
子串扫描守「写死的字面量」，行为判据守「退化」，**只留一条就有一整类漏网**。
（`fmt` 那一格是变异行超了行宽，附带红，不是靶。）

## 3 `KR92D3` 的两刀

### 刀 ① 转了而 `readers` 断言没跟 ⇒ **必须红** —— 实测**红**

变异（`M7`）：登记表那条已转成 `reader`，把断言改回 `readers, 8`。

读数：`GATE: FAIL —— cargo（退出码 101）`，`failures:` **一条**：
`local_read_surface_registry::tests::every_reader_names_its_retirement_owner`，
报错原文带着棘轮的账：`` `reader` 条数变了（**实测 9 条** …… 这个数就是 F10 的真实工作面``。

### 刀 ② 两边都跟 ⇒ **绿** —— 由基线那一趟兑现（`M0` 13 格全绿）。

## 4 还原自证

七刀每一刀跑完都用落刀前的原件 `cp -a` 覆盖回去，`git status --short` 只剩
`?? evidence/K-R92-deathvalue.md`（本文件，当时还没提交）。
⚠ **`M3` 那一刀例外，如实记**：它改了带 `ts_rs::TS` 的类型 ⇒ 门禁跑 `cargo test` 时
**顺手把 `src/generated/HistoryProject.ts` 重写了**，`cp -a` 覆盖 Rust 源覆盖不到它。
已单独 `git checkout -- src/generated/HistoryProject.ts` 还原，之后 `git status` 干净。
（记这一条是因为它是一类真陷阱：**变异的副产物可以落在变异脚本没碰过的文件上**。）

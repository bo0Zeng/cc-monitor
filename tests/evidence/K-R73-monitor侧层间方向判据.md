# `K-R73` · monitor 侧「谁能引用谁」—— 逐刀读数

> 量于工作树 `/home/zbl/文档/claudecode-frontend/.claude/worktrees/k-r73`，分支 `track/k-r73`，基点 `841d297`。
> **全部读数在沙箱里打**（`DECISIONS.md#R21`：`cargo test` 执行被测代码 ⇒ 一律沙箱）。
> 量具：`<本会话 scratchpad>/kr73-box`（挂载/隔离与 `.claude/devbox/gate` 逐条相同，
> 被测对象恒指 `k-r73` 这棵树、target 恒是 `.claude/pm-targets/k-r73`）＋
> `<本会话 scratchpad>/mut.py`（逐字锚点替换，每处先断 `count()==1`，备份一律落 scratchpad）。
> ⚠ **量具住会话 scratchpad，不随本文件发布** —— 复跑请照 §六 重建。

---

## 一 · 变异台的分母

| 项 | 读数 |
|---|---|
| 变异台命令 | `cargo test -p monitor --lib`（沙箱内，`CARGO_TARGET_DIR=.claude/pm-targets/k-r73`） |
| **基线**判定行（基点 `841d297`，动手之前现打） | `test result: ok. 1397 passed; 0 failed; 10 ignored; 0 measured; 0 filtered out` |
| **交付态**判定行 | `test result: ok. 1403 passed; 0 failed; 10 ignored; 0 measured; 0 filtered out` |
| 判定行条数 | **1**（单包单 target ⇒ 只有一行 `test result`） |
| 新增判据 | **+6**（`1397 → 1403`）—— 逐条买到什么见 §五 |
| 为什么用它而不是整趟门禁 | 三条 dod 的靶全在 `-p monitor --lib` 这一格里（`backend::layering` · `doc_claim_registry::tests`）。⚠ **它盖不到** daemon / npm / e2e 三格 —— 那三格由收工那趟整门禁盖（件文件 `§8` 四格数字） |

🔴 **判定行纪律**：下面每一刀都核过「判定行仍是 1 行」。**编译错一律按 CRASH 单列，不进判定行读数** ——
本轮 CRASH **0 次**（见 §四）。

---

## 二 · 逐刀（每刀：逐字锚点 · `count()==1` 断言 · 判定行读数 · 最小面）

### 刀 M1 —— `KR73D1` 死值验 ①：在 `control/` 下加一处引用 `observe::`

- **切在哪**：`src-tauri/src/backend/control/daemon_route.rs`。
- **逐字锚点**：`pub(crate) fn no_channel(origin: &str) -> Routed {` —— **命中恰好 1 次**（实打 `1`）。
- **换成**：在它前面插一个**真能编过**的反向引用（不是注释、不是字符串）：

  ```rust
  #[allow(dead_code)]
  pub(crate) fn kr73_reverse_probe(t: &str) -> bool {
      matches!(
          crate::backend::observe::local_query::run_query(t, &["--x"]),
          crate::backend::observe::local_query::QueryOutcome::NoBackend(_)
      )
  }
  ```

- **变异已落地**：新形态 `count()==1`，实打 `1`。
- **判定行读数**：`test result: FAILED. 1402 passed; 1 failed; 10 ignored`（判定行 **1** 行，与基线同）。
- **最小面**：**红 1 条**，逐字 `backend::layering::control_layer_must_not_reference_observe`。
- **报错原文（沙箱现打，逐字）**：

  ```
  control/ 引用了 observe/（反向不许）：
    control/daemon_route.rs → crate::backend::observe::local_query::QueryOutcome::NoBackend
    control/daemon_route.rs → crate::backend::observe::local_query::run_query
  ```

- ⚠ **它逮到了两个符号而不是一个**：函数与类型各一条。那是刻意的（daemon 那份逐字
  「**类型也要登记，不只是函数**」）——「接口只算函数」是个会腐的口径。

### 刀 M2 —— `KR73D1` 死值验 ②：在 `observe/` 下加一处**没登记**的 `control::` 引用

- **切在哪**：`src-tauri/src/backend/observe/local_query.rs`。
- **逐字锚点**：`    let bin: PathBuf =` —— **命中恰好 1 次**。
- **换成**：在它前面插 `let _ = crate::backend::control::daemon_route::no_channel("kr73");`（真能编过）。
- **判定行读数**：`test result: FAILED. 1402 passed; 1 failed; 10 ignored`（判定行 **1** 行）。
- **最小面**：**红 1 条**，逐字 `backend::layering::observe_to_control_interface_is_exactly_the_registered_set`。
- **报错原文（逐字，两边都印出来了）**：

  ```
    left: ["crate::backend::control::daemon_route::no_channel",
           "crate::backend::control::local_backend::Resolved::Found",
           "crate::backend::control::local_backend::Resolved::Missing",
           "crate::backend::control::local_backend::resolve_beside_this_exe"]
   right: ["crate::backend::control::local_backend::Resolved::Found",
           "crate::backend::control::local_backend::Resolved::Missing",
           "crate::backend::control::local_backend::resolve_beside_this_exe"]
  ```

- 🔴 **这段读数顺带证明了一件本件最要紧的事**：`right`（登记表）**非空，3 条** ⇒
  这条 `assert_eq!` **不是** `[] == []` 的空真。**正向这一半是真钉住的，不是「随便引」。**
  （盘上那 3 条边全部出自 `K-R71` 归位时显形的那一处引用：一次函数调用 + 它返回类型的两个变体。）

### 刀 M2b —— `KR73D1` 正向的**另一个方向**：登记表里少一条

- **切在哪**：`src-tauri/src/backend/mod.rs` 的 `ALLOWED_OBSERVE_TO_CONTROL`。
- **逐字锚点**：`Resolved::Missing` 那一整条（含理由串）—— **命中恰好 1 次**。
- **换成**：删掉（表从 3 条变 2 条，**盘上的边一条没动**）。
- **判定行读数**：`test result: FAILED. 1402 passed; 1 failed; 10 ignored`。
- **最小面**：**红 1 条**，同 M2 那条。
- ⇒ **「多一条」与「少一条」两个方向都有牙** —— 登记表腐烂（边没了却留着登记）也会被逮到。

### 刀 M2c —— `KR73D1` 的「条数钉死」不许被模块级登记洗掉

- **切在哪**：同一张表，`crate::backend::control::local_backend::resolve_beside_this_exe` 那一条。
- **逐字锚点**：该字符串 —— **命中恰好 1 次**。
- **换成**：`crate::backend::control::local_backend`（**模块级**）。
- **判定行读数**：`test result: FAILED. 3 passed; 1 failed`（`backend::layering` 过滤跑，判定行 **1** 行）。
- **最小面**：**红 1 条**，同上。报错逐字：
  「登记项 `crate::backend::control::local_backend` 只钉到**模块级** —— 那等于把整个模块的接口面都放开，而条数看不出区别。」
- ⇒ 挡的是**下一个人「修红」最省事的那条路**：把模块名加进表里，从此那个模块的任意符号都能被读面调，而条数仍是「3 条」。

### 刀 M3 —— `KR73D2` 死值验：建一个**空的** `backend/platform/` 目录

- **切在哪**：盘上 `mkdir -p src-tauri/src/backend/platform`（**一个文件都不放**，连 `mod.rs` 都没有）。
- **变异已落地**：`ls src-tauri/src/backend/` 实打 `control  mod.rs  observe  platform`。
- **判定行读数**：`test result: FAILED. 1402 passed; 1 failed; 10 ignored`（判定行 **1** 行）。
- **最小面**：**红 1 条**，逐字 `doc_claim_registry::tests::each_registered_status_still_matches_reality`。
  报错原文（逐字）：「`…/backend/platform` 建出来了，可里面一个住户都没有（只有 `mod.rs` 也算没有）。」
- ⚠ **为什么最小面是 1 而不是 3**：**刻意建的是一个不含任何 `.rs` 的目录**。
  若放一份 `platform/mod.rs` 进去，`every_file_under_backend_is_registered_with_a_reason` 与
  `every_file_under_backend_lives_on_a_capability_line` 会同红（那两条只看 `.rs`）——
  **那不是本条的牙，是别人的**。要把本条**单独**证到，标的必须是「目录在、`.rs` 一个也没有」。

### 🔴 刀 M3′ —— `KR73D2` 真正的差分：**同一个空目录 + 把文档那一格翻成「已交付」**

件计划的死值验字面是「建空目录 ⇒ 必须红」。**如实说：那一刀在基线上也是红的**
（裸 `is_dir()` 翻 `true`、文档说「待做」⇒ `assert_eq!` 不等 ⇒ 红）。
⇒ 光那一刀**证不出本件买到了什么**。真正的差分要把「下一个人会怎么修红」也放进去：
他不会删目录，他会**顺手把文档那一格改成「已交付」**。

| 态 | 量法 | `doc/ARCHITECTURE.md` `platform/` 那格 | 盘上 | 判定行 | 红 |
|---|---|---|---|---|---|
| **基线形态** | `…/backend/platform").is_dir()`（逐字退回原文） | `**已交付** —— 目录建起来了（见 2.4）` | 空目录 | `test result: ok. 19 passed; 0 failed`（`doc_claim_registry` 过滤跑） | **0** |
| **交付形态** | `a_capability_line_has_landed(&root, "platform")` | 同上，一字不改 | 同上 | `test result: FAILED. 1402 passed; 1 failed; 10 ignored` | **1** |

- 两态**唯一的差别是那一行量法**（文档与盘上的变异逐字相同）。
- 🔴 **基线那一格「一条不红」就是 `R29` 裁定二说的那笔债的现值**：
  一个空壳目录 ＋ 一次顺手改文档 = 全绿，与 `K-R71` 的 `7u` 逮到的那一形**逐字同族**。
- **最小面 1**，逐字 `doc_claim_registry::tests::each_registered_status_still_matches_reality`。

### 刀 M4 —— `KR73D2` 失效方向：把量法改成**恒 `false`**

- **切在哪**：`src-tauri/src/doc_claim_registry.rs` 的 `layer_has_landed_at`。
- **逐字锚点**：`fn layer_has_landed_at(dir: &Path) -> bool {\n        if !dir.is_dir() {\n            return false;\n        }` —— **命中恰好 1 次**。
- **换成**：在后面再插一段 `if dir.is_dir() { return false; }`（**恒 `false`**，签名与头注一字未动）。
- **判定行读数**：`test result: FAILED. 1402 passed; 1 failed; 10 ignored`（判定行 **1** 行）。
- **最小面**：**红 1 条**，逐字 `doc_claim_registry::tests::the_capability_line_landing_probe_actually_bites`。
- ⇒ **「两个方向都要有刀」的第二把刀落地了**：恒 `false` 会让那两格**永远说「没落地」**，
  真落地那天同样没人红 —— 现在它当场红在那条**活体非空对照**上（拿盘上真的 `backend/control/` 量，必须为 `true`）。

### 刀 M4b —— `KR73D2` 反方向：改成**恒 `true`**

- **切在哪**：同一处，`return false;` 换成 `return true;`（目录不存在时也说「落地了」）。
- **判定行读数**：`test result: FAILED. 1401 passed; 2 failed; 10 ignored`。
- **最小面**：**红 2 条** —— `each_registered_status_still_matches_reality` ＋
  `the_capability_line_landing_probe_actually_bites`。
- ⚠ **2 条是结构上躲不开的，不是粗刀**：恒 `true` 同时打破「两格今天该是 `false`」（前者）
  与自检的「目录不存在 ⇒ `false`」那一格（后者）。**只红一条的形，在这个变异下不存在。**

### 刀 M5 —— `KR73D3` 的棘轮：「对不上」那一栏涨一条

- **切在哪**：`MEASURE_CENSUS` 里 `monitor-backend-common-landed` 那行的 `Verdict::Holds`。
- **逐字锚点**：那三行（键名 + 形状 + `Verdict::Holds`）—— **命中恰好 1 次**。
- **换成**：`Verdict::FallsShort`（5 → 6）。
- **判定行读数**：`test result: FAILED. 18 passed; 1 failed`（`doc_claim_registry` 过滤跑）。
- **最小面**：**红 1 条**，逐字 `doc_claim_registry::tests::every_status_cell_measure_is_in_the_census`。
  报错**逐条点名**了那 6 条（不是只报一个数）。

### 刀 M5b —— `KR73D3` 的两向对拍：普查表少一行

- **切在哪**：`MEASURE_CENSUS` 里 `ccm-invocation-kernel-exists` 那一整条（237 字节）。
- **逐字锚点**：那一整条 —— **命中恰好 1 次**。
- **换成**：删掉。
- **判定行读数**：`test result: FAILED. 18 passed; 1 failed`。
- **最小面**：**红 1 条**，同上。报错把 `left`（10 个量法键）与 `right`（9 个）**逐个印出来**。
- ⇒ 「`STATUS_CELLS` 新加一格而普查表没跟」这一形有闸了 —— 那正是 `R29` 裁定零那次的形状
  （三格裸 `is_dir()` 是**跟着上一格一起写下去的**，没有一格被单独问过「你量的是落地还是有个目录」）。

### 🔴 刀 M6 —— `KR73D3` 普查里那条**今天就能演示**的：把 TS 那两处调用整行注释掉

- **切在哪**：`src/remote-launch-run.ts`，含 `commands.render_launch_payload(` /
  `commands.render_ccm_launch(` 的行，**逐行加 `// ` 前缀**。
- **变异已落地**：注释掉 **3** 行；剩余**未注释**的调用实打 **0**。
- **判定行读数（本模块过滤跑）**：`test result: ok. 19 passed; 0 failed`
  ⇒ 🔴 **`doc_claim_registry` 那一格照样绿** —— 它用 `read()` 而不是 `prod()`，**不剥 TS 注释**。
- **判定行读数（全量）**：`test result: FAILED. 1402 passed; 1 failed; 10 ignored`。
- **最小面**：**红 1 条**，逐字
  `backend::control::launch_wire::f07_main_path_tests::the_remote_launch_main_path_really_calls_the_backend_renderers`。
- ⚠ **诚实修正，别把这条读成「那个性质没人守」**：守它的是 `launch_wire.rs` 那条生产接线钉
  （走 `production_ts()`，行首与行尾注释都剥，头注逐字「行尾注释里的提及不算数」）。
  ⇒ `doc_claim_registry` 那一格是**同一个事实的第二份、而且更弱的那一份**。
  它的害处不是漏守，是**让人以为这一格自己有牙**。这一条已逐字写进普查表那一行。

---

## 三 · `7u` —— 把本件交出去的三样东西整个退掉（逐处掏空，签名留着）

🔴 **没有用 `git checkout <基线> -- <文件>`**（判据与被判对象同住两个文件，checkout 会把别人的东西一起退掉）。
退法是**逐处掏空**：每一处先断 `count()==1` 再替换，**函数名 · 常量表 · 头注 · 文档里那几句声称，一个字节都没动**。

### 「实现」与「签名」在本件里分别是什么

**实现 = 三个判定核**：① 层间引用的抽取与两条方向断言；② 能力线落地量法；③ 普查表的两向对拍与棘轮。
**签名 = 它们在盘上「看起来还在」的一切**：`mod layering` 与它的 4 个 `#[test]`、
`ALLOWED_OBSERVE_TO_CONTROL`（3 条）、`MEASURE_CENSUS`（10 行）、`FALLS_SHORT_CEILING`、
`doc/ARCHITECTURE.md` 新写的那两段「现在有人管了」、`local_query.rs` 那段「这条边有登记的家了」。

| 处 | 掏空动作 | 锚点命中 |
|---|---|---|
| ① `refs_to_layer` | 体换成 `Vec::new()`，签名 + 头注留着 | 1 |
| ② `violating_edges` | 体换成 `Vec::new()` | 1 |
| ③ 正向那条 | `assert_eq!(found, want, …)` 换成 `assert!(found.len() < 10_000)` —— **`KR73D1` 逐字点名的失效方向「正向写成随便引」** | 1 |
| ④ `the_backend_layer_scan_actually_bites` | 体掏空 | 1 |
| ⑤ `the_backend_direction_judgments_bite_on_a_live_tree` | 体掏空 | 1 |
| ⑥ `layer_has_landed_at` | 恒 `false` —— **`KR73D2` 逐字点名的失效方向** | 1 |
| ⑦ `the_capability_line_landing_probe_actually_bites` | 体掏空 | 1 |
| ⑧ 普查的两向对拍 + 棘轮 | 换成 `assert!(MEASURE_CENSUS.len() >= 1)` —— **`KR73D3` 逐字点名的失效方向「报一个总数而不逐条点名」** | 1 |
| ⑨ `doc/ARCHITECTURE.md` · `local_query.rs` 的声称 | **刻意不退** —— 那就是签名 | — |

### 读数

```
test result: ok. 1403 passed; 0 failed; 10 ignored; 0 measured; 0 filtered out
```

## 🔴 **仍绿：全部。0 红。** 照实报，没有去凑。

三条失效方向**同时**成立、文档继续宣称「现在有人管了」——
`-p monitor --lib` **1403 条一条都不红**，判据条数**一格没少**（掏的是体，不是签名）。

### 分方向的单刀读数（各自单独掏、其余不动）

| 单独掏空 | 判定行 | 红 |
|---|---|---|
| ③ 正向写成「随便引」（`KR73D1` 的失效方向） | `test result: ok. 1403 passed; 0 failed; 10 ignored` | **0** |
| ⑧ 普查只报一个总数（`KR73D3` 的失效方向） | `test result: ok. 1403 passed; 0 failed; 10 ignored` | **0** |
| ⑥ 量法恒 `false`（`KR73D2` 的失效方向） | `test result: FAILED. 1402 passed; 1 failed; 10 ignored` | **1**（刀 M4） |

⇒ **三条 dod 的失效方向里，只有 `KR73D2` 那条今天有闸**（因为本件给它配了活体非空对照）。
`KR73D1` 正向与 `KR73D3` 那两条**被削掉之后没有任何东西会红**。

### 它的牙在哪一刀 —— 逐条

| 仍绿的那一族 | 掏空态为什么仍绿 · 它的牙在哪 |
|---|---|
| 本件那 6 条新判据本身 | 掏空态里**它们就是被测对象**，不是量具 ⇒ 「仍绿」按定义成立。它们的牙在 M1 · M2 · M2b · M2c · M3 · M3′ · M4 · M4b · M5 · M5b 这十刀上，逐条有最小面 |
| `scanning_guard_registry::every_registry_guard_keeps_its_reverse_half` | 它按 `TABLE_DECLS` 这个**闭集、按名字**认「带登记表的判据」，成员只有 `const REGISTERED:` / `SITES:` / `SCHEDULING_SITES:` / `FORMS:`。`ALLOWED_OBSERVE_TO_CONTROL` 与 `MEASURE_CENSUS` **从来没进过它的人群** ⇒ 它对本件的表**一个字都没说过**。它的牙在那四个名字命中的那些文件上（`K-R33` 头注逐字记着这一形：「登记表根本没去找它」，默认结局是恒绿） |
| `scanning_guard_registry::no_new_guard_walks_the_tree_without_excluding_itself` | 它管「**新**出现的裸遍历」。`src-tauri/src/backend/mod.rs` 与 `src-tauri/src/doc_claim_registry.rs` **本来就在 `PENDING` 存量清单里**（09-12 现打，两条都在）⇒ 我在这两份文件里加的遍历它按定义看不见。它的牙在「一份**不在清单上**的文件开始裸遍历」那一刀上（`K-R71` 的 M2 证过） |
| `needle_anchor_registry::bare_contains_on_disk_corpora_only_goes_down` | **递减棘轮**：掏空只会让计数变小，结构上不可能红。它的牙在「新增一处语料变量上的裸 `contains("…")`」上。⚠ 顺带对账：本件新写的代码**一处都没有**踩这一族（`find`/`contains` 的实参全是变量，不是字符串字面量），交付态该判据实打绿 |
| `structural_scan::every_symbol_address_in_the_sources_still_resolves` | 掏空**只掏体、不动名字** ⇒ 散文里点名的每一个符号地址照样解析得到。它的牙在「散文点名了一个盘上不存在的符号」上 |
| `doc_claim_registry::tests::each_registered_status_still_matches_reality` | 掏空态里量法恒 `false`，而 `platform/` `common/` 两格文档写的是「待做」⇒ **对上了**。它的牙在 M3 / M3′ / M4b 三刀上（都要文档与现场**对不上**才红） |
| 🔴 `doc/ARCHITECTURE.md` 2.1 / 2.2 本件新写的那两段声称 | **没有任何判据读它们** —— 掏空态里它们就是两句假话（「两侧今天都有机器在管谁能引用谁」「空壳目录直接红」），而全树 1403 条一条不响。⚠ 它落在 `doc_claim_registry` 模块头注**已登记的诚实边界**里（逐字：「「实测句」那一族有 **63** 句、绝大多数是散文，**钉不住**」）⇒ **不是新洞，但也不是「有人管着」**。如实记，交回 PM |

### ⚠ 这一栏买到了什么、没买到什么（别写宽一格）

- **买到**：`KR73D2` 那条失效方向（恒 `false`）**有闸**，而且闸是活体的（拿盘上真的 `control/` 当非空对照）。
- **没买到**：`KR73D1` 正向那条与 `KR73D3` 那条**被削弱时无人出声**。
  两者的形状相同 —— 「判据自己被削弱」这一族，在本仓只有 `every_registry_guard_keeps_its_reverse_half`
  一个接手人，而它按**闭集**认表、认不出本件的两张表。
  ⚠ **要不要把这两张表的名字纳入那个闭集，归 PM 裁**：那个闭集的头注逐字写着
  「这是一个闭集，不是一族形状 —— 往里加一个名字就是在**放宽**一条守卫」，
  住址 `src-tauri/src/scanning_guard_registry.rs` 的 `TABLE_DECLS`。**本件不自批。**
- **没买到**：本护栏只扫 `backend/control/` 与 `backend/observe/` 两棵子树。
  `backend/` **之外**的调用方（`usage.rs` · `local_accounts.rs` 今天都在调 `backend::observe::local_query`）
  **不在人群里**，一个字都看不见。这一条已逐字写进 `refs_to_layer` 的头注。

---

## 四 · CRASH 单列（**不进判定行读数**）

| # | 什么时候 | 现象 | 处置 |
|---|---|---|---|
| — | — | **本轮 0 次** —— 四条新判据首跑即过，两处死值验的插入代码都一次编过 | — |

⚠ 另记两条**不是 CRASH、但值得写下来的量具事故**（都发生在变异台上，与被测代码无关）：

1. **变异台的备份目录里混进了三份不属于本台的 `.orig` 文件**，`restore()` 把它们写进了工作树根
   （`ARCHITECTURE.md.orig` · `mod.rs.orig` · `scanning_guard_registry.rs.orig`）。
   当场 `git status --porcelain` 逮到、立即 `rm`，并给 `restore()` 加了一条白名单
   （只还原 `src-tauri/` · `doc/` · `src/` 打头的路径）。
   🔴 **三份都落在工作树根，不在 `src-tauri/src/` 下** ⇒ 没有触发纪律 ㉑ 那条假红。
2. **`7u` 掏空脚手架自己死循环了两次**：`group_names_layer(group, "")` 里 `find("")` 恒返回 `Some(0)`
   而 `from += layer.len()` 为 0 ⇒ 无限循环，两趟沙箱各挂 10 分钟后被 `docker kill`。
   **是脚手架传了空串，不是被测代码的缺陷**（真实调用点的 `layer` 永远非空）。改传 `"z"` 后正常。

---

## 五 · `+6` 条判据各买到什么（预登记押 `+2 ~ +5`，实打 **+6**，逐条交代）

| # | 判据 | 买到的那条性质 | 少了它会怎样 |
|---|---|---|---|
| 1 | `backend::layering::control_layer_must_not_reference_observe` | **反向零容忍** | `KR73D1` 的一半没了（M1 那一刀无人接） |
| 2 | `backend::layering::observe_to_control_interface_is_exactly_the_registered_set` | **正向逐条列举、条数被等号钉住** | 只剩「没接反」，买不到「接线面被钉住」—— 件计划逐字点名的失效方向 |
| 3 | `backend::layering::the_backend_layer_scan_actually_bites` | **扫描器**认得四种模块级拼法 + 两种成组导入 + 三种符号路径根，且不误伤前缀同名 | daemon 那份的头注用两张表记着：少认一种拼法，判据就是安慰剂（它被证伪过两轮） |
| 4 | `backend::layering::the_backend_direction_judgments_bite_on_a_live_tree` | **判据本身**（走目录 → 剥生产段 → 采集面自检 → 汇总 → 断言，五段原封不动）在真树上会咬人 | 第 1 条今天是 `[] == []`，**闸死了照样绿**；喂字符串只盖到第 3 条那一段 |
| 5 | `doc_claim_registry::tests::the_capability_line_landing_probe_actually_bites` | 落地量法**两个方向都不许恒定**（恒 `false` / 恒 `true` 各有一格接住） | `KR73D2` 逐字点名的失效方向无人接（`7u` 实测：只有它红） |
| 6 | `doc_claim_registry::tests::every_status_cell_measure_is_in_the_census` | 普查表与 `STATUS_CELLS` **两向对拍** + 「对不上」那栏的**递减棘轮** | 新加一格量法而没人判过它量的是不是它声称的那件事 —— `R29` 裁定零那次的形状 |

🔴 **为什么是 6 而不是 5**：第 3 条与第 4 条**买的不是同一样东西**，daemon 那份逐字论证过这一点
（「喂字符串证明的是**扫描器**的牙，不是**判据**的牙」）。把它们合成一条会让「扫描器瞎了」与
「判据空转」两种失效在输出上分不开 —— 而那正是本区反复抓的那一形。**其余四条一条都拆不动。**

---

## 六 · 量具原文（复跑用）

### `kr73-box`（沙箱）

```bash
#!/usr/bin/env bash
# K-R73 专用：在 ccmon-devbox 沙箱里跑任意命令（挂载/隔离与 .claude/devbox/gate 逐条相同）。
# 用法： kr73-box '<bash 命令>'
set -o pipefail
PROJ=/home/zbl/文档/claudecode-frontend
SKILL=/home/zbl/.claude-accts/z/skills/planned-build
WT="$PROJ/.claude/worktrees/k-r73"
TARGETS="$PROJ/.claude/pm-targets"
CARGO_CACHE=ccmon-cargo-registry
exec docker run --rm \
  --network none \
  -v "$PROJ:$PROJ" \
  -v "$SKILL:$SKILL:ro" \
  -v "$CARGO_CACHE:/opt/rust/cargo/registry" \
  -e "CARGO_TARGET_DIR=$TARGETS/k-r73" \
  -e HOME=/home/zbl \
  -e PB_WS=backend-consolidation \
  -w "$WT" \
  ccmon-devbox:latest \
  bash -o pipefail -c "mkdir -p \"\$HOME/.claude/projects\" && $1"
```

⚠ 被测对象恒指 `k-r73` 这棵树（写死在 `WT`），target 恒是 `.claude/pm-targets/k-r73`。
⚠ **别在容器里用 `| head`** —— SIGPIPE 会把 `cargo fmt` 从中间打断，看起来像「格式化跑过了」而其实没写完（本轮踩过一次）。

### `mut.py`（变异台）

```python
#!/usr/bin/env python3
"""K-R73 变异台：逐字锚点替换 + count()==1 断言；备份一律落 scratchpad。"""
import io, os, shutil
WT = "/home/zbl/文档/claudecode-frontend/.claude/worktrees/k-r73"
BK = "<本会话 scratchpad>/bak"
os.makedirs(BK, exist_ok=True)

def _bk(rel):
    return os.path.join(BK, rel.replace("/", "__"))

def apply(rel, old, new, expect=1):
    p = os.path.join(WT, rel)
    s = io.open(p, encoding="utf-8").read()
    n = s.count(old)
    print(f"  锚点命中 {rel}: {n} （要 {expect}）")
    assert n == expect
    b = _bk(rel)
    if not os.path.exists(b):
        shutil.copy2(p, b)
    io.open(p, "w", encoding="utf-8").write(s.replace(old, new))
    print(f"  变异已落地：新形态 count = {io.open(p, encoding='utf-8').read().count(new)}")

def restore():
    for f in os.listdir(BK):
        rel = f.replace("__", "/")
        # 🔴 白名单：目录里混进别的东西时绝不写进工作树（本轮踩过一次，见 §四）
        if not (rel.startswith("src-tauri/") or rel.startswith("doc/") or rel.startswith("src/")):
            continue
        shutil.copy2(os.path.join(BK, f), os.path.join(WT, rel))
        os.remove(os.path.join(BK, f))
```

🔴 **备份与中间产物一律落会话 scratchpad，一个字节都没落进 `src-tauri/src/`**（纪律 ㉑）——
收工前 `git status --porcelain` 已核（见件文件 `§8`）。

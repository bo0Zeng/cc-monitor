# `K-R110` 死值验原文 —— 逐刀真实输出

> 量于 **2026-09-13**，工作树 `.claude/worktrees/k-r110`、分支 `track/k-r110`、基点 `b0953e5`。
> 刀具住址：`<本工作树>/evidence/K-R110-cut.py`（`apply <刀名>…` / `restore`，fail-closed，备份落 `evidence/.K-R110-cut-backup.json`）。
> 沙箱门禁（**唯一许可的命令**，从 `/home/zbl/文档/claudecode-frontend` 起跑）：
> `PB_WS=backend-consolidation .claude/devbox/gate <本工作树> k-r110`
> 🔴 **宿主上一条 `cargo` / `npm` / `vitest` / `tsc` 都没跑过**（K31）。

## 零 · 刀名与锚点（第 7 条：切之前先断言锚点恰好命中 N 次）

| 刀 | 落在哪个文件 · 哪一处 | 锚点命中 | 落地形核对 |
|---|---|---|---|
| `k1` | `remote-daemon-proto/src/plugin/mod.rs` · `the_exit_code_shape_guard_actually_bites` 里「一张擦掉了名字的真码表…」那句 `assert!` 之后（落在**今天漏出测试段的那 31 行**内，实得插在第 666–669 行） | **1 次**（要 1） | 落地形命中 **1 次** |
| `k3` | `remote-daemon-proto/src/readonly_guard.rs` · `mod remote_write_layer`（本文件**最后一个**测试模块）里**最后一个 `#[test]` 之后**那段 `assert!(blind > 0, …)` 之后 | **1 次**（要 1） | 落地形命中 **1 次** |
| `k4` | `src-tauri/crates/guard-core/src/lib.rs` · ① `assert_tree_strips_clean` 里那一行调用 ② 标记块 `KR110D1`（判据本体）③ 标记块 `KR110D1-selftest`（它的两条自检）④ 头注里三处 `[`…`]` 引用改成不带符号的措辞 | 调用锚点 **1 次** · 两对标记块**各 1 对** · 三处引用**各 1 次** | 锚点残留 **0 处**；`assert_test_module_ranges_are_brace_balanced` 这个符号在文件里 **0 处** |
| `k5` | `src-tauri/crates/guard-core/src/lib.rs` · `test_module_ranges` 里 `let hay: &str = masked.as_deref().unwrap_or(src);` → `let hay: &str = src;`（**收尾针退回在裸文本上找**） | **1 次**（要 1） | 落地形命中 **1 次** |

⚠ **一处如实记账**：`k4` 第一版是**粗刀**（只摘判据本体与调用点，头注里那三处 `[`…`]` 引用没跟着摘）
⇒ 它自己带来一条红（见下面 `M5`）。**精刀**才是阴性对照该有的形状。两趟都留在下面，别只读一趟。

---

## 一 · 变异表（逐行真实输出）

分母口径：`cargo` 那一格是 **9 个包合计**；`daemon` 那一格是**单包 `remote-daemon-proto`**，
只有一行 `test result` ⇒ 最大值就是合计。两个数都取自门禁自己印的判定行，**不是我加出来的**。

| 趟 | 盘上状态 | `cargo` | `daemon` | 其余 11 格 | `GATE` | 判 |
|---|---|---|---|---|---|---|
| `M0` | **干净树**（`b0953e5`，只有 4 份未跟踪的 evidence 占位） | **1601** | **755** | 全绿 | `OK`（13/13）· `EXIT=0` | 基线，**自己现打** |
| `M1` | **刀①**（修**之前** ＋ `k1`） | 1601 | **755** | 全绿 | `OK`（13/13）· `EXIT=0` | 🔴 **必须不红 ⇒ 不红。这是本件存在的证据** |
| `M2` | 修法 ＋ 新判据 ＋ 夹具（第一趟） | **1605** | 755 | `fmt` **红** | `FAIL —— fmt（rc=1）` | `fmt` 红的是**我自己**一行 `assert!` 超宽（原文见 §四），其余 12 格全绿 |
| `M2b` | 同上，`fmt` 修好 | **1605** | 755 | 全绿 | `OK`（13/13）· `EXIT=0` | 修法落地面 |
| `M3` | **刀②**（修**之后** ＋ `k1`，同一刀） | 1605 | **754 passed; 1 failed** | 其余全绿 | `FAIL —— daemon（rc=101）` | 🔴 **必须红 ⇒ 红，且只红 1 条**（下面 §二 逐字） |
| `M4` | **刀③**（修之后 ＋ `k3` 同形新反例） | 1605 | **755** | 全绿 | `OK`（13/13）· `EXIT=0` | 🔴 **必须被正确切段 ⇒ 是** |
| `M5` | **刀④粗刀**（`k3` ＋ 摘判据，头注引用没跟着摘） | **1468 passed; 1 failed** | 755 | `fmt` 红（刀留下的 2 处空行） | `FAIL —— fmt；cargo` | 见 §三：红的是 `structural_scan` 那条**散文点名**判据，**不是刀③** |
| `M6` | **刀④精刀**（`k3` ＋ 判据本体 ＋ 两条自检 ＋ 三处头注引用一起摘 ＋ 收掉多余空行） | **1603** | **755** | 12 个代码格**全绿** | `FAIL —— pb check`（`FAIL=1`） | 🔴 **代码面一条都不红 ⇒ 阴性对照成立**；`pb check` 那一格点的是**别人的件**（§三） |
| `M7` | **退掉实现**（`k5` 把收尾针退回裸文本 ＋ `k3`） | **49 passed; 1 failed**（`-p guard-core --lib`） | **754 passed; 1 failed** | `fmt` 全绿 | `FAIL —— cargo；daemon；pb check` | 🔴 **撤掉 fix 就红 ⇒ 夹具与新判据都不是仪式**（§二） |
| `M8` | **终局干净树**（本件全部交付，无刀） | 见 §五 | 见 §五 | 见 §五 | 见 §五 | 交付面 |

**没有 CRASH。** 每一趟的判定行都在（`cargo` / `daemon` 两格都印出了 `test result:` 行），
没有一趟是「判定行掉了」或「异常退出」——`M2`/`M5` 的 `fmt` 红是 `cargo fmt --check` 的 `rc=1`（两态格，本来就没有判定行）。

---

## 二 · 每一刀红的**是谁**（逐字，不是「几条红」）

### `M3` = 刀②：修完之后，同一刀必须红

```
thread 'plugin_walk_fixture::tests::this_fixture_never_ships' panicked at src/plugin_walk_fixture.rs:1465:9:
assertion `left == right` failed: 测试段里经通用调用口起进程的文件与登记的对不上。
**多出来的**：先回答一句「这是不是又一处夹具，挂在那条唯一的起进程口上」…
  left: ["layering_guard.rs", "mod.rs"]
 right: ["layering_guard.rs"]
test result: FAILED. 754 passed; 1 failed; 2 ignored; ...
```

- **红的射程 = 最小面**：`754 passed; 1 failed` —— **恰好 1 条**，而且正是那条以**测试段**为人群的天花板。
- **它为什么是「本该被拦的东西」**：那条天花板逐字登记的合法命中只有 `layering_guard.rs` 一份
  （登记表 `SYNTHETIC_ONLY` 的理由栏写着「分层扫描的两处**合成违规样本**（字符串字面量，不起进程）」）
  ⇒ 我插的探针形状与那两处**逐字同类**（也是字符串字面量），多出来一处而没签字 ⇒ 按设计必须红。
- **`M1` 为什么不红**：同一行字节、同一份文件、同一条判据 —— 只差「那 31 行算不算测试段」。
  🔴 **这就是 fail-open 的活体**：`guard_core::test_source` 少扫 31 行 ⇒ 那条天花板对这份文件**看不见**。

### `M7` = 退掉实现：两处红，各是谁

```
[guard-core 单元层]
thread 'tests::a_column_zero_brace_that_is_not_code_does_not_close_the_test_module' panicked at
crates/guard-core/src/lib.rs:2603:13:
[原始字符串] 测试段少了尾巴 —— 区间在字符串/注释里提前收尾了：
test result: FAILED. 49 passed; 1 failed; ...

[daemon 树遍历层 —— 这是本件新加的那条判据在说话]
thread 'guard_support::tests::every_daemon_file_strips_clean' panicked at
.../guard-core/src/lib.rs:1099:5:
readonly_guard.rs：`test_module_ranges` 切出的 1 段里有段**花括号不配平**：
readonly_guard.rs · 第 3423–4062 行这一段：`{` 56 个 / `}` 54 个
★ 典型成因：收尾判据（列 0 的右大括号）落进了**字符串字面量 / 原始串 / 块注释**里 …
test result: FAILED. 754 passed; 1 failed; ...
```

- 🔴 **新判据逐字点名了刀③ 造的那份新反例**（`readonly_guard.rs · 3423–4062`）——
  它证明的是「修的是**这一形**」，不是「把 `plugin/mod.rs` 那个夹具挪走了」。
- ⚠ **诚实边界**：`assert_tree_strips_clean` 是**逐份文件走、第一处不配平就 panic** ⇒
  这一趟 `plugin/mod.rs` 也是不配平的，只是遍历先撞上 `readonly_guard.rs`。
  **别把「只点了一份」读成「只有一份」。**
- 三个夹具形（原始字符串 / 块注释 / 普通串里的真换行）里，**第一形先红就整条 panic** ⇒
  另外两形这一趟没单独出声（同一条 `#[test]` 里的三次循环）。如实记：**三形我没有各切一刀**。

---

## 三 · 两处「红了但不算这一刀的账」，逐处写清

### ① `M5` 的 `cargo` 红 —— 粗刀自己带来的，不是刀③

```
thread 'structural_scan::tests::every_dead_name_named_in_the_prose_is_declared_dead' panicked at
src/structural_scan.rs:3065:9:
这几处散文点名了一个**代码里根本不存在**的名字，而登记表对不上：
  src-tauri/crates/guard-core/src/lib.rs  `assert_test_module_ranges_are_brace_balanced`  盘上 3 处，登记表写 0 处
test result: FAILED. 1468 passed; 1 failed; ...
```

粗刀只摘了判据本体与调用点，**头注里三处 `[`…`]` 引用留着** ⇒ 它们变成了指不到的散文名字。
🔴 **这条红本身是个好消息**：monitor 侧真有一条判据在守「摘掉一个符号却留着头注指它」，
⇒ 本件新加的那条判据是**双锚**的：摘它不会静默。
精刀（`M6`）把那三处一起改成不带符号的措辞之后，**代码面 12 格全绿** ⇒ 阴性对照成立。

### ② `M6` / `M7` 的 `pb check FAIL=1` —— 点的是**别人的件**（纪律 ㉓）

```
FAIL   [J3 陈账] INDEX.md 比源文件旧 —— 重跑 `pb index` 落盘
===== pb check: FAIL=1 BROKEN=0 =====
```

现打 mtime（量于 `/home/zbl/文档/claudecode-frontend/.claude/planned-build/backend-consolidation`）：

| 文件 | mtime |
|---|---|
| `INDEX.md` | **20:15:35** |
| `features/K-R102-子命令表里有而分派臂没有这一形没有判据看得见.md` | **20:42:28** |

⇒ 是并跑的 **`K-R102`** 在 20:42 改了它自己的件文件，把共享计划仓的 `INDEX.md` 顶成了陈账。
🔴 **`M0`–`M5` 五趟 `pb check` 都是 `FAIL=0`，而那五趟我一个计划仓文件都没写过。**
⚠ 而 `pb index` 属「窗口开着期间一概不跑」的生成命令族（铁律 19 第三条）⇒ **我不跑，交 PM**。

---

## 四 · `M2` 那次 `fmt` 红的原文（我自己的，如实记）

```
Diff in .../guard-core/src/lib.rs:2608:
-                assert!(prod.contains(keep), "[{what}] 剥过头了，生产段少了 `{keep}`：\n{prod}");
+                assert!(
+                    prod.contains(keep),
+                    "[{what}] 剥过头了，生产段少了 `{keep}`：\n{prod}"
+                );
```

同一份诊断里还印了 **三条 `whitespace symbol '\u{3000}' is not skipped` 警告**，
住 `src-tauri/src/skill_host.rs:946/947/949` —— **那三条是存量、不是我带来的**：
`M0` 那一趟 `fmt` 是 `ok`（`rc=0`），警告不改退出码；是我这一处 diff 把 `rc` 顶成 1，
诊断才把整份 stderr 一起吐了出来。⚠ 这三条**在我的写区外**，一个字没动，交 PM。

---

## 五 · 终局门禁（`M8`，量于干净树、本件全部交付、无刀）

```
  ok   hooks          11 passed
  ok   fmt            1 passed
  ok   fmt-daemon     1 passed
  ok   winchk         1 passed
  ok   cargo          1605 passed（9 个包合计）
  分母 cargo          本树未铺 src-tauri/embedded-daemons/ ⇒ 少「本地后端真的能起来吗」那族 4 条
  ok   generated      与 Rust 源一致
  ok   daemon         755 passed
  ok   npm            1722 passed
  ok   ccm e2e        ccm-print-parity       PASS=12（地板 12，恒等）
  ok   ccm e2e        ccm-rbind-title        PASS=8 （地板 8，恒等）
  ok   ccm e2e        ccm-cli                PASS=46（地板 46，恒等）
  ok   ccm e2e        ccm-contract-parity    PASS=45（地板 45，恒等）
  FAIL pb check       [J3 陈账] INDEX.md 比源文件旧 —— 重跑 `pb index` 落盘
GATE: FAIL —— pb check[backend-consolidation]（FAIL=1 BROKEN=0）· EXIT=1
```

- **12 个代码格全绿**，逐格与 `M0` 基线对得上：`cargo` **1601 → 1605**（+4 = 本件新加的四条 `guard-core` 夹具）·
  `daemon` **755 → 755**（本件没往 daemon 加 `#[test]`，新判据是**在既有的树遍历里多断一条**）· 其余 11 格逐格相同。
- 唯一那一格红点的是**共享计划仓**，且**这一趟我一个计划仓文件都没写过**（§三②）。
  ⚠ 交回之后我要写件文件 `§3`/`§8` ⇒ `[J3 陈账]` 会**继续红**，直到 PM 收窗口那一拍跑 `pb index` ——
  这是铁律 19「窗口开着期间一概不跑生成命令」的结构性后果，**不是本件的缺陷**。

---

## 六 · 把实现整个退掉，还有多少条新断言仍绿（`7u`）

**退掉的是什么（逐处点名，不是 `git checkout`）**：`k5` —— `test_module_ranges` 的收尾针
**退回在裸文本上找**（`let hay: &str = src;`）。**保留**：新原语 `mask_all_literals` / `scan_and_blank`、
新判据 `assert_test_module_ranges_are_brace_balanced` 与它的调用点、全部四条新夹具、全部头注。

实得（`M7`）：`guard-core` `49 passed; 1 failed` · `daemon` `754 passed; 1 failed`。

**本轮新加的 5 条断言，逐条给判**：

| 新断言 | `7u` 之后 | 为什么 | 它的牙在哪一刀上 |
|---|---|---|---|
| `a_column_zero_brace_that_is_not_code_does_not_close_the_test_module`（三形一条） | 🔴 **红** | 它钉的正是被退掉的那件事 | `M7`（原始字符串那一形先 panic；**块注释与普通串两形没单独切过刀**，如实记为「这一趟没单独出声」） |
| `assert_test_module_ranges_are_brace_balanced`（判据本体，经 `assert_tree_strips_clean` 走两棵树） | 🔴 **红**，并**逐字点名** `readonly_guard.rs · 3423–4062` | 收尾针退回裸文本 ⇒ 区间在串里收尾 ⇒ 计数不配平 | `M7`（正向）＋ `M6`（把它整段摘掉，代码面 12 格全绿 ⇒ 反向：**它不误红**） |
| `the_brace_balance_check_actually_bites`（判据自检·正反两向） | **绿** | 它喂的是**手搓的**不配平/配平文本，不经收尾针 ⇒ 与 `k5` 无关 | 自检内建两向（不配平必 panic ＋ 正常语料不许红 ＋ 「真的切出了 1 段」的反空真）；**没从外面单独切过刀** |
| `the_brace_balance_check_says_it_cannot_tell_when_the_lexer_gives_up` | **绿** | 它钉的是**兜底出声**，与收尾针在哪份文本上找无关 | 🔴 **没单独切过刀 —— 按「自检 ≠ 变异验过」记为未验** |
| `the_lexical_mask_blanks_only_non_code_bytes_and_keeps_the_length` | **绿** | 它钉的是**新原语本身**（等长 · 只剩 1 个 `{` · 兜底 `None`），`k5` 没动原语 | 🔴 **没单独切过刀 —— 未验** |

🔴 **仍绿不等于仪式，但也不等于验过** —— 上表点了 **2 条「未验」**（＋1 条只有内建自检），别读成「`7u` 证明了它们没牙」。

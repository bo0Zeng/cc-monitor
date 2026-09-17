# `K-R25` `KR25D4` · 那 9 份没被三条看门判据看着的文件，各是什么

**只答不改**（件文件 `§1 KR25D4` 逐字：「答清那 9 份各是什么、为什么不在人群里、要不要拉进来。只答不必改。」）。
**量于** 工作树 `.claude/worktrees/k-r25` @ `3aab95a`（本树未铺 `src-tauri/embedded-daemons/`）。
**分母怎么切**（三条命令，都在本树根跑）：

```
git ls-files | grep '\.rs$' | grep -v '^src-tauri/vendor/' | wc -l      → 188
git ls-files src-tauri/src            | grep -c '\.rs$'                 → 105
git ls-files remote-daemon-proto/src  | grep -c '\.rs$'                 →  73
git ls-files src-tauri/crates         | grep    '\.rs$'                 →   9 份（逐份列在下表）
```

`188 = 105 + 73 + 9 + 1`（最后那个 1 是 `src-tauri/build.rs`）。
三条看门判据看着 `105 + 73 + 1（guard-core 自己）= 179` ⇒ **差 9 份**，与 `K-R9` 落定拍
`evidence/K-R9-R2-fallback-watch-scope.md §E` 的算法逐字相同，**这一趟是重打的，不是抄的**。

---

## 逐份

| # | 文件 | 它是什么 | 为什么不在人群里 | **今天有没有判据把它整份喂进剥法** |
|---|---|---|---|---|
| 1 | `src-tauri/crates/acct-core/src/lib.rs` | 账号模型的纯逻辑 crate | 三条看门判据的 `root` 是 `src-tauri/src` · `remote-daemon-proto/src` · `crates/guard-core/src` —— **`crates/` 下别的包一个都不在这三个根里** | **有**：`structural_scan.rs` 的 `addr_corpus()`（`:1216` 三棵树含 `src-tauri/crates`）→ `strip_comment_lines`；另 `needle_anchor_registry.rs:249` → `test_source`；`doc_claim_registry.rs:715/1349` → `strip_comment_lines`；`creds_store.rs:700`、`gate_singleton_guard.rs:80`、`quote_singleton_guard.rs:82`、`session_name_registry.rs:135`、`structural_scan.rs:680/1672` → `production_code` |
| 2 | `src-tauri/crates/branch-core/src/lib.rs` | 分支名/路径的纯逻辑 crate | 同上 | **有**（同上那批按 `src-tauri/crates` 整棵树扫的判据） |
| 3 | `src-tauri/crates/creds-core/src/lib.rs` | 凭据 crate 的门面 | 同上 | **有**：`creds_store.rs::the_plaintext_argument_is_only_ever_handed_one_hop_further` 读它**原文**（不走 `production_code`）＋ 上面那批整棵树扫的 |
| 4 | `src-tauri/crates/creds-core/src/perm.rs` | 凭据文件权限 | 同上 | **有**（同上那批） |
| 5 | `src-tauri/crates/creds-core/src/store.rs` | 凭据落盘 | 同上（⚠ 它是 `K-R1` 那道的写区，本拍一个字没看动） | **有**（同上那批） |
| 6 | `src-tauri/crates/gate-core/src/lib.rs` | 门禁/单例判定的纯逻辑 | 同上 | **有**：`gate_singleton_guard.rs:128` → `production_code(read_to_string(..))`；`session_name_registry.rs:95` → `production_code` |
| 7 | `src-tauri/crates/shell-quote-core/src/lib.rs` | shell 引用的纯逻辑 | 同上 | **有**：`quote_singleton_guard.rs:134` → `production_code(read_to_string(..))` |
| 8 | `src-tauri/crates/usage-core/src/lib.rs` | 用量口径的纯逻辑 | 同上 | **有，而且是 `K-R9` 点过名的那一份**：`usage.rs::the_usage_kou_jing_has_exactly_one_home`（`usage.rs:416`）把它**整份**喂给 `guard_core::production_code` |
| 9 | `src-tauri/build.rs` | 内嵌 daemon 的构建脚本 | 它不在任何一个 `root` 下（`src-tauri/` 根目录，不是 `src-tauri/src`） | **有**：`write_site_registry.rs::corpus()`（`:138`/`:515`）把它 `push` 进语料 → `production_code`；`structural_scan.rs::addr_corpus()`（`:1219`）同理；`doc_claim_registry.rs:732`、`sftp.rs:1414/:1472` 读它原文 |

---

## 答案

### ㈠ 它们各是什么

**8 份是 `src-tauri/crates/` 下除 guard-core 以外的全部纯逻辑 crate 源码**
（acct-core 1 · branch-core 1 · creds-core 3 · gate-core 1 · shell-quote-core 1 · usage-core 1），
**第 9 份是 `src-tauri/build.rs`**。

### ㈡ 为什么不在人群里

**不是有人裁过，是三条判据的 `root` 恰好只有三个**：
`src-tauri/src` · `remote-daemon-proto/src` · `crates/guard-core/src`。
`crates/` 下的兄弟 crate 与仓根的 `build.rs` **落在这三个根之外**，
而这三条判据是**一条一条长出来的**（monitor 一条、daemon 一条、guard-core 吃自己的狗粮一条），
从来没有一条负责「全仓」。⚠ `K-R9` 落定拍已经点过这件事：
`assert_block_comment_model_holds` 的头注当时写着「本仓 187 份 `.rs` 今天是 0 份」——
**「本仓」这个词盖不住任何一个量得出的数**。那句话本拍已经删掉（改成指住判据本身，不写基数）。

### ㈢ 🔴 要不要拉进来 —— **要，而且比 `K-R9` 当时判的更该拉**

`K-R9` 当时的说法是「这 9 份里**至少有一份**（`usage-core`）是真被 `production_code` 吃的」。
**本拍逐份重打，结论比那句强**：

> **9 份里 9 份都被至少一条判据整份喂进了 `guard_core` 的剥法**
> （8 份走 `src-tauri/crates` 整棵树的那批判据，`build.rs` 走 `write_site_registry::corpus()` 与
> `structural_scan::addr_corpus()`）。
> ⇒ 它们**任何一份掉进兜底，那份上的「块注释喂饱判据」洞当场重开，而三条看门判据一条都不会响。**

⚠ **这句话的分母**：上表最后一列是我逐份查出来的**至少一条**喂法（住址逐处给了），
**不是**「所有喂它的地方」的穷举 —— 那个分母我没数。它够用是因为要证的是「有没有人喂它」，
一条就够；要证「没人喂它」才需要穷举。

### ㈣ 建议的形状（**本拍没做，`KR25D4` 写着只答不改**）

最便宜的一条：给 `guard-core` 那条吃狗粮的自检**换个根**，从
`CARGO_MANIFEST_DIR/src`（只有它自己 1 份）抬到 `CARGO_MANIFEST_DIR/..`（= `src-tauri/crates`，9 份），
再加一条盖 `src-tauri/build.rs` 的。地板跟着从 `1 / 30` 抬到 `9 / <现打块数>`。

🔴 **为什么本拍不顺手做**（三条，任一条都够）：
1. `KR25D4` 逐字「只答不必改」—— **改 DoD 的字面不是实现方能自批的事**（`brief` 17）；
2. 它是**换一条判据的射程**，要它自己的死值验（至少：把某份 crate 文件弄成掉兜底 ⇒ 它当场红并点名），
   而本拍的死值验预算已经花在 `KR25D2` 那七刀上；
3. `crates/creds-core/src/store.rs` 是 `K-R1` 那道的写区 —— 扩根之后本条会**读**它，
   两道同拍改同一份文件的风险要 PM 排。

⇒ **交给 PM 决定是立跟进件还是并进下一拍。** 建议的地板取值要现打，别抄本文件里的数。

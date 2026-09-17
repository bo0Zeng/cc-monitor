# `K-R100` 死值验 + 读数

量于工作树 `.claude/worktrees/k-r100`（分支 `track/k-r100`，基点 `f5ecffa`）。
所有 `cargo` / `npm` 命令一律在沙箱镜像 `ccmon-devbox:latest` 里跑（`K31`）。
唯一在宿主上跑的是 `§4` 那两段**只读**语料统计（`os.stat` / 读 jsonl，不写宿主任何东西）。

---

## §1 `KR100D1` 那 12 个助手只剩一份实现，两侧都调它

### 刀 ① 任一侧长出第二份实现 ⇒ 红

**a. monitor 侧**——往 `src-tauri/src/search.rs` 生产段塞回一份 `fn find_ci`：

```
---- search::kou_jing_guard::the_search_kou_jing_has_exactly_one_home stdout ----
panicked at src/search.rs:784:13:
monitor src/search.rs 的生产段里又长出了这些助手的定义：["find_ci"]
test result: FAILED. 14 passed; 1 failed
```

**b. daemon 侧**——往 `remote-daemon-proto/src/observe/search_query.rs` 生产段塞回一份 `fn make_snippet`：

```
---- search::kou_jing_guard::the_search_kou_jing_has_exactly_one_home stdout ----
panicked at src/search.rs:778:13:
daemon observe/search_query.rs 的生产段里又长出了这些助手的定义：["make_snippet"]
test result: FAILED. 2 passed; 1 failed
```

### 刀 ② 两侧都调 core ⇒ 绿

未变异态：`cargo test --lib -- search::` ⇒ `15 passed; 0 failed`
（含 `kou_jing_guard::the_search_kou_jing_has_exactly_one_home`）。
daemon 侧 `cargo test -- search_query` ⇒ `5 passed; 0 failed`。

### 刀 ③ 🔴 改 core 一处、两侧行为都跟着变

**这一刀分两步，第二步才是它与刀 ① 的区别。**

**第一步 —— 改 core 一处，两侧的「真跑出来的东西」跟着动。**
把 `search-core` 的 `SNIPPET_CTX` 由 `48` 改成 `20`（**只改这一处**），两侧的行为判据
（期望值取自 `search_core::SNIPPET_CTX`，实际值来自各自的生产管线）**都仍然绿**：

```
monitor  search::tests::the_snippet_window_comes_from_core          ⇒ ok. 1 passed
daemon   search_query::tests::the_snippet_window_comes_from_core    ⇒ ok. 1 passed
```

⚠ 「仍然绿」在这里**不是空真**：那两条断言的是
`before.chars().count() == SNIPPET_CTX + 1`。若某一侧没跟着动，它会给出 49 而期望是 21 ⇒ 红。
下一步就是把这个反例造出来。

**第二步 —— 让 monitor 偷偷不跟 core 走，而且用一个「名字型判据看不见」的写法。**
core 保持 `SNIPPET_CTX = 20`，在 `search.rs` 里加一个**名字不在 `HELPERS` 里**的
`fn snip_local`（窗口写死 48）并在生产路径上改调它：

```
---- search::tests::the_snippet_window_comes_from_core stdout ----
panicked at src/search.rs:1121:9:
assertion `left == right` failed: snippet 前窗必须等于 `search_core::SNIPPET_CTX`（=20）+ 省略号
  left: 49
 right: 21
test result: FAILED. 14 passed; 1 failed
```

🔴 **同一趟里 `kou_jing_guard::the_search_kou_jing_has_exactly_one_home` 是绿的**
（14 passed 里就有它）—— 名字型的结构判据**看不见**这次分叉，行为判据**看见了**。
⇒ 刀 ③ **不是刀 ① 的换句话说**，它挡的是另一种回潮形状。

### 失效方向的自查（PM 点名的那条）

本件**没有**写「两边源码文本一样」那种判据。理由逐字记在
`search.rs::kou_jing_guard` 的头注里：那判的是**写法**，而两份今天本来就逐字相同
（`K-R85` 09-12 实测）⇒ 那样一条判据**恒绿**。

---

## §2 `KR100D2` A 档：两侧 snippet 预算顺序一致

### 刀 ① 恢复成「一侧按文件系统序、一侧按最近」⇒ 必须红

把 daemon `search()` 里那行 `search_core::sort_by_recency(&mut files, |(_, m)| *m);` 注释掉
（= 回到收口前的 `WalkDir` / `readdir` 序），monitor 那侧不动：

```
---- observe::search_query::tests::the_snippet_budget_goes_to_the_most_recent_sessions ----
assertion `left == right` failed: 预算/输出顺序必须是最近优先（…）
  left: ["mid", "new", "old"]      ← readdir 真的给出了另一个序
 right: ["new", "mid", "old"]
test result: FAILED. 4 passed; 1 failed

---- search::kou_jing_guard::the_search_kou_jing_has_exactly_one_home ----
daemon observe/search_query.rs 不再调 `search_core::sort_by_recency`
test result: FAILED. 2 passed; 1 failed
```

**两条判据同时红**：一条判行为（真的花错了顺序），一条判结构（不再走那唯一一处）。

### 刀 ② 一致 ⇒ 绿

未变异态：monitor `the_snippet_budget_goes_to_the_most_recent_sessions` ⇒ ok ·
daemon 同名 ⇒ ok。两条用的是**同一份语料形状**（创建序与 mtime 序刻意相反）。

### 🔴 哪一种顺序对 —— 理由 + 读数（不照抄 PM 的倾向）

**结论与 PM 的倾向一致（最近优先），但支撑它的不是「用户搜的多半是近的」这句话。**

**理由（结构性，不是偏好）**：合并那一步逐字
`sessions.sort_by(|a, b| b.updated_at.cmp(&a.updated_at))`，前端 `renderSearchResults`
照这个顺序渲染 ⇒ **展示顺序已经定死是最近优先**。预算顺序一旦与展示顺序不同，
**缺 snippet 的就正好是列表最上面那几张卡**。这条与「哪种更相关」无关，
它只是要求「花钱的顺序 = 看见的顺序」。

**读数**（09-13，语料 = 本机 `~/.claude/projects`，**169 个 jsonl / 49 个项目目录**；
脚本复刻 `WalkDir(max_depth=2)` 的 `readdir` 枚举序）：

| 量 | 走序（收口前 daemon） | 最近序（收口前 monitor / 今天两侧） |
|---|---|---|
| 与对方前 3 的重合 | **0/3** | — |
| 与对方前 10 / 前 50 的重合 | 2/10 · 13/50 | — |
| 前 3 的中位年龄 | **16.1 天** | **0.01 天** |
| 走序前 10 在最近序里的名次 | `[6, 85, 96, 94, 82, 119, 118, 4, 52, 50]` | — |

🔴 **决定性的一格**（`--limit 50`，三个真实查询词，模拟两种预算顺序后按展示序排）——
**展示序最上面 10 张卡里「一条 snippet 都没有」的张数**：

| 查询词 | 走序 | 最近序 |
|---|---|---|
| `docker` | 6/10 | **2/10** |
| `门禁` | 9/10 | **8/10** |
| `search` | 10/10 | **6/10** |

⚠ **如实记一格反例，别只报好看的**：按「命中会话里 `hits: []` 的**总**张数」量，
最近优先**不占优**：`docker` 21→20 · `门禁` 22→**26** · `search` 41→**46**。
原因是最近那个会话往往命中最多，一口气吃掉大半预算。
⇒ **「最近优先」买到的不是「少几张空卡」，是「空卡不在你眼前」**。

**还有一格**：走序**根本不是一个「顺序」** —— `readdir` 在 ext4 上是目录哈希序，
随文件系统状态变、跨机器不同 ⇒ 同一个查询在两台远端上给出的 snippet 集合不可复现，
而差异与相关性无关。最近序是**内容决定的**，两台机器上同义。

---

## §3 `KR100D3` B 档：远端截断说得出话

### 刀 ① 截断而不说 ⇒ 红

**a. daemon 不再吐 `hitsTruncated`**（`"hitsTruncated": false` 写死）：

```
---- observe::search_query::tests::truncation_is_stated_not_left_to_an_empty_array ----
assertion `left == right` failed: 🔴 `hitCount>0` 而 `hits: []` 必须自己说出「我被预算砍了」…
  left: Bool(false)
 right: true
test result: FAILED. 4 passed; 1 failed
```

**b. monitor 合并退回 `truncated: local.truncated`**（收口前逐字就是这一行）：

```
---- search::tests::remote_truncation_survives_the_merge ----
panicked: 远端截断在合并处被丢掉了 —— 那正是「远端截断界面一个字不说」的成因
test result: FAILED. 9 passed; 1 failed
```

### 刀 ② 说了 ⇒ 绿

未变异态：daemon `truncation_is_stated_not_left_to_an_empty_array` ⇒ ok ·
monitor 同名 + `remote_truncation_survives_the_merge` ⇒ ok ·
前端 `src/views/history-search-truncation.vitest.ts` ⇒ **5 passed**。

### 刀 ③ 🔴 「真的 0 命中」与「截断了」两种情况下游分得开

把前端渲染退回收口前那一句通吃
（`…还有 N 条命中（点任意条打开会话查看全部）`，不可点；状态行 `（结果较多，仅显示前若干条）`）：

```
✓ (A) 真的没有命中 → 「无匹配」，一张卡都没有
✓ (B) 有命中、全给了 snippet → 有命中行，没有任何截断提示
× (C) 远端被预算砍了 → 状态行说得出，卡片说得出，且点得开
× 🔴 「预算砍的」与「本会话只列前 N 条」文案不同 —— 一个值不许再装两件事
× 🔴 (C) 与 (A) 在「够不够得到那些结果」这一维上必须不同
Tests  3 failed | 2 passed (5)
```

#### ⚠ 一条**写过又改掉**的判据，留下经过（这一格是自己逮到自己的）

第一版的第 5 条写的是「三种形状序列化后**两两不同**」。它在死值验里**没死**：
三份 fixture 的 `hitCount` / 卡片数本来就不同，于是「两两不同」在退回收口前的渲染时
**照样成立**（第一趟死值验实测 `2 failed | 3 passed`，它在那 3 个 passed 里）。
那是一句真话摆错了格 —— 它声称在判「分得开」，实际判的是「三份 fixture 不一样」。
⇒ 换成「(C) 与 (A) 在**够不够得到那些结果**这一维上必须不同」：
断言 (C) 状态行点名「预算」、卡片那一行**不写「点任意条」**、且它**真的能打开会话**
（`hits: []` 时它是唯一入口）。换完再跑同一个变异 ⇒ `3 failed | 2 passed`，它死了。

---

## §4 性能：「桌面侧内存查的路径不许因为收口径而变慢」

### 量法一（**配对**，这条是载体）

整份 query 的墙钟在容器里进程间噪声 ±5%，分辨不出 2% 的差别。而收口径**唯一可能变慢
的地方**就是那几个助手从同 crate 变成跨 crate 调用 ⇒ 在**同一个进程里**交替各跑一遍：
`A = search_core::make_snippet` · `B = 收口前那份逐字相同的本地副本`（贴进临时台架）。
`40×500` 次/轮、5 轮取中位，跑 3 趟。

| 趟 | 不加 `#[inline]` | 加 `#[inline]` 之后 |
|---|---|---|
| 1 | core 151360 us · 副本 149277 us ⇒ **+1.4%** | core 157317 · 副本 157279 ⇒ **+0.0%** |
| 2 | core 151613 us · 副本 149852 us ⇒ **+1.2%** | core 156327 · 副本 156876 ⇒ **−0.3%** |
| 3 | core 152417 us · 副本 151343 us ⇒ **+0.7%** | core 155813 · 副本 156389 ⇒ **−0.4%** |

🔴 **第一列是「真的退了」，不是噪声**：三趟同号、同量级，而配对法的分辨力在 0.3% 上下。
成因是跨 crate 调用不内联（`cargo test` 的 profile 不开 LTO）。
⇒ 给 core 那 8 个热函数标 `#[inline]`（理由逐字写在 `search-core` 的注释里），
第二列三趟落回 **0.0% / −0.3% / −0.4%**，退的那一格没了。

### 量法二（整份 query 的墙钟，只作背景值）

200 会话 × 200 消息的内存索引，`query("docker", limit=300)` 30 次均值，每趟一个新进程：

- **改前**（`f5ecffa` 的 `search.rs`，4 个样本）：5153 · 5485 · 4993 · 5089 us ⇒ 中位 **5121**、均值 5180
- **改后**（无 `#[inline]`，9 个样本）：5038 … 5513 us ⇒ 中位 **5308**、均值 5285
- **改后**（有 `#[inline]`，4 个样本）：5310 · 5144 · 5365 · 6055 us ⇒ 中位 **5338**

⚠ **这一格的结论是「判不了」**，如实写：两组区间几乎完全重叠（4993–5485 vs 5038–5513），
样本量 4 vs 9，而进程间噪声 ±5% —— 它**分辨不出** 1~2% 的差别。
真正给出读数的是量法一。

**口径**：`--release`（`opt-level="z"` · `lto=true` · `codegen-units=1`），
沙箱容器内，`CARGO_TARGET_DIR` 同一份。台架**跑完就删了**（不留 `#[ignore]` 在门禁里）。

---

## §5 消费者普查（`§0d` 两把尺子）—— 读数

### 尺子① 提名字的（量于基点 `f5ecffa`，分母 = `git grep "fn <name>(" -- '*.rs'`）

那 12 个助手在盘上共 **27 处定义**，逐个点名：

- 两侧各一份（12 对 = 24 处）：`extract_text_blocks` · `extract_tool_text` ·
  `stringify_json` · `clean_user_text` · `make_snippet` · `find_ci` · `tail_chars` ·
  `head_chars` · `collapse_ws` · `collapse_ws_keep_ellipsis` ·
  `truncate_chars_plain`＝`truncate_plain` · `truncate_chars`＝`truncate_excerpt`
  （住 `src-tauri/src/search.rs` 与 `remote-daemon-proto/src/observe/search_query.rs`）
- 🔴 **另外 3 处在本件射程之外**（属 `K-R54` 那 16 处的堆）：
  `src-tauri/src/history.rs:2556 clean_user_text` · `src-tauri/src/history.rs:2578 truncate_chars` ·
  `remote-daemon-proto/src/observe/history_query.rs:615 truncate_chars`

另外两个名字：`SearchIndex` **30 处 / 8 份文件** · `search_query` **33 处 / 13 份文件**
（分母 = `*.rs` `*.ts` `*.md`，不含 `*.lock`）—— 逐份核过，**没有一份**是「口径的第二个家」，
都是住址引用 / 登记表 / 文档。

**改后**：同一把尺子量 ⇒ **15 处**。射程内 24 → **0**，`search-core` 里 **12**，
射程外那 3 处原样留着（`〔R100a〕`）。

### 尺子② 数它的（计数型判据 / 登记表，**零命中字面量却数着这两份**）

**PM 现打已知 2 个 · 本轮实测 6 个 · 多出来的 4 个逐个点名**：

| # | 住址 | 它数的是什么 | 本件让它动了什么 | 谁逮到的 |
|---|---|---|---|---|
| 1 | `src-tauri/src/cross_half_edge_registry.rs::CROSS_EDGES` | 跨半边编译期边的条数（遍历发现 vs 登记对拍） | **17 → 18**（新判据 `include_str!` daemon 那份） | PM 已知 |
| 2 | `src-tauri/src/shared_crate_registry.rs` | 共享 crate 数（4 条判据） | 地板 `n>=7`→`8` · `members` +1 · `scripts/gate.sh` 的 `run_gate_sum cargo 8`→**9**（它**现算自 crate 数**）· `every_path_dependency_is_actually_committed` 要求新 crate 进 git | PM 已知（**红过一次**：新 crate 的 `Cargo.toml` 没 `git add` ⇒ 当场红，报文逐字点名） |
| 3 | `src-tauri/src/byte_cap_registry.rs` | 字节上限的登记表（4 条判据 + 2 张表） | 扫描面 `src-tauri/crates` **两处都要加**（`size_typed_consts()` 与 `scan()` **各走一遍遍历**，头注说「共用同一份」是假的，本轮就是漏了第二处才红）· `CAPS` 里 search 的 4 条 → 2 条（住址改到 `search-core`）· 「对 D」那条两侧对拍**删掉**（结构上不再可能漂）· `NOT_A_SIZE_CAP` 加 `LIMIT_MAX` · 死木检查的扫描面同步 +1 | **本轮量出来的** |
| 4 | `src-tauri/src/rust_timer_registry.rs::the_shell_wake_scan_is_neither_too_narrow_nor_too_wide` | 「模式面画大了会误命中 Rust 循环」的**反向标的** | 标的（`find_ci` 的 `for i in 0..n`）随实现搬进 `crates/`，而它的遍历器只走 `src/` ⇒ 报文变成「`search.rs` 里那个循环不见了」，**方向是错的**。改成按住址读 core | **本轮量出来的** |
| 5 | `remote-daemon-proto/src/readonly_guard.rs::g6_dependency_signoff::SIGNED` | daemon 每条依赖的「写面签字」 | **+1 行**（`search-core`，判档 `已量·未见写面`） | **本轮量出来的** |
| 6 | `doc/IPC-PROTOCOL.md` 的 `--search` 那条 | 子命令的选项面与输出形状 | 输出多一个 `hitsTruncated`、行序改成最近优先 ⇒ 补进文档（选项面本身没变，`protocol_doc_guard` 未红） | **本轮量出来的** |

**量过而没动的**（写下来，免得下次再量一遍）：
`structural_scan.rs`（地板 `monitor>=100` / `daemon>=80`，加文件只涨）·
`parity_ledger.rs`（按 IPC **命令名**登记，本件不增删命令）·
`remote-daemon-proto/src/agent_locality_guard.rs`（`("observe/search_query.rs", 2, "会话记录根 + 会话文件判定")` 两处没动）·
`remote-daemon-proto/src/protocol_doc_guard.rs`（`include_str!` ×2 仍指向同一文件，选项面没变）·
`agent_dispatch_registry.rs` / `local_read_surface_registry.rs` / `remote_write_registry.rs`（住址没变）。

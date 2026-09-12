# `K-R71` · `observe/` 归位 —— 逐刀读数

> 量于工作树 `/home/zbl/文档/claudecode-frontend/.claude/worktrees/k-r71`，分支 `track/k-r71`，基点 `072d849`。
> **全部读数在沙箱里打**（`DECISIONS.md#R21`：`cargo test` 执行被测代码 ⇒ 一律沙箱）。
> 量具：`<本会话 scratchpad>/kr71-box`（一份只属于本轮的脚本，挂载/隔离与 `.claude/devbox/gate` 逐条相同，
> 被测对象恒指 `k-r71` 这棵树、target 恒是 `.claude/pm-targets/k-r71`）。
> ⚠ **量具住在会话 scratchpad，不随本文件发布** —— 复跑请照下面「量具原文」那一节重建。

---

## 一 · 变异台的分母

| 项 | 读数 |
|---|---|
| 变异台命令 | `cargo test -p monitor --lib`（沙箱内，`CARGO_TARGET_DIR=.claude/pm-targets/k-r71`） |
| 基线判定行 | `test result: ok. 1397 passed; 0 failed; 10 ignored; 0 measured; 0 filtered out` |
| 判定行条数 | **1**（单包单 target ⇒ 只有一行 `test result`） |
| 为什么用它而不是整趟门禁 | 门禁 5 格 + 4 套 e2e 一趟数分钟；本族三条 dod 的靶**全在 `-p monitor --lib` 这一格里**（`backend::tests` · `scanning_guard_registry::tests` · `doc_claim_registry::tests`）。⚠ **它盖不到** daemon / npm / e2e 三格 —— 那三格由收工那趟整门禁盖（`§8` 四格数字） |

🔴 **判定行纪律**：下面每一刀都核过「判定行仍是 1 行」。
**编译错一律按 CRASH 单列，不进判定行读数** —— 本轮 CRASH **1 次**，见 §四。

---

## 二 · 五刀（每刀：逐字锚点 · `count()==1` 断言 · 判定行读数 · 最小面）

### 刀 M1 —— `KR71D1` 的死值验

- **切在哪**：`src-tauri/src/backend/mod.rs` 的 `BACKEND_FILES`，`observe/local_query.rs` 那一条的**能力线**。
- **逐字锚点**：`        "observe/local_query.rs",\n        "observe",\n` ——**命中恰好 1 次**（切之前断言，实打 `1`）。
- **换成**：同一条的能力线字面量改回 `"control"`（**只动那 7 个字符**，路径与理由一字未动）。
- **变异已落地**：改后同锚点的 `"control"` 形态 `count()==1`，实打 `True`。
- **判定行读数**：`test result: FAILED. 1396 passed; 1 failed; 10 ignored`（判定行 **1** 行，与基线同）。
- **最小面**：**红 1 条**，逐字 `backend::tests::every_file_under_backend_lives_on_a_capability_line`。
- **判什么**：正是那条判据里 `f.starts_with(&format!("{line}/"))` 的路径前缀断言 ——
  `observe/local_query.rs` 登记成 `control` 线，前缀对不上。

### 刀 M2 —— `KR71D2` 的死值验

- **切在哪**：`src-tauri/src/scanning_guard_registry.rs` 的 `PENDING`（`K-R38` 立的递减棘轮那张表）。
- **逐字锚点**：`        "src-tauri/src/backend/observe/local_query.rs",\n` —— **命中恰好 1 次**（实打 `1`）。
- **换成**：旧住址 `"src-tauri/src/backend/control/local_query.rs"`（**条数不变，仍 29 条**）。
- **变异已落地**：改后旧路径 `count()==1`、新路径 `count()==0`，实打 `True / 0`。
- **判定行读数**：`test result: FAILED. 1395 passed; 2 failed; 10 ignored`（判定行 **1** 行）。
- **最小面**：**红 2 条**，逐字
  · `scanning_guard_registry::tests::no_new_guard_walks_the_tree_without_excluding_itself`
  · `scanning_guard_registry::tests::the_pending_inventory_only_shrinks`
- ⚠ **2 条不是粗刀，是同一张表的两个方向**，件计划只预言了第一条：
  · 第一条从 `newcomers` 那头红 —— 搬过去的那份文件在裸遍历，而清单上没有它；
  · 第二条从 `stale` 那头红 —— 清单上那条旧住址**已经不裸遍历了**（文件不在那儿了）。
  ⇒ **表→盘、盘→表两个方向都真的有牙**，这一格比件计划预言的多买到一条。

### 刀 M3 —— `KR71D3` 方向 ①（盘上多、表上无）

- **切在哪**：`src-tauri/src/backend/observe/` 目录，多放一份**不登记**的 `.rs`。
- **夹具**：`src-tauri/src/backend/observe/spare.rs`，内容三行（`pub(crate) fn spare() -> u8 { 0 }`）。
  🔴 **中性名**（不取自任何断言用的子串）· **不被任何 `mod` 声明** ⇒ 不参与编译，只被目录遍历看见。
- **变异已落地**：`ls observe/` 实打 `local_query.rs mod.rs spare.rs`。
- **判定行读数**：`test result: FAILED. 1396 passed; 1 failed; 10 ignored`（判定行 **1** 行）。
- **最小面**：**红 1 条**，逐字 `backend::tests::every_file_under_backend_is_registered_with_a_reason`。
- 🔴 **纪律 ㉑ 对账**：这份夹具是**变异体本身**，不是备份/中间产物；跑完立即删，
  收工前 `git status --porcelain` 已核 `src-tauri/src/` 下**零残留**（见 `§8`）。
  变异台的备份全部落会话 scratchpad，**一个字节都没落进 `src-tauri/src/`**。

### 刀 M4a —— `KR71D3` 方向 ②（表上有、盘上无），**件计划字面那一刀**

- **切在哪**：`BACKEND_FILES` 里**加一条**指向不存在文件的条目。
- **逐字锚点**：`    ("observe/mod.rs", "observe", "读面的说明 + 什么该进来的判准"),\n` —— **命中恰好 1 次**。
- **换成**：在它后面插一条 `("observe/absent.rs", "observe", "变异 M4a：这条指向一个盘上不存在的文件"),`。
- **变异已落地**：`"observe/absent.rs"` `count()==1`，实打 `True`。
- **判定行读数**：`test result: FAILED. 1395 passed; 2 failed; 10 ignored`（判定行 **1** 行）。
- **最小面**：**红 2 条**，逐字
  · `backend::tests::every_file_under_backend_is_registered_with_a_reason`（要它红的那条，红了）
  · `backend::tests::the_backend_scan_actually_finds_files`（抽取器自检）
- ⚠ **第二条是结构上躲不开的**，不是粗刀：那条自检断的是 `n >= BACKEND_FILES.len()`，
  而「表上多一条盘上没有的」按定义就让 `len(表) > n`。⇒ **只要走「加一条 ghost」这个形，它必然同红。**

### 刀 M4b —— `KR71D3` 方向 ②，**把条数摁住之后的最小面**

- **切在哪**：同一处，但**不加条目**，把一条**已有**条目的路径换成盘上不存在的。
- **逐字锚点**：同 M4a 那一行 —— **命中恰好 1 次**。
- **换成**：`("observe/absent.rs", "observe", "读面的说明 + 什么该进来的判准"),`（**表长不变，仍 16 条**）。
- **变异已落地**：`"observe/absent.rs"` `count()==1`，实打 `1`。
- **判定行读数**：`test result: FAILED. 1396 passed; 1 failed; 10 ignored`（判定行 **1** 行）。
- **最小面**：**红 1 条**，逐字 `backend::tests::every_file_under_backend_is_registered_with_a_reason`。
- ⚠ **诚实边界，别把这一刀说过头**：它证明的是「**那一条 `assert_eq!` 在表长不变时也会红**」，
  它**没有**把方向 ② 与方向 ① 分离开 —— 这一刀里两个方向同时成立（表上多 `absent.rs`、盘上多 `mod.rs`）。
  **真正只有方向 ②、别的什么都不动的形，在这条判据的构造下不存在**（见 M4a 那条注）。
  ⇒ 「两个方向都查」这句代码自陈：**方向 ① 由 M3 单独证到（1 红最小面）；方向 ② 由 M4a/M4b 证到，
  但它天然与抽取器自检或方向 ① 绑在一起。** 这就是本条能给到的全部，不多写一个字。

### 刀 M5 —— `§0a` **没有列到的第五类面**（PM 现打漏的那一格）

- **它是什么**：`doc/ARCHITECTURE.md §2.1` 那张「backend 四层在 monitor 侧落地到哪一步」表，
  `` `observe/` `` 那一格的状态列，由 `doc_claim_registry::tests::each_registered_status_still_matches_reality`
  拿现场量法 `monitor-backend-observe-landed` 对拍。
  🔴 **`observe/` 目录一建出来，那一格从「待做」当场变成假话** —— 而 `§0a` 的四类面里没有它。
- **切在哪**：`doc/ARCHITECTURE.md`，把那一格改回原文。
- **逐字锚点**：`| \`observe/\` | 有 | **已交付**〔2026-09-12 \`K-R71\`〕—— 目录建起来了，住户只有传输那一跳，见 2.2 |`
  —— **命中恰好 1 次**。
- **换成**：`| \`observe/\` | 有 | **待做** —— **刻意未建**，谁来叫醒见 2.2 |`（即基线原文）。
- **变异已落地**：`| \`observe/\` | 有 | **待做**` `count()==1`，实打 `True`。
- **判定行读数**：`test result: FAILED. 1396 passed; 1 failed; 10 ignored`（判定行 **1** 行）。
- **最小面**：**红 1 条**，逐字 `doc_claim_registry::tests::each_registered_status_still_matches_reality`。
- ⇒ **那一格是有牙的**，本件不改它就出不了门 —— 而它不在 `§0a` 的四类面里。

---

## 三 · `7u` —— 把实现整个退掉（逐处掏空，签名留着）

🔴 **没有用 `git checkout <基线> -- <文件>`**（判据与实现同住 `backend/mod.rs`，checkout 会把判据一起退掉）。
退法是**逐处掏空**，用 `os.rename` ＋ 逐条逐字锚点替换，每一处都先断言 `count()==1`。

### 「实现」与「签名」在本件里分别是什么

本件的**实现 = 那次真搬运**；本件的**签名 = `observe/` 这条能力线在盘上、在登记表上、在文档上的存在**。
⇒ 掏空态刻意做成**件计划 `KR71D1` 逐字点名的那个失效方向**：
「🔴 **建一个空的 `observe/` 目录、文件不动** —— 那是装饰不是搬运」。

| 处 | 掏空动作 | 锚点命中 |
|---|---|---|
| ① `observe/local_query.rs` | `os.rename` 搬回 `control/local_query.rs` | 目录实打 `['mod.rs']` |
| ② 那次跨线引用 | `crate::backend::control::local_backend::…` 退回 `super::local_backend::…` | 1 |
| ③ `observe/mod.rs` | 删掉 `pub mod local_query;` —— **整段模块说明（签名）留着** | 1 |
| ④ `control/mod.rs` | 重新写回 `pub mod local_query;` | 1 |
| ⑤ `BACKEND_FILES` | 那一条退回 `("control/local_query.rs", "control", …)`；**`("observe/mod.rs","observe",…)` 那一条留着** | 1 |
| ⑥ `usage.rs` / `local_accounts.rs` | `backend::observe::local_query` 退回 `backend::control::local_query` | 4 / 2 |
| ⑦ `scanning_guard_registry.rs` | `PENDING` 那一条退回旧住址 | 1 |
| ⑧ `doc/ARCHITECTURE.md` | **刻意不退** —— 「已交付」那一格就是签名 | — |

### 读数

```
test result: ok. 1397 passed; 0 failed; 10 ignored; 0 measured; 0 filtered out
```

## 🔴 **仍绿：全部。0 红。**

「本件新增的断言」这一栏的分母是 **0**（本件一条判据都没新写，`cargo` 四格数字一格没涨），
所以按字面口径「新断言里还有几条仍绿」= **0/0**。
**但那是个空真的答案，本节要报的是比它贵得多的那一条**：

> **把整次搬运掏空、只留一个空的 `observe/` 目录 ＋ 一份什么都不声明的 `mod.rs`，
> 再让 `doc/ARCHITECTURE.md` 继续宣称 `observe/` 已交付 —— `-p monitor --lib` 1397 条一条都不红。**

### 它的牙在哪一刀 —— 逐条

| 判据 | 掏空态为什么仍绿 |
|---|---|
| `every_file_under_backend_is_registered_with_a_reason` | 它比的是**目录内容 ↔ 登记表**。掏空态里两边**逐字一致**（盘上 `mod.rs`、表上 `mod.rs`）⇒ 一致就是它要的答案。它的牙在 M3 / M4a / M4b 那三刀上，**不在「这条线上有没有真住户」这一问上**。 |
| `every_file_under_backend_lives_on_a_capability_line` | 它逐条问「登记的能力线与路径前缀一致吗」。掏空态里 `observe/mod.rs` 登记 `observe`、前缀 `observe/` —— **一致**。它的牙在 M1 那一刀上。 |
| `each_registered_status_still_matches_reality`（`observe/` 那格） | 🔴 **它的现场量法是一句裸 `root.join("src-tauri/src/backend/observe").is_dir()`** ⇒ 空目录照样 `true` ⇒ 文档说「已交付」= 现场说 `true`，对上了。它的牙在 M5 那一刀上（**文档与量法漂开**），**不在「目录里有没有东西」上**。 |

### 🔴 这不是「7u 仪式性地全绿」，这是一条**盘上真实存在的缺口**，且它有明确住址

`doc_claim_registry.rs` **自己在隔壁那一行写着这条病的诊断**，逐字：

> `⚠ \`control/\` 那格刻意不是裸 \`is_dir\`：目录空着也算「有目录」，
> 而这一格要主张的是**控制面真的住进来了** ⇒ 钉住那个唯一的分流器在里面。`

⇒ `control/` 那格量的是 `backend/control/daemon_route.rs` **这份文件在不在**；
而**紧挨着的 `observe/` 那格量的是裸 `is_dir()`** ——
**同一段代码里，写下那条警告的人在下一行就犯了它警告的那件事。**

**这一格归 PM 裁，本轮没动** —— 理由是硬的：`src-tauri/src/doc_claim_registry.rs` **不在本件写区**
（写区 10 项里没有它）。一句话的修法已经现成（照 `control/` 那格的形）：

```rust
"monitor-backend-observe-landed" => (
    root.join("src-tauri/src/backend/observe/local_query.rs").is_file(),
    "monitor 侧 `backend/observe/` 在，且那个唯一的读面传输住在里面",
),
```

⚠ 同族还有两格**没量过**：`monitor-backend-platform-landed` / `monitor-backend-common-landed`
也都是裸 `is_dir()`。它们今天都是 `false`（目录不存在）⇒ **今天不会说假话**，
但**建目录的那一刻就会**，与 `observe/` 这一格 09-12 的遭遇逐字同形。

---

## 四 · CRASH 单列（**不进判定行读数**）

| # | 什么时候 | 现象 | 处置 |
|---|---|---|---|
| 1 | 刚 `git mv` 完、还没改跨线引用时 | `cargo check --lib` **rc=101**，`error[E0433]: cannot find \`local_backend\` in \`super\`` **3 处**（`observe/local_query.rs:68/69/70`） | 根因：`super::local_backend::…` 原先能解析是因为两份文件同住 `control/`。改成 `crate::backend::control::local_backend::…`。**按 CRASH 记，不算一次判定行读数。** |

---

## 五 · 顺带量到的一条结构事实（要 PM 知道，不自批）

那次修复**造出了 monitor 侧第一条跨能力线的边**：`observe/local_query.rs → control/local_backend.rs`。

- **方向是对的**：daemon 侧 `remote-daemon-proto/src/layering_guard.rs` 头注逐字
  「`observe/`（读）与 `control/`（改变世界）之间**只许一个方向**：`observe → control`，
  且**接口面必须显式列举、条数被钉住**；反向一条都不许。」
- 🔴 **但 monitor 侧没有对应物**：daemon 那侧有 `ALLOWED_OBSERVE_TO_CONTROL`（一张逐条列举、
  条数钉死的表）＋ `control_layer_must_not_reference_observe`（反向零容忍）；
  **monitor 侧这两条判据一条都不存在** —— `backend/mod.rs` 今天只有「文件住在哪条线上」，
  没有「线与线之间谁能引用谁」。
- ⇒ 如实记为**诚实边界**，已逐字写进 `observe/local_query.rs` 那处引用点的注释（写区内）。
  **要不要在 monitor 侧立同款登记表，归 PM 裁**（本件 `预登记` 押 cargo 不涨，且这属于新射程）。

---

## 六 · 量具原文（复跑用）

本轮量具**只有一份**，住会话 scratchpad（`kr71-box`），逐字：

```bash
#!/usr/bin/env bash
# K-R71 专用：在 ccmon-devbox 沙箱里跑任意命令（挂载/隔离与 .claude/devbox/gate 逐条相同）。
# 用法： kr71-box '<bash 命令>'
set -o pipefail
PROJ=/home/zbl/文档/claudecode-frontend
SKILL=/home/zbl/.claude-accts/z/skills/planned-build
WT="$PROJ/.claude/worktrees/k-r71"
TARGETS="$PROJ/.claude/pm-targets"
CARGO_CACHE=ccmon-cargo-registry
exec docker run --rm \
  --network none \
  -v "$PROJ:$PROJ" \
  -v "$SKILL:$SKILL:ro" \
  -v "$CARGO_CACHE:/opt/rust/cargo/registry" \
  -e "CARGO_TARGET_DIR=$TARGETS/k-r71" \
  -e HOME=/home/zbl \
  -e PB_WS=backend-consolidation \
  -w "$WT" \
  ccmon-devbox:latest \
  bash -o pipefail -c "mkdir -p \"\$HOME/.claude/projects\" && $1"
```

⚠ **被测对象恒指 `k-r71` 这棵树**（写死在 `WT`），target 恒是 `.claude/pm-targets/k-r71` ——
风险 `5k`（同名量具被别人覆盖成指向另一棵树）在这里靠「名字带件号 ＋ 树写死」挡。

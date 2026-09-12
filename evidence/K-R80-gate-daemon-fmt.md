# `K-R80` 门禁的 `fmt` 格盖不到 daemon 那棵树 —— 读数与刀

> 工作树 `.claude/worktrees/k-r80` · 分支 `track/k-r80` · 基点 `be271a0`
> 沙箱 `ccmon-devbox:latest`（`K31`：一切开发动作在沙箱里跑，宿主上不跑）
> 本树**未铺** `src-tauri/embedded-daemons/` —— 门禁那行「分母」自报家门与本读数一致

★ 本件买的一句话：**一格「我盖不到那儿」的诚实注释，被当成了处置。**
`fmt` 那一格的分母里逐字写着「`remote-daemon-proto` 是另一个 workspace，本行盖不到」，
那句话**每趟门禁都印**，而那棵树的 `cargo fmt --check` 在基点上就是红的。

---

## §1 `KR80D1` 门禁盖得到 daemon 那棵树的格式

### 落点（最小面）

`scripts/gate.sh` **多一格**，紧挨在 `fmt` 那一格下面：

```
run_gate fmt-daemon '不是数出来的数：`cargo fmt --check` … 唯一成员 `cc-monitor-remote` …' \
         bash -c 'cd remote-daemon-proto && cargo fmt --check 2>&1 && echo "fmt-daemon: 1 passed"'
```

🔴 **要的是多一格，不是并成一棵** —— 两棵树的 workspace 关系**一个字没动**
（`remote-daemon-proto/Cargo.toml` 与 `src-tauri/Cargo.toml` 都不在本件写区，实测也确实没动）。

### 🔴 死值验：在 daemon 树里故意改坏一处格式 ⇒ 门禁必须红

**变异**：`remote-daemon-proto/src/observe/fs.rs` 第 26 行缩进 4 → 6 空格
（挑这一份的理由：它 32 行、**没有任何 `include_str!` 扫它自己**，
不会把「排版红」和「某条扫描型守卫红」混在一起）。
**锚点** `    std::fs::metadata(p)\n        .and_then(|m| m.modified())\n` ——
下刀前断言 `count() == 1`，实得 **1**。
**活体信号（㉒）**：`cargo fmt --check` 逐字点名 `…/observe/fs.rs:23:` 并把那两行 `-`/`+` 打出来
⇒ 它读的是**盘上这一份**，不是缓存。

整趟门禁（`PB_WS=backend-consolidation .claude/devbox/gate … k-r80`）的裁决行：

```
GATE: FAIL —— fmt-daemon（退出码 1）
```

🔴 **同一趟里另外 11 格全绿** —— 这一条比「新格红了」更值钱：

| 那一趟 | 读数 |
|---|---|
| `fmt`（src-tauri） | ok 1 passed |
| **`fmt-daemon`** | 🔴 **FAIL（退出码 1）** |
| `winchk` | ok 1 passed |
| `cargo` | ok **1544** passed（8 个包合计） |
| `generated` | ok |
| `daemon` | ok **694** passed |
| `npm` | ok **1688** passed |
| `ccm e2e` ×4 | 12 / 8 / 46 / 45（全 `exact`） |
| `pb check` | ok `FAIL=0 BROKEN=0` |

⇒ ★ **一趟读数同时买到两件事**：① 新格逮得住；
② **旧的 11 格一格都逮不住** —— 那个盲区是真的，不是推的。
（这就是件计划里「今天不红，那就是题面」那句话的读数版。）

### 诊断印不印得出来（`K-R22` / `N-G1` 那条纪律）

红的那一格自带为什么红 —— 输出只有 9 行，走的是 `gate_diag` 的第 ③ 支（短输出全印）：

```
  ---- fmt-daemon 失败原文（全 9 行）----
  | Diff in …/remote-daemon-proto/src/observe/fs.rs:23:
  | -      std::fs::metadata(p)
  | +    std::fs::metadata(p)
  ---- fmt-daemon 诊断完 ----
```

⚠ 模式表里 `^Diff in ` 那一形是 09-10 加 `fmt` 那一格时补的，本格**直接受益、没有再改模式表**。

### 🔴 一条现打的坑：**不许顺手加 `--all`**

在 `remote-daemon-proto` 下 `cargo fmt --all --check -v` 读它真喂给 rustfmt 的那串文件：
**12 个 crate 根，11 个在这棵树外** —— `src-tauri/build.rs` · `src-tauri/src/lib.rs` ·
`src-tauri/src/main.rs` · 7 个 `crates/*-core/src/lib.rs`，以及
🔴 **`src-tauri/vendor/code-picture-core/src/lib.rs`**。
（成因：那棵树的 path 依赖指进 `../src-tauri/`，`--all` 顺着它们走出去；
而 `cargo metadata --no-deps` 的 `workspace_members` 现打**只有 1 个** —— 两者不是一回事。）

⇒ 加 `--all` 会把 vendor 那棵**我们无权修**的树拉进出货门禁，与 `cargo` 那格
`--exclude code-picture-core` 要避开的是同一件事（`C7`：vendor 不动）。
**不加 `--all`** 时同一趟 `-v` 现打 rustfmt 只收 `remote-daemon-proto/src/main.rs` 一个根，
读数 6 处不变 ⇒ **少的只有别人家那棵树。**

### `ci.yml` 不用改 —— 这一格是「补本机，不是补 CI」

`.github/workflows/ci.yml` 的 `daemon` job **早就有** `cargo fmt --check`
（`working-directory: remote-daemon-proto`，与本格逐字同一条命令）。
⇒ 本件补的是 `fmt` 那格头注自己写着的那句话的反面：
**「本机门禁不是云端的超集，而『绿』这个字在两边长得一模一样。」**
那条「棘地板要三处一起改」的纪律与本行无关（本行不带地板）。

---

## §2 `KR80D2` 那 6 处存量清掉，而且清的是格式不是内容

### 第一条死值验：`cargo fmt --check` **6 处 ⇒ 0 处**

| 时点 | `remote-daemon-proto` 下 `cargo fmt --check` |
|---|---|
| 基点 `be271a0`（本件开工前，沙箱现打） | 🔴 **rc=1 · 6 处 / 3 文件** —— `agents/mod.rs:131` · `control/ccm/argv.rs:327/412/431/442` · `protocol_doc_guard.rs:599` |
| 本件之后 | ✅ **rc=0 · 0 处** |

处置就是在那棵树下跑一次 `cargo fmt`（**不带 `--all`**，理由见上）；
`git status` 现打**只有那 3 份被改**，一份不多。

### 🔴 第二条死值验：`daemon` 那一格读数逐格等于基线 **694**

| 格 | 基线（预登记） | 本件实得 | 判 |
|---|---|---|---|
| `daemon` | **694 ± 0** | **694** | ✅ 逐格相等 —— **没越界** |

★ 这一条是**行为面**的证据：判据一条不多、一条不少地照跑。

### 第三条（本件自己加的）：**文本面**的刀

`daemon 694` 买不到「一个不改变任何判据结果的内容改动」（改一句注释、挪一个参数）。
⇒ 量具 `evidence/K-R80-fmt-content-unchanged.py`（可复跑）：
把 `git show be271a0:<文件>` 与盘上那份**归一**后逐字节比 ——
① 字符串/字符字面量**原样抠出来逐条对比**；② 抠掉之后删全部空白、再削掉紧挨 `)]}` 的尾随逗号。

| 文件 | 原文字节 | 今天字节 | 字面量（前/后） | 尾随逗号净增 | 归一后 |
|---|---|---|---|---|---|
| `remote-daemon-proto/src/agents/mod.rs` | 21227 | 21254 | 48 / 48 | +0 | **逐字节相同** |
| `remote-daemon-proto/src/control/ccm/argv.rs` | 22120 | 22234 | 145 / 145 | **+2** | **逐字节相同** |
| `remote-daemon-proto/src/protocol_doc_guard.rs` | 68326 | 68372 | 230 / 230 | +0 | **逐字节相同** |

⚠ 那 **+2** 个尾随逗号是 rustfmt 换行时补的（`die(` 那一处、`for bad in [` 那一处）——
**是排版不是内容**，所以归一器把它削掉；如实登记在这里，不藏。

### ⚠ 这把刀**不是地板** —— 三条死值验（`K-R79` 那一课）

| 变异 | 锚点（下刀前 `count()`） | 判词 |
|---|---|---|
| `N1` 改一个**字符串字面量**的内容：`"--account 与 --base 互斥"` → `…冲突"`（argv.rs） | `count()==1` | 🔴 `C1 …字面量变了：… [('"--account 与 --base 互斥"', '"--account 与 --base 冲突"')]` |
| `N2` 改一处**非字面量**内容：`why.len() > 40,` → `> 41,`（protocol_doc_guard.rs） | `count()==1` | 🔴 `C2 …归一之后仍然不同 —— 头一处分歧在归一串第 6651 字节` |
| `N3` 把 `agents/mod.rs` **退回基点**（原文一个字没变） | 整份替换 | 🔴 `C0 …与基点逐字节相同 —— 那这一格是地板，不是判据` |

⚠ `N3` 就是**非空对照**那一条：文件没被改过时「归一后相等」永远满足 ⇒ 必须有 `C0` 顶着。
三刀跑完**逐份 md5 复原核对**：`argv.rs` `bcaf6d50…` 与 `pdg.rs` `524a85ce…` 前后同值。

### ⚠ 它买不到什么

· 不判 rustfmt 改得对不对，只判「改的只有空白与尾随逗号」。
· 字面量抠法是正则不是 Rust 词法器（`'a'` 与生命周期 `'a` 有歧义）——
  但**两侧用同一个归一器**，认错也认得一样 ⇒ 只可能漏报，不会误报。

---

## §3 `KR80D3` 「门禁还有哪些格盖不到」有读数

量具：`evidence/K-R80-gate-cell-coverage.py`（可复跑；`python3 … [<gate.sh 路径>]`）。

### 🔴 先订正一个**腐掉的数**：不是「九格」，现打是 **11 格**（本件之后 12 格）

件计划与派工单说的「门禁九格」出自 `gate.sh` 末尾那行裁决的点名
「三道门 + 生成物漂移 + `pb check` + 四套 `ccm` e2e」= 3+1+1+4 = **9**。
而那行 **09-10 加 `fmt` / `winchk` 两格时没跟着改** ⇒ 盘上真有 **11** 格，它只点得出 9 格。
⇒ 本件把它订正成「**12 格全绿（逐格点名）**」，并且**不让它再自己烂下去**：
量具的 `C5` 拿那一行里的数与「本文件真有几格判定」对拍，加了格而点名没跟 ⇒ 红。

### 登记（12 格 × 12 棵树，逐格点名它盖不到的那些）

| 格 | cwd | 全 | 部 | 🔴 盖不到 |
|---|---|---|---|---|
| `fmt` | `src-tauri/` | — | `src-tauri/` | **11** |
| `fmt-daemon` | `remote-daemon-proto/` | — | `remote-daemon-proto/` | **11** |
| `winchk` | `src-tauri/` | — | `src-tauri/`·vendor | **10** |
| `cargo` | `src-tauri/` | `src-tauri/` | `remote-daemon-proto/`·`e2e/`·`scripts/`·`shared/`·`doc/`·`.github/` | **5** |
| `generated` | 仓根 | — | `src/` | **11** |
| `daemon` | `remote-daemon-proto/` | `remote-daemon-proto/` | `src-tauri/`·`e2e/`·`doc/`·`.github/` | **7** |
| `npm` | 仓根 | `src/` | `src-tauri/`·`remote-daemon-proto/`·`e2e/`·`scripts/`·`shared/` | **6** |
| `ccm e2e/ccm-print-parity` | 仓根 | — | `remote-daemon-proto/`·`e2e/` | **10** |
| `ccm e2e/ccm-rbind-title` | 仓根 | — | `remote-daemon-proto/`·`e2e/` | **10** |
| `ccm e2e/ccm-cli` | 仓根 | — | `remote-daemon-proto/`·`e2e/` | **10** |
| `ccm e2e/ccm-contract-parity` | 仓根 | — | `remote-daemon-proto/`·`e2e/` | **10** |
| `pb check` | 仓根（查的目录**在仓外**） | — | — | **12** |

**树的全集 12 棵**，分母不是拍脑袋定的：`C4` 拿 `git ls-files -z` 现打的**顶层目录**
（10 棵）对拍，另加两格 —— `src-tauri/vendor/code-picture-core/`（**从 `src-tauri/` 单拆**，
它受不同的门管：`cargo` 那格显式 `--exclude` 它）与 `<仓根文件>`。
逐格逐树的理由（`C6`：一格不许空）由量具打印，不在本文复述一份（复述就会漂）。

### 🔴 转置之后掉出来的那一条

| 树 | 盖到它的格数 |
|---|---|
| `remote-daemon-proto/` | 8（本件之前 7） |
| `e2e/` | 7 · `src-tauri/` 5 · `src/` 2 · `scripts/` 2 · `shared/` 2 · `doc/` 2 · `.github/` 2 · vendor 1 |
| 🔴 `hooks/` · `evidence/` · `<仓根文件>` | **0** |

**这 3 棵树今天 12 格里一格都盖不到。**
⚠ 件计划 `§0d` 逐字「不去补 `KR80D3` 数出来的其它盲区」⇒ **只数不补**，补是另一件。

### ⚠ 这份登记买得到什么、买不到什么

**买得到**（`C1`–`C6`，任一条不满足 `exit 1`）：格与登记两侧互为子集 · 每条登记的逐字锚点
在 `gate.sh` 里 `count()==1` · 每格对每棵树都有裁词且取值在闭集 `全/部/无` 里 ·
树的全集与 `git ls-files` 对得上 · 裁决行的格数对得上 · 每个裁词都带非空理由。
**买不到**：🔴 **它不判裁词对不对** —— 一条**写错的**裁词能骗过它。
**这是登记的机检，不是覆盖率的判据**，别把绿读成后者。

### ⚠ 它也不是地板 —— 六条死值验

| 变异 | 判词 |
|---|---|
| `M1` `gate.sh` 尾部加一格 `run_gate zzz` | 🔴 `C1 …有格没登记：['zzz']` ＋ `C5 …自称 12 格，而现打是 13 格` |
| `M2` 删掉 `fmt-daemon` 那一格 | 🔴 `C1 …登记里有格盘上没有：['fmt-daemon']` ＋ `C2 …锚点命中 0 次` ＋ `C5` |
| `M3` 裁决行的数改成 11 | 🔴 `C5 …自称 11 格，而现打是 12 格` |
| `M4` 把 `daemon` 那格锚点措辞改掉 | 🔴 `C2 daemon 的逐字锚点…命中 0 次（应当恰好 1 次）` |
| `M5` 登记里漏一棵树的裁词 | 🔴 `C3 … 对这几棵树没有裁词：['hooks/']`（11 格各报一次） |
| `M6` 把裁词的理由掏空 | 🔴 `C6 …没带理由 —— 空白格不算点名`（113 处） |

⚠ `M5` 第一版是**崩在 `KeyError`** 上的（红了，但 `C3` 那句话一个字印不出来）——
改成 `verdict_of()` 之后才既红又说得清。**红而说不清 ≈ 没红**，这条与 `K-R22` 同源。

### ⚠ 一条**量具自己的**病，现打逮到并订正

`C4` 第一版用裸 `git ls-files` 按换行切 —— 本仓有中文文件名，git 默认**加引号并转义**
⇒ 顶层目录被切成 `"doc/` 与 `"evidence/`，`C4` 当场报「盘上多出两棵树」。
**那是尺子的病，不是盘上的事实**（本仓最高频那族：量具的作用域对不上事实）。
换成 `git ls-files -z` 之后归零。

---

## §4 `7u` —— 把实现整个退掉，还有多少条新断言仍绿

**新断言三组**：① `gate.sh` 的 `fmt-daemon` 一格 · ② 覆盖登记的 `C1`–`C6` ·
③ 文本面的 `C0`–`C2`。逐处退：

| 退掉哪一处 | 哪几条仍绿 | 理由 |
|---|---|---|
| 退掉**那 3 份的排版修复**（文件回基点），留新格 | **0 条** | ① 当场红（`cargo fmt --check` 6 处，实测就是基点读数）；③ 的 `C0` 当场红（原文与基点逐字节相同 = 地板）——`N3` 已实打 |
| 退掉 **`gate.sh` 那一格**，留排版修复 | **0 条** | ② 的 `C1`/`C2`/`C5` 三条同时红 —— `M2` 已实打 |
| 退掉 **`gate.sh` 裁决行那个数**（改回 9/11） | ② 只剩 `C5` 红 | 其余五条与那个数无关；`M3` 已实打，红 1 条 —— **不是空真，是射程本来就窄** |
| **全退** | **0 条** | 三组各自被上面两行覆盖 |

⇒ **没有一条新断言在实现退掉之后还绿。** 空真 0 条。

---

## §5 改动面（最小面）

| 文件 | 改了什么 | 谁在守 |
|---|---|---|
| `scripts/gate.sh` | 加 `fmt-daemon` 一格（＋头注）· `fmt` 那格分母尾部加一句「那一棵由下面 `fmt-daemon` 那一格盖」· 裁决行点名订正到 12 格 | 覆盖登记的 `C1`/`C2`/`C5` |
| `remote-daemon-proto/src/agents/mod.rs` | 只有排版（`account_env_of` 那条链换行） | `KR80D2` 三条死值验 |
| `remote-daemon-proto/src/control/ccm/argv.rs` | 只有排版（`die(` 一处 · 三条 `#[test]` 里的断言换行） | 同上 |
| `remote-daemon-proto/src/protocol_doc_guard.rs` | 只有排版（一条 `assert!` 换行） | 同上 |
| `evidence/K-R80-gate-cell-coverage.py`（新） | `KR80D3` 的登记 ＋ 机检 | 自带 6 条死值验 |
| `evidence/K-R80-fmt-content-unchanged.py`（新） | `KR80D2` 的文本面刀 | 自带 3 条死值验 |
| `evidence/K-R80-gate-daemon-fmt.md`（新） | 本文 | — |

**没动**：两棵树的 `Cargo.toml`（不并 workspace）· `.github/workflows/ci.yml`（它早有这一步）·
那 3 份文件的任何内容 · `KR80D3` 数出来的其它盲区。

**编译错（CRASH）单列**：本件 **0 次**（排版改动不触碰编译；`cargo` 1544 / `daemon` 694 两格
在变异那一趟就已经与基线逐格相等）。

---

## §6 三处 `git status --porcelain`

**开跑前 · 计划仓 `.claude/planned-build`**：9 条 `?? backend-consolidation/.briefs/*.md`
（`K-R48b` `K-R48c` `K-R53` `K-R54` `K-R55` `K-R56` `K-R57` `K-R58` `K-R60`），无别的。

**收工后 · 计划仓**（末趟门禁那一刻，提交前）：

```
 M backend-consolidation/DECISIONS.md                 ← PM 的（在飞期间）
 M backend-consolidation/INDEX.md                     ← PM 的（13:46:10 跑了 pb index）
 M backend-consolidation/features/K-R80-….md          ← 本件（§3 + §8b）
 ?? backend-consolidation/.briefs/*.md                ← 开跑前就在的那 9 条
 ?? backend-consolidation/features/K-R81-….md         ← PM 的（在飞期间新立的件）
```

⚠ **件文件 `§8b` 那一行只点了本件自己那一份** —— 上面这三条 PM 的
（`DECISIONS.md` · `INDEX.md` · `K-R81`）是 PM **在飞期间**动的，按 `R40 裁定零`
那条常驻纪律，那是 PM 的动作、不是环境异常。**这里补齐点名，不含糊。**
🔴 **`§8b` 那一行没回去改，理由是硬的**：`[J3 陈账]` 判的是 mtime ——
再动一次件文件就必然让 `pb check` 那格红（`K-R10` 记的那条死锁，`K-R11` 落地才消失）
⇒ **改一行文字要赔掉一个 `GATE: OK`**。订正落在这里
（`evidence/` 在本件自己数出来的登记里是 **0 格覆盖**，改它不动任何一格读数）。

**收工后 · 代码仓主树 `cc-monitor`**：**空**（本件一个字节没落在主树上）。

**收工后 · 工作树 `k-r80`（提交前）**：

```
 M remote-daemon-proto/src/agents/mod.rs
 M remote-daemon-proto/src/control/ccm/argv.rs
 M remote-daemon-proto/src/protocol_doc_guard.rs
 M scripts/gate.sh
?? evidence/K-R80-fmt-content-unchanged.py
?? evidence/K-R80-gate-cell-coverage.py
?? evidence/K-R80-gate-daemon-fmt.md
```

---

## §7 门禁末趟（`GATE: OK`）—— 逐字

```
  ok   fmt            1 passed（分母：… `remote-daemon-proto` 是另一个 workspace，本行盖不到（那一棵由下面 fmt-daemon 那一格盖））
  ok   fmt-daemon     1 passed（分母：… 唯一成员 `cc-monitor-remote`；… 刻意不加 --all）
  ok   winchk         1 passed
  ok   cargo          1544 passed（8 个包合计）
  分母 cargo          本树未铺 src-tauri/embedded-daemons/ ⇒ …少了「本地后端真的能起来吗」那一族（4 条）
  ok   generated      与 Rust 源一致（跑过上面那道 cargo 门之后再判的）
  ok   daemon         694 passed
  ok   npm            1688 passed
  ·    e2e 前置       build 后端二进制（四套 ccm e2e 的被测对象）
  ok   ccm e2e        ccm-print-parity       PASS=12（地板 12，恒等）
  ok   ccm e2e        ccm-rbind-title        PASS=8（地板 8，恒等）
  ok   ccm e2e        ccm-cli                PASS=46（地板 46，恒等）
  ok   ccm e2e        ccm-contract-parity    PASS=45（地板 45，恒等）
  ok   pb check       [backend-consolidation] ===== pb check: FAIL=0 BROKEN=0 =====

GATE: OK —— 12 格全绿（fmt · fmt-daemon · winchk · cargo · generated · daemon · npm · 四套 ccm e2e · pb check），可以出货
```

| 预登记 | 实得 | 判 |
|---|---|---|
| `cargo` 1544 ± 0~3 | **1544** | ±0 |
| 🔴 `daemon` **694 ± 0** | **694** | 逐格相等 —— 没越界 |
| `npm` 1688 ± 0 | **1688** | ±0 |
| `e2e` 恒等 | **12 / 8 / 46 / 45** | 恒等 |
| 🔴 **那格新读数**（不在四格里） | `cargo fmt --check` **6 处 ⇒ 0 处** · 门禁新格 `fmt-daemon` **ok 1 passed** · 变异那一趟 `GATE: FAIL —— fmt-daemon（退出码 1）` | 兑现 |

⚠ **预登记里那句「本件多半只涨 `cargo` 那一格里的判据数」实测是 0** ——
本件一条 Rust 判据都没加（新格住 `scripts/gate.sh`、两把尺子住 `evidence/`），
`cargo` 因此**一动没动**。预登记是**预测**，这里如实记它没兑现的那一半。

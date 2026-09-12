# `K-R82` —— `hooks/` 会被执行，而 12 格里 0 格看着它

> 量于工作树 `k-r82`（分支 `track/k-r82`，基点 `880b864`），沙箱 `ccmon-devbox:latest`。
> **本树未铺 `src-tauri/embedded-daemons/`** ——「铺没铺」是 cargo 那个合计的第二维，见门禁那行「分母 cargo」。

## §0 来路

`K-R80` `KR80D3` 那份转置表逐字印出：**`hooks/` · `evidence/` · 仓根文件三棵树，12 格里 0 格覆盖**。
`DECISIONS.md#R42` 裁定四拍：**`hooks/` 立件**（它**会被 git 执行**、跑在每一次提交上、**能改仓**，
与另两棵性质不同），另两棵**明写「不需要门」**。

---

## §1 现打①：git 对一个「不可执行的 hook」到底怎么处置

⚠ **这一节推翻了本实现方自己写的第一版头注。** 第一版逐字写着「一个字都不印」——
现打之后发现是错的，已在 `scripts/hooks-are-runnable.sh` 头注里订正。

**量法**：沙箱里 `mktemp -d` 起一个合成 git 仓，`core.hooksPath hooks`，
hook 内容是 `echo SENTINEL-HOOK-RAN >&2; exit 1`，三种状态各提交一次。
**`git version 2.43.0`**（沙箱 `ccmon-devbox:latest`）。

| hook 的状态 | git 干了什么 | 退出码 | 提交进去了吗 |
|---|---|---|---|
| **644（无可执行位）** | 只打一句 `hint: The 'hooks/pre-commit' hook was ignored because it's not set as executable.` | **`rc=0`** | **进去了** |
| 755 + 语法好 | 真跑了（哨兵出现在 stderr） | `rc=1` | 被挡住 |
| 755 + 语法坏 | 解释器报 `Syntax error: end of file unexpected (expecting "fi")` | `rc=1` | 被挡住 |

🔴 **第一行才是本件的病灶形状**：那句 `hint` **是提示不是失败**，而且
`git config advice.ignoredHook false` 就能关掉 ⇒ **闸门整个不在了，退出码却一切正常。**
第三行反而是 fail-closed 的 —— 但它把**每一次提交**都变成一次报错，同样得有东西红。

## §2 现打②：本格落地那一趟，**当场逮到一条真的**

`scripts/hooks-are-runnable.sh` 第一次跑（在任何修复之前），工作树 `k-r82`：

```
  · hooks/pre-commit         盘上 不可执行 · 库里 100644 · 解释器 sh · md5 d2836ce5
✗ hooks/pre-commit **盘上没有可执行位** …
✗ hooks/pre-commit **库里记的是 100644，不是 100755** …
hooks: FAIL=2（通过 9 条）
rc=1
```

**两句话分别是什么**（记忆条 `filemode-false-chmod-invisible`）：

| 问的是 | 主树 `cc-monitor` | 工作树 `k-r82`（新 checkout） | 库里（index） |
|---|---|---|---|
| 盘上跑不跑得起来 | `-rwxrwxr-x` ✔ | **`-rw-rw-r--` ✘** | — |
| 库里记没记 | — | — | **`100644` ✘** |

⇒ 本仓 `core.filemode=false`（现打 `git config core.filemode` ⇒ `false`），
`chmod +x` **从来没进过 git**。主树上它能跑，**是因为那份文件是在主树上带着 +x 建出来的**；
而 index 里是 644 ⇒ **每一棵新开的工作树 checkout 出来都是 644**，
`C7` 那条「`[profile.dev]` 不许进提交」的机器挡**在那些树里等于不在**。

**修法**（只此一条，别用 `chmod` 指望它进库）：

```
chmod +x hooks/pre-commit                    # 盘上这一句
git update-index --chmod=+x hooks/pre-commit # 库里这一句
```
修完复打：`hooks: 11 passed`（`rc=0`）。

---

## §3 `KR82D1` 的刀 —— 「跑得起来」这件事真的有人守

判据本体 `scripts/hooks-are-runnable.sh`，接进 `scripts/gate.sh` 的**第 13 格 `hooks`**。

### §3a 判据层：6 把刀 + 1 条反向对照

夹具是 `mktemp -d` 里造的**真 git 仓**（`core.filemode false`，与本仓同配置），
`hooks/pre-commit` 从真工作树拷进去。**一趟都不碰工作树**（红线 ㉑）。
刀本身住 `scratchpad/r82/d1-knives.sh`。

| 刀 | 变异 | 判据读数 | rc |
|---|---|---|---|
| **M-D1z** | 无（反向对照） | `hooks: 11 passed` | 0 |
| **M-D1a** | 插一处语法错（`if` 没有 `fi`） | ✗ **语法过不了它自己声明的解释器（sh）**；md5 `d2836ce5`→**`24f9b90b`** | 1 |
| **M-D1b** | 盘上 `chmod -x`（库里仍 100755） | ✗ **盘上没有可执行位**（那一行同时印出「库里 100755」⇒ 两句话真的分开了） | 1 |
| **M-D1c** | 库里 `update-index --chmod=-x`（盘上仍 +x） | ✗ **库里记的是 100644**（同上，反向） | 1 |
| **M-D1d** | 跟踪着但盘上删了 | ✗ 盘上没有可执行位 ＋ ✗ 解释器 `<盘上读不到>` | 1 |
| **M-D1e** | `hooks/` 下一个跟踪文件都没有 | ✗ **分母是空的，三条判据全成空真（不许当成绿）** | 1 |

★ **`M-D1b` 与 `M-D1c` 是本条 dod 的核心**：同一份文件、同一个 md5，
一把刀只动盘上、一把刀只动库里，**两句话各红各的**。
★ **`M-D1e` 挡的是空真**（`brief` 第 9 条那一族）：`hooks/` 空掉时三条判据都恒真。
★ **一条 `test -e` 都没有** —— 件计划点名的失效方向（「只判文件在不在」）在判据里没有住址。

### §3b 判据自己的阳性对照（每趟都跑，不是只在死值验那天跑）

`K-R79` 的教训（自检写成地板）+ `K-R81` 的解法（自带阳性对照）。
三把尺子**正反各一条**，共 8 条，跑在合成夹具上，与真判据**走同一个函数**（不另写一份）：

| 条 | 挡的是 |
|---|---|
| `S1` 语法尺子对坏语法**放行** | 尺子瞎了 |
| `S2` 语法尺子对好语法**也红** | 尺子恒红（恒红的尺子会被人关掉） |
| `S3` 盘上尺子对没有 +x **放行** | 同上 |
| `S4` 盘上尺子对有 +x **也红** | 同上 |
| `S5` 库里尺子把 `100644` 当成记着了 | **正是本件病灶的形状** |
| `S6` 库里尺子对 `100755` 也红 | 恒红 |
| `S7` 闭集外的解释器**放行** | 静默跳过 = 没查 |
| `S8` 没有 shebang **放行** | 同上 |

⇒ `11 = 1 个 hook 文件 × 3 条 + 8 条阳性对照`。**这个数是数出来的**，
往 `hooks/` 加一份 hook 它就涨 —— 与 `fmt` 那几格「只有绿/红两态」不同。

### §3c 门层：**整趟门禁真的红了一次**（不是靠推）

变异：`git update-index --chmod=-x hooks/pre-commit`（只动库里那一句），
其余一个字节没改，跑完整 13 格。**读数**（`scratchpad/r82/gate-mut-index.txt`）：

```
GATE: FAIL —— hooks（退出码 1）
```
同一趟**另外 12 格全绿**：`fmt 1 · fmt-daemon 1 · winchk 1 · cargo 1551（8 个包合计）·
generated ok · daemon 694 · npm 1688 · ccm e2e 12/8/46/45 · pb check FAIL=0 BROKEN=0`。
诊断段（`K-R22` 那半）把判据原文端出来了，含活体信号：

```
  ---- hooks 失败原文（全 3 行）----
  |   · hooks/pre-commit         盘上 可执行 · 库里 100644 · 解释器 sh · md5 d2836ce5
```

★ **一趟读数同时买到两件事**：新格**逮得住**，且**它逮的不是别的格已经逮得住的东西**
（另外 12 格一格没动）。变异后已 `--chmod=+x` 还原，还原读数见 `§7`。

---

## §4 `KR82D2` 的刀 —— 由那份转置表**自己的 `C1..C6`** 接住

🔴 **没有另起一份「hooks 覆盖登记」**（件计划点名的失效方向）。
新格只在 `evidence/K-R80-gate-cell-coverage.py` 的 `REGISTRY` 里加了**一条 `cell(...)`**，
一条新判据都没另立 —— 接住它的是那份表本来就有的 `C1`（名字两侧互为子集）·
`C2`（逐字锚点 `count()==1`）· `C3`/`C6`（12 棵树逐棵表态带理由）· `C5`（裁决行格数三方对拍）。

| 刀 | 变异 | 那份表的读数 |
|---|---|---|
| 反向对照 | 无 | `KR80D3: OK —— C1..C6 全过；C6b 全过` |
| **M-D2a** | `gate.sh` 里把 `run_gate hooks` 注释掉 | `FAIL=2`：`C1 登记里有格盘上没有：['hooks']` ＋ `C5 GATE: OK 那行自称 13 格，而现打是 12 格` |
| **M-D2b** | 登记里把那条 `cell("hooks", …)` 删掉 | `FAIL=2`：`C1 gate.sh 里有格没登记：['hooks']` ＋ `C6b hooks/ 今天 0 格覆盖，而登记里一条说明都没有` |

★ **`M-D2b` 顺带证了两条判据的联锁**：把新格从登记里摘掉，`hooks/` 当场掉回「0 格覆盖」，
于是 `C6b`（下一节）立刻要求给它一句「为什么不需要门」—— **摘不掉、也糊弄不过去。**

**转置表里 `hooks/` 那一列的前后**：

| | `K-R80` 收官时 | 本件之后 |
|---|---|---|
| `hooks/` 被几格盖到 | **0** | **1**（`hooks`，裁词「全」） |
| 判定格数 | 12 | **13** |
| 0 格覆盖的树 | 3（`hooks/` · `evidence/` · `<仓根文件>`） | **2**（`evidence/` · `<仓根文件>`） |

⚠ 那一格对 `scripts/` 刻意判**「无」**，理由写在登记里：
本格的**尺子**住 `scripts/hooks-are-runnable.sh`，**但尺子不是分母** ——
把量具算进它自己的覆盖，正是本区最高频那族病。

---

## §5 `KR82D3` 的刀 —— 另两棵**明写「不需要门」**，而那句话被钉住

🔴 **「不需要」与「没查」在输出上一模一样** —— 上一版那张表印出的 `0` 一个字都没说是哪一种。
本件在登记里加了 `NO_GATE_NEEDED`，并由 **`C6b`** 逐条钉住。

⚠ **编号是 `C6b`，不是 `C7`** —— 本仓的 `C7` 是宪章那条「vendor 不动」，同名会互相冒充。
它是 `C6`（「一格的裁词都不许空」）换了个方向：`C6` 管**裁词**的理由，`C6b` 管**整棵树 0 覆盖**的理由。

### §5a 两条理由（逐条给，没有一个字是「不重要」）

- **`evidence/`** —— 读数落点，不进构建、不进运行时、不被产品代码读。
  ★ **`0 格覆盖`正是它的用途，不是它的缺陷**：`K-R80` 收官那趟就利用过 ——
  `[J3 陈账]` 判 mtime 造成「改一行件文件就赔掉一个 `GATE: OK`」的死锁，
  它把订正落在这棵树上，正因为**改它不动任何一格读数**。给它加门 = 把那条唯一的泄压口焊死。
- **`<仓根文件>`**（`git ls-files` 里不含 `/` 的那些，现打 **16** 份，量于 `880b864`）——
  ① **跑得到的那几份不是没人管，是没人「覆盖」它们**：`npm` 那一格的命令本体就
  **住在 `package.json` 的 `scripts.test` 里**，`vitest.config.ts` 给它 `include`
  ⇒ 它们是那格的**依赖**，不是那格的**分母**；登记只记分母，所以这里仍判 0，**不是漏登**。
  ② 其余的（`README*.md` / `CHANGELOG.md` / `LICENSE` / `PHASE-G-REPORT.md` /
  `.gitattributes` / 那份审阅报告）是文档与仓库元数据，改坏不改变任何产品行为。
  ⚠ **与 `hooks/` 的区别就在这句：那棵树里的东西会被执行。**

### §5b 6 把刀（每一把都真切过）

| 刀 | 变异 | `C6b` 的读数 |
|---|---|---|
| **M-D3a** | 把 `evidence/` 那条整条删掉 | ✗ `evidence/` 今天 0 格覆盖，而登记里**一条说明都没有** |
| **M-D3b** | 理由换成「不重要」 | ✗ 只有 3 字（下限 60）＋ ✗ **出现了敷衍词 `['不重要']`** |
| **M-D3c** | 钉子（留档里的逐字串）指空 | ✗ `evidence/K-R80-gate-daemon-fmt.md` 里逐字命中 **0 次**（应当恰好 1 次） |
| **M-D3d** | 理由点名的那一格改名 `npm`→`nmp` | ✗ 理由点名了 `nmp` 那一格，而登记里没有这一格 —— 理由指空了 |
| **M-D3e** | `package.json` 里那个 key 搬走 | ✗ `scripts.test-that-moved` 取不到（`KeyError`）—— 那句话不成立了 |
| **M-D3f** | 给一棵**已被盖到**的树写「不需要门」 | ✗ 登记说 `hooks/` 不需要门，而它今天**已经被格盖到了** —— 这条说明陈了 |

★ **`M-D3c`/`M-D3d`/`M-D3e` 是「那句说明本身被钉住」的实体**：
理由里每一处**点名**（一份留档的逐字串 · 一格的名字 · 两份仓根文件 · `package.json` 里一个 key）
都要在盘上真指得到；指空一处就红。**这不是注释，是判据。**

⚠ **`C6b` 买不到什么**（与 `C1`–`C6` 同一条边界）：它**不判理由对不对**。
一条写错但指得到东西的理由能骗过它。它只保证理由**在**、**不敷衍**、**指得到的东西还在盘上**。

---

## §6 最小面 —— 改了 5 个文件，逐个交代为什么

| 文件 | 改了什么 | 为什么非改不可 |
|---|---|---|
| `hooks/pre-commit` | **只改 mode**：`100644` → `100755`（内容 0 字节变化，md5 `d2836ce5` 前后同） | `KR82D1` 第 ② 刀在真盘上就是红的 —— 不修就出不了 `GATE: OK` |
| `scripts/hooks-are-runnable.sh` | **新增**（判据本体 + 8 条阳性对照） | 放在 `scripts/` 而不是塞进 `gate.sh`：死值验要**对着变异过的副本**跑，得能单独传一个仓根进去 |
| `scripts/gate.sh` | 新增第 13 格 `hooks`（一段头注 + 一行 `run_gate`）· 裁决行 `12 格` → `13 格` ＋ 点名 | 裁决行不改 `C5` 当场红（三方对拍） |
| `evidence/K-R80-gate-cell-coverage.py` | 加一条 `cell("hooks", …)` · 加 `NO_GATE_NEEDED` ＋ `C6b` · 孤儿那行的 `12` 改成 `len(cells)` | `KR82D2`/`KR82D3` |
| `scripts/README.md` · `.github/workflows/ci.yml` | 各一条：登记新脚本 · shellcheck 计数地板 `57` → `58` | **被两条已有的 Rust 判据当场逮住**，见下 |

### 🔴 那两条 Rust 判据逮住了我 —— 如实记，因为它们买到的正是本件在讲的东西

第一趟全量门禁 `GATE: FAIL —— cargo（退出码 101）`，两条红：

- `doc_claim_registry::tests::every_script_in_the_directory_is_listed_in_its_readme`
  逐字：``​`scripts/` 里这些文件在 `scripts/README.md` 里查不到：["hooks-are-runnable.sh"]``
- `shell_lint_registry::tests::the_coverage_floor_equals_what_is_actually_covered_today`
  逐字：`CI 的 shellcheck 计数地板写着 57，而那条表达式今天真实覆盖 58 个文件。`

⇒ **新建一个 shell 脚本这件事，本仓已经有两格看着它**（而 `hooks/` 一格都没有 —— 这就是题面）。
两条都按它们自己的诊断落地：README 加一行 · 地板棘到 58 并**把实测构成一起写下**（那条地板自己的规矩）。
**逐组现打，不倒推**（量于工作树 `k-r82`）：
**`58 = e2e/*.sh 31 + e2e/weak-net/*.sh 4 + fake-claude 1 + cc-bus 14 + scripts 3 + vendored 4 + hooks/pre-commit 1`**。
修完复打 `cargo test -p monitor --lib` ⇒ `1431 passed; 0 failed`（此前 `1429 passed; 2 failed`）。

---

## §7 读数

### 四格（预登记 vs 实测）

| 格 | 预登记 | 实测 | 差 |
|---|---|---|---|
| `cargo` | `1551 ± 0~3` | **1551**（8 个包合计） | **±0** |
| `daemon` | `694 ± 0` | **694** | ±0 |
| `npm` | `1688 ± 0` | **1688** | ±0 |
| `ccm` e2e ×4 | 恒等 | **12 · 8 · 46 · 45**（全 `恒等`） | ±0 |

★ 与 `K-R80` 同形：**本件的产出一格都不进四格读数**（判据是 1 份 bash + 1 段 python，不是 Rust `#[test]`）。
⚠ `cargo` 那 1551 的第二维：**本树未铺 `src-tauri/embedded-daemons/`** ⇒ 少了「本地后端真的能起来吗」那 4 条。

### 两格不在四格里的（本件真正的产出）

| 读数 | 前 | 后 |
|---|---|---|
| **① 门禁格数** | **12** | **13**（新格 `hooks`；**不是扩现有格的射程** —— 理由见下） |
| **② 转置表里 `hooks/` 那一列** | **0 格** | **1 格**（`hooks`，裁词「全」）；另两列各有**一行被 `C6b` 钉住**的「为什么不需要门」 |

**为什么是加一格，不是扩现有格的射程**：现有 12 格没有一格的命令进得了 `hooks/`
（登记里 12 条对 `hooks/` 的裁词全是「本格的命令根本不进这棵树」）。
硬扩只有两条路：往 `cargo` 里塞一条 Rust 扫描守卫（它读得到文件，但**读不到「盘上有没有 +x」**
—— `git` 的 index mode 与文件权限它都得自己去问，等于在 Rust 里重造这把尺子），
或往 `npm` 那条 `&&` 链里挂 —— 那会让一条 hook 判据的失败印成「npm 退出码非零」。
**两条都是把一格的读数与另一件事焊在一起**，本区治过。

### 非数量的一格

`fmt-daemon`（`K-R80` 立的）与 `K-R81` 那些新判据 **保持绿**：本趟 `fmt-daemon 1 passed`，
`cargo` 合计 1551 与 `K-R80`/`K-R81` 收官读数同值（±0）。

### `7u` —— 把实现逐块退掉，还有几条新断言仍绿：**0 条**

逐处实打（不是推的）：

| 退掉哪一块 | 谁红 | 读数 |
|---|---|---|
| 退**修复**（`hooks/pre-commit` mode 回 `100644`，判据全留） | 门禁 `hooks` 格 | `GATE: FAIL —— hooks（退出码 1）`，同趟另外 12 格全绿（`§3c`） |
| 退**判据本体**（`scripts/hooks-are-runnable.sh` 搬走，`gate.sh` 那行留） | `hooks` ＋ `cargo` **两格** | `GATE: FAIL —— hooks（退出码 127）；cargo（退出码 101）`，诊断段逐字 `bash: scripts/hooks-are-runnable.sh: No such file or directory`；`shell_lint_registry` 同时红（`58` ⇒ 真实 `57`） |
| 退**门禁那一格**（`gate.sh` 里注释掉） | 覆盖登记 `C1` ＋ `C5` | `KR80D3: FAIL=2`（`§4` M-D2a） |
| 退**登记那一条** | 覆盖登记 `C1` ＋ `C6b` | `KR80D3: FAIL=2`（`§4` M-D2b） |
| 退**「不需要门」那一条** | 覆盖登记 `C6b` | `KR80D3: FAIL=1`（`§5b` M-D3a） |

★ 第二行还顺带买到一件事：**判据文件消失不会退化成静默跳过** —— 它踩响两格，
而且 `run_gate` 的诊断把 `No such file or directory` 原样端出来了。
★ 那一趟也是本轮唯一一次**两格同红**，裁决行的分隔符（`K-G3` 治过的那个坏字节）
现打是好的：`hooks（退出码 127）；cargo（退出码 101）`。

### 编译错（CRASH）：**0 次**

本件一行 Rust 都没改（`src-tauri/` 与 `remote-daemon-proto/` 一个字节未动）；
`cargo` 那格红过一趟，是**两条已有守卫判红**（`§6`），不是编译失败。

### `git status --porcelain`

**开跑前 · 工作树 `k-r82`**：**空**
**开跑前 · 代码仓主树 `cc-monitor`**：**空**
**开跑前 · 计划仓 `.claude/planned-build`**：9 行，全是 `?? backend-consolidation/.briefs/*.md`（PM 的派工单，不是本件动的）

**收工后 · 工作树 `k-r82`（提交前）**：7 行 ——
`M .github/workflows/ci.yml` · `M evidence/K-R80-gate-cell-coverage.py` ·
`?? evidence/K-R82-hooks-gate.md` · `M hooks/pre-commit`（**只有 mode**）·
`M scripts/README.md` · `M scripts/gate.sh` · `?? scripts/hooks-are-runnable.sh`
**收工后 · 工作树 `k-r82`（提交后）**：**空**
**收工后 · 代码仓主树 `cc-monitor`**：**空**（本件一个字节没落在主树上）
**收工后 · 计划仓 `.claude/planned-build`**：9 行 `?? .briefs/*.md`（与开跑前同）
＋ 件文件 `features/K-R82-….md` 的 `§8`（本件写的，写在最后一趟 `GATE: OK` **之后** —— 见 `〔R82c〕`）

### §7a 一条要单独说的：`hooks/pre-commit` 的 mode 怎么进的库

`core.filemode=false` ⇒ **`chmod` 进不了 git**。本件走的是记忆条
`filemode-false-chmod-invisible` 那条路：`git update-index --chmod=+x` **只改暂存区**，
提交时**不带 pathspec**（带了会从工作树重新 add，那一步在 `filemode=false` 下把这次 chmod 冲掉）。
⇒ 提交后 `git ls-files -s hooks/` 必须是 `100755`；`git show --summary` 里那一行是
`mode change 100644 => 100755 hooks/pre-commit`，**没有内容 diff**（`--stat` 那行是 `| 0`）。

**「库里记没记」的终局读数** —— 拿 `git archive` 问「一棵全新的 checkout 会拿到什么」
（沙箱里跑，不建真工作树）：

| 提交 | 一棵新 checkout 拿到的 `hooks/pre-commit` |
|---|---|
| `880b864`（基点） | **`-rw-rw-r--`** ⇒ git 忽略它、照常提交，`C7` 那道挡不在 |
| `419bdcc`（本件） | **`-rwxrwxr-x`** ⇒ 真的会跑 |

★ 这一行才是本件真正修好的东西：主树上那份**一直**能跑，所以这个洞在主树上**看不见**。

---

## §8 交回 PM（`〔R82x〕`）

- **`〔R82a〕`** 🔴 **`R42` 裁定四里有一句现打对不上**。它逐字写「仓根文件由 `generated` / `winchk`
  等间接覆盖到要紧的那几份」—— 现打：`generated` 的 pathspec 是 `-- src/generated/`（碰不到任何仓根文件），
  `winchk` 在 `src-tauri/` 里跑 `cargo check`（同样碰不到）。**真正间接够得到仓根文件的是 `npm` 那一格**
  （命令本体住 `package.json` 的 `scripts.test`）。本件按**现打**写进登记（见 `§5a`），
  并把这条差异交回 —— 要不要回头订正 `DECISIONS.md#R42` 那一句，PM 定。
- **`〔R82b〕`** `scripts/gate.sh` 开头那句自述射程「它跑的是**工作树**的三道门 + 一道生成物漂移检查
  + `pb check` + 四套 `ccm` e2e」**仍在腐**（它今天点不出 `fmt` / `fmt-daemon` / `winchk` / `hooks`），
  而**今天没有任何判据钉着它**（`C5` 钉的是末尾那行裁决，不是这一句）。本件**没动它**（最小面），交回 PM 裁：
  订正 / 立判据 / 还是就让它这样。
- **`〔R82c〕`** 🔴 **`§8` 与 `GATE: OK` 今天互斥**，本件复现了 `K-R80` 撞过的那条死锁：
  一动件文件，`pb check` 那格立刻 `FAIL [J3 陈账] INDEX.md 比源文件旧`（**现打验过**：
  只 `touch` 件文件、一个字节不改，`FAIL=0` ⇒ `FAIL=1`；已把 mtime 原样还回去再取的绿）。
  而派工单逐字「收工前不要跑 `pb index`」⇒ 实现方无解。
  ⇒ 本件按 `R42 裁定五` 的先例办：**读数与刀全部落在这份 `evidence/`（0 格覆盖，改它不动任何一格读数）**，
  件文件 `§8` 在**最后一趟 `GATE: OK` 之后**才写。**那趟绿量于写 `§8` 之前的树**，这里说清，不含糊。
- **`〔R82d〕`** `.github/workflows/ci.yml` 动了**一个数 + 一段注**（`57` → `58`）。
  本区先例是「`ci.yml` 不在实现方写区，逐字 diff 交回 PM」——
  但这一处**不改就出不了 `GATE: OK`**（`shell_lint_registry` 那条恒等判据当场红，见 `§6`），
  且它自己的诊断逐字给出了落地路径。⇒ 已改，**在这里点名**，请 PM 在 diff 上过一眼。

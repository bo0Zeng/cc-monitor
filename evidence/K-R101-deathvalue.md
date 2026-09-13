# K-R101 死值验原文（`C` 阶段填）

四条 dod ＋ `7u` 各自的刀、逐刀**完整原文读数**住这里；件文件 `§3` 只放摘要。

## 规矩（缺一条这份证据不算数）

- **全部在沙箱里跑**：`PB_WS=backend-consolidation .claude/devbox/gate <本工作树绝对路径> <tag>`。
  **不许直接在本机跑**（`K31` / `R15`）。
- **一趟一刀**，下一刀前还原干净，贴 `git status --porcelain` 自证。
- 🔴 **还原不许用 `cp -a`**（连 mtime 还原 ⇒ cargo 判「没变」⇒ **你读到的是上一刀的红**）。用 `cp` ＋ `touch`。
- 🔴 **刀具取不到该取的东西时必须拒跑（fail-closed）** —— `K-R87` 那轮 `cut.sh` 只认第 2 行的
  `# FILES:`，某个变异写在第 4 行 ⇒ **还原静默跳过**、`7u` 首趟叠在上一刀上。
  **量具「静默跳过」比量具报错危险得多。**
- **点名**：每刀写清**哪一条判据红了**，不是只写「红了」；阴性对照也贴读数。
- ⚙ **「变异存活」两种成因别混**：判据瞎 ⇒ 补判据 · 变异没生效 ⇒ 重切刀（`K-R87` 的 `M4`
  就是后者：真 tmux 3.4 上 `new-session -A` 在无终端 daemon 里 rc=1 ⇒ 变异落地了、语义没落地）。
- ⚙ 本树 `node_modules` 软链已铺（不铺 `M0` 必红，`tsx: not found`）；gitignore 盖着，**不是写区项**。

## 量具住址（`brief` 第 12 条：住址要能唯一定位到那一份）

| 量具 | 住址 | 被测对象指向哪棵树 |
|---|---|---|
| 切刀 ＋ 跑法（同一份） | `<wt>/evidence/K-R101-cut.py` | **本文件所在的那棵工作树**，`REPO = 本文件的祖父目录`，**不接受路径参数**（`5k`：同一住址下先后住过两份被测对象不同的量具，是一次静默的假读数） |

`<wt>` = `/home/zbl/文档/claudecode-frontend/.claude/worktrees/k-r101`。

复跑：`python3 evidence/K-R101-cut.py run <落点目录> M2 M3 … M13`。

🔴 **跑法一度是 `evidence/K-R101-run-cuts.sh`，被判据当场拦下** ——
`src-tauri/src/shell_lint_registry.rs::every_shell_script_is_either_linted_or_registered_as_exempt`：
全仓每个 shell 脚本要么进 `ci.yml` 的 shellcheck 表达式（连带一个计数地板），要么进 `EXEMPT`
并写清理由。⇒ 折进 `.py`（本仓 `evidence/` 下既有的形态），不动那两处。
⚠ 它在 `M11`/`M12`/`M13` 第一趟的 cargo 那一格里留下过噪音（那三趟每趟 **2 failed**，
其中一条是它自己），**三趟已重打**，下面表里是重打后的读数。

⚠ **逐刀跑的不是整趟 13 格门禁，是那一刀够得着的那几格**（`vitest` / `cargo` / `daemon`，
逐刀写在脚本的 `suites_for`），命令与门禁那几格**逐字相同**、**同一个沙箱镜像、同一个
target 目录**。整趟 13 格的读数由下面 `M0`（动手前）与 `M1c`（实现后、切之前）给。

## `M0` 基线 —— **动手前**，量于分支基点 `cef5cc7`（＝ `track/k-r101` 当时的尖）

```
GATE: OK —— 13 格全绿（hooks · fmt · fmt-daemon · winchk · cargo · generated · daemon · npm · 四套 ccm e2e · pb check）
  hooks 11 · fmt 1 · fmt-daemon 1 · winchk 1 · cargo 1595（9 个包合计）· generated 与 Rust 源一致
  daemon 742 · npm 1714 · e2e 12/8/46/45 · pb check FAIL=0 BROKEN=0
```

⚠ **本树未铺 `src-tauri/embedded-daemons/`** ⇒ `embedded_daemons` cfg 不置 ⇒ cargo 那个合计里
少了「本地后端真的能起来吗」那一族（4 条）。**这是那个数的分母的一部分，不是漏跑。**

⚙ 这四个数与 PM 单子里给的参照（`cef5cc7` 上 `K-R87` 收官：1595 / 742 / 1714 / 12·8·46·45）
**逐个相同** —— 但那是**现打之后**发现相同，不是抄过来的（`§3` 明写「别抄 PM 给的数」）。

## `M1c` 实现后、切之前 —— 量于本分支提交 `353b69a`

```
GATE: OK —— 13 格全绿
  hooks 11 · fmt 1 · fmt-daemon 1 · winchk 1 · cargo 1598（9 个包合计，+3）· generated 与 Rust 源一致
  daemon 742 · npm 1722（+8）· e2e 12/8/46/45 · pb check FAIL=0 BROKEN=0
```

⚠ **两个增量各自的分母**：
- **cargo +3**（1595 → 1598）= `src-tauri/src/account_usage.rs` 新增的三条：
  `the_probe_is_a_composition_of_commands_the_daemon_already_has` ·
  `the_whole_probe_needle_actually_catches_one` ·
  `the_orchestration_registry_still_describes_what_this_file_does`。
- **npm +8**（1714 → 1722）。⚠ 这个 **+8 不等于「加了 8 条判据」**：
  本件在四份 vitest 上**新增 26 个 `it(`、删掉 18 个 `it(`**（净 +8）。
  删掉的那 18 条**全部**是解析层退役后契约不存在了的（`formatUsageSummaryCompact` 那一族 ·
  `ok` / `unrecognized` / `not-logged-in` / `cli-missing` 那四态的用例）。
  逐条名单：`git diff 353b69a~1 353b69a -- src/*.vitest.ts src/**/*.vitest.ts | grep '  it('`。

## ⚠ 两趟**作废**的读数（如实登记，别让人照着复跑）

- `M1`（第一趟）`GATE: FAIL` —— fmt / cargo / generated 三格红，全是**尺子②**逮的（见件文件 `§8`）。
- `M1b` `GATE: FAIL —— generated` —— ⚠ **它还有第二个毛病**：那一趟门禁**正在跑的时候**，
  我在同一棵工作树上跑了一次切刀的冒烟（`apply M13` ＋ `restore`）。
  重叠窗口只有几秒、读数看着自洽（cargo 1598 · npm 1722），
  **但我证不了它没读到变异中的文件** ⇒ **整趟作废，重打成 `M1c`**。
  ★ 教训：**门禁在跑的时候，那棵工作树是它的**。

## 逐刀读数

> **读法**：每刀一节，写清 ① 刀口（锚点逐字住 `evidence/K-R101-cut.py` 的 `CUTS` 表，
> 命中数由刀具切之前断言，对不上就拒跑）· ② **哪一条判据红了**（点名，不是「红了」）·
> ③ 分母 · ④ **阴性对照**（哪些没红，以及为什么那正是要的）。
> 落点：`M2`–`M10` 在 `…/scratchpad/kr101/cuts2/<id>.out`；
> `M11`–`M13` 在 `…/cuts3/<id>.out`（第一趟有噪音，见上面「量具住址」那一节）。

### `M2` · `KR101D1` ① —— **展示那一层**把 `raw` 扔了

刀口：`src/account-usage.ts` 的 `usageScreenEl` 里 `pre.textContent = raw;` → `pre.textContent = "";`
（锚点命中 1/1）。

**红 6 条**（分母 vitest 全量 **1722**，1716 passed）：

| 判据 | 住址 |
|---|---|
| `★ KR101D1：展开菜单 → 那一屏**原文**落到菜单当前账号行的 DOM 上（折叠态留空）` | `src/account-chip.vitest.ts` |
| `★ KR101D1：点击后 查询中 → 那一屏**原文逐字**摆在单元格里（等宽 · 保留空白）` | `src/settings/accounts-section.vitest.ts` |
| `★ 生产路零解析：屏上写着 sign in / command not found 也不再被判成一个态` | 同上 |
| `★ KR101D1：screen → 那一屏**原文逐字**落在页面上（等宽 · 保留空白）` | `src/views/usage-plan-block.vitest.ts` |
| `★ 连读两次：只留最新那份` | 同上 |
| `★ 失败之后再成功：上一次的失败消息不许留在屏上` | 同上 |

🔴 **阴性对照 —— 这一刀正是件文件点名的那个失效方向的活体**：
`M2` 的替换**只动一句赋值**，`AccountUsageOutcome` 的 `raw` 字段与生成物
`src/generated/AccountUsageProbeResult.ts` 的 `raw` **一个字节没动**
⇒ **一条只判「`raw` 这个字段还在类型里」的判据，这一刀之下是绿的**，而上面六条红了。
⚙ 另一侧的阴性对照：`KR101D2`/`KR101D3` 那 8 条**全绿**（这一刀没碰解析层与墓碑）⇒ 不是粗刀。

### `M3` · `KR101D1` ① 的第二个刀口 —— **取数那一层**把 `raw` 换成空串

刀口：`fetchAccountUsage` 里 `? { status: "screen", raw: result.raw ?? "" }` →
`? { status: "screen", raw: "" }`（命中 1/1）。

**红 5 条**（1717 passed / 1722）：`★ KR101D1：captured:true → screen，那一屏原文一个字节不改地带出来`
· `账号 0 的探测结果照常带回原文 + 进缓存`（均住 `src/account-usage.vitest.ts`）·
`★ KR101D1：展开菜单…`（`account-chip.vitest.ts`）· `★ KR101D1：点击后 查询中…` ·
`★ 生产路零解析：屏上写着 sign in…`（均住 `accounts-section.vitest.ts`）。

⚙ **阴性对照 · 两条，都要读**：
① `usage-plan-block.vitest.ts` 的三条**没红** —— 它 mock 掉的正是 `fetchAccountUsage`
（`vi.mock("../account-usage", …)` 只覆盖那一个口），所以这一刀在它的射程外。
**那不是漏，是射程**：同一条性质由 `M2`（展示层）在那份文件上打红过。
② 类型面同 `M2`：`raw` 字段仍在 ⇒ 判形状那条仍绿。

### `M4` · `KR101D1` ③ —— **把空屏判成失败**

刀口：`outcome = result.captured` → `outcome = result.captured && (result.raw ?? "") !== ""`（命中 1/1）。

**红 4 条**（1718 passed / 1722），**四条全部是 ③ 那一族**：
`captured:true ∧ raw==='' ⇒ screen` · `captured:true ∧ raw===null（Rust 侧 Option 的 None）⇒ 也是 screen`
（`src/account-usage.vitest.ts`）· `★ KR101D1 ③：抓到空屏也算成功 —— 仍然渲染，并明说它是空屏`
（`account-chip.vitest.ts`）· `★ KR101D1 ③：…照样渲染，并明说是空屏`（`accounts-section.vitest.ts`）。

⚙ **阴性对照**：`★ 反向自检：captured:false 仍然是 probe-failed` **仍绿** ——
证明上面那四条不是「什么都算成功」换来的。
⚠ `usage-plan-block` 那条 ③ 没红，理由同 `M3` ①（它 mock 了 `fetchAccountUsage`）。

### `M5` · `KR101D2` ① —— 生产段又把那一屏喂回解析器

刀口：`src/account-usage.ts` 加回 `import { parseUsageCapture }`，并把 `raw` 换成
`parseUsageCapture(result.raw ?? "").status`（两处锚点各 1/1）。

**红 10 条**（1712 passed / 1722）。**本刀的正题那一条**：
`★ ① 生产段**一处都不许**调 parseUsageCapture / import 解析器`（`src/account-usage.vitest.ts`）。
另外 9 条是 `KR101D1` 那一族（原文被换成了 `"ok"` 这种状态串 ⇒ 原文到不了界面）。

⚙ **阴性对照**：`★ 抽取器自检：生产段人群不是空的` 与 `★ 阳性对照：这把尺子真的逮得住`
**都绿** ⇒ 红的是「真有人在调」，不是尺子坏了。

### `M6` · `KR101D2` ③ —— **把解析器整份删掉**

刀口：删除 `src/account-usage-parse.ts`（原 md5 `f15aa39802a4`）。

**红 5 条 ＋ 一整份套件加载失败**（1698 passed / **1703** —— 分母从 1722 掉到 1703，
少掉的 19 条正是 `src/account-usage-parse.vitest.ts` 自己那一套：它 import 不到被删的模块，
vite 报 `TransformPluginContext.error`）：

- `★ ③ 退役不是删除：解析器 ＋ 它的 vitest ＋ 冻结夹具，三样都得在盘上`
- `KR101D3` 那四条（`★ 一 · 谁退的它` · `★ 二 · 复活条件` · `★ 三 · 绿灯只证明` · `★ 反向自检`）
  —— 它们读盘拿墓碑，文件没了当场红。

🔴 **这一格就是「退役不是删除，两个方向都要拦」的兑现**：`M5`（接回生产路）与 `M6`（整份删掉）
**两刀都红**，而它们的方向相反。

### `M7` · `KR101D2` ③ 的最小面 —— **只删冻结夹具**

刀口：删除 `src/__fixtures__/usage-capture-2026-07-31.txt`。

**红 1 条**：`★ ③ 退役不是删除：解析器 ＋ 它的 vitest ＋ 冻结夹具，三样都得在盘上`
（1702 passed / **1703**；同样是 `account-usage-parse.vitest.ts` 整份加载失败 ⇒ 分母 1703）。

⚙ **这一刀是「最小面」的证据**（`brief` 第 9 条）：只动三样里的**一样**，
`KR101D3` 那四条**全绿**、`KR101D1` 那九条**全绿** ⇒ 上面 `M6` 的 5 条红不是粗刀铺开的。

### `M8` / `M9` / `M10` · `KR101D3` 三样，**一刀一样**

| 刀 | 抹掉哪一样 | 红的那一条（唯一） | 分母 |
|---|---|---|---|
| `M8` | 一 · 谁退的它（`R59` 逐字 ＋ 日期） | `★ 一 · 谁退的它：R59〔用 09-13〕逐字那句 ＋ 日期` | 1721/1722 |
| `M9` | 二 · 复活条件（四样换成一句空话） | `★ 二 · 复活条件写的是「什么成立了才接回去」，不是「以后可能要用」` | 1721/1722 |
| `M10` | 🔴 三 · 那句会救人的话 | `★ 三（本条的重心）· 绿灯只证明「对那份冻结夹具有效」，不证明「对真机 /usage 有效」` | 1721/1722 |

🔴 **三刀各红一条、互不牵连** —— 这正是 `KR101D3`「三样缺任一样 ⇒ 红」要的形状：
不是一条判据笼统看「墓碑在不在」，是三条各自钉一样。
⚙ **阴性对照**：三刀里 `★ 反向自检：这三条不是靠「文件很长」蒙过去的` **都绿**
（它断的是那三句话**只在墓碑段里出现**，抹掉其中一句不影响这一点）。

### `M11` · `KR101D4` ① —— daemon 上长出一条「整条探针」

刀口：`remote-daemon-proto/src/main.rs` 的 `SUBCOMMANDS` 加一行 `"--usage-probe"`（命中 1/1）。

**cargo 格红 1 条**（1465 passed / 1 failed）：
`account_usage::tests::the_probe_is_a_composition_of_commands_the_daemon_already_has`
—— 正是 `§0c` 点名的失效方向。

**daemon 格另红 2 条**（740 passed / 2 failed），**都是 PM 单子里点名的计数型判据**：
`build_id_guard::tests::adding_a_subcommand_forces_a_build_id_bump` ·
`protocol_doc_guard::tests::every_dispatched_subcommand_appears_in_the_protocol_doc`。
⚙ 这两条**不是我加的** —— 它们的存在本身就说明「新开一条子命令」这个动作在 daemon 那侧
已经有三处在数；本件加的那一条补的是**「这条命令是不是整条探针」**这一维（前两条只管
「有没有 bump / 有没有写进协议文档」，一条叫 `--usage-probe` 的大动作**照样能过它们**）。

⚠ **第一趟这一刀的 cargo 格是 2 failed**，多出来的那条是
`shell_lint_registry::tests::every_shell_script_is_either_linted_or_registered_as_exempt`
—— 我当时把跑法写成了 `evidence/K-R101-run-cuts.sh`，一个**未登记的 shell 脚本**。
**那是量具自己的噪音，不是这一刀的读数**；跑法折进 `.py` 之后重打，才是上面这个 1 failed。

### `M12` · `KR101D4` ② —— 编排登记表**塌成一条**

刀口：`PROBE_ORCHESTRATION_STEPS` 里「挂自毁看门狗」那一行的 owner
`"--oneshot-session"` → `"--launch"`（命中 1/1）⇒ 互不相同的 owner 从 4 掉到 3。

**cargo 格红 1 条**：同上那一条（走的是它的 `owners.len() >= 4` 那一格）。

⚙ **阴性对照**：`the_whole_probe_needle_actually_catches_one` 与
`the_orchestration_registry_still_describes_what_this_file_does` **都绿**
⇒ 红的是「塌成一条」，不是「表被动过」。

### `M13` · `7u` —— 把实现整个掏空，判据一条不动

刀口 7 处（`src/account-usage.ts` 3 · `src/account-usage-parse.ts` 3 · `src-tauri/src/account_usage.rs` 1），
逐条锚点住 `CUTS` 表。

**vitest 红 14 条**（1708 passed / 1722）· **cargo 红 1 条**（1465 passed / 1 failed）⇒ **共 15 条**。

逐条与「仍绿的为什么仍绿」写在件文件 `§3-5`。


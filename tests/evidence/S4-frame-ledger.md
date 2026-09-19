# 秤 4 读数：每帧账本

对应 `调研/设计/17-算法与复杂度.md` §6 表第 **4** 行，验的是同文档 **§2.7**
（`computeMainBranch` 每帧对整个会话重扫）。

- 仪表：`src/branch-fold.ts` 末尾 `=== 秤 4 仪表 ===` 那一段（账本挂 `window.__ccmPerf.branchLedger`）
- 判据：`tests/scale4-frame-ledger.vitest.ts`（7 格，全绿）
- 死值验：`tests/evidence/S4-deathvalue.sh`（四刀，原文见 §6）
- 跑的日期：2026-09-18
- 机器：aya（Linux 7.0.0-30-generic x86_64），node v22.22.1（**无指针压缩**：
  `v8_enable_pointer_compression=0`），vitest 4.1.10，environment=jsdom

---

## 0. 一句话结论：**「97% 走快路」这句话成立，但它没在回答任何人关心的问题**

| 问 | 实测 |
|---|---|
| §2.7 说的那个 97%，在它自己引的分叉密度下是多少？ | **97.00%**（n=600、17 个分叉 ≈ 3% parent 分叉率、每条一帧） |
| 那这句话是被验证了吗？ | **没有。** 97% 是从「3% parent 成 fork」这句**算**出来的；读数复现的是那个算术，不是那个 3% |
| 生产里 live 是每条一帧吗？ | **不是。** F15 的帧末合批已经在里面了 ⇒ 4 条/帧 **92.17%** · 8 条/帧 **85.50%** · 16 条/帧 **72.17%** |
| 启动重放那条路呢？ | **0.00%** —— 而且那条路上 O(N) 本来就只跑一次，档 1 在那里一点用都没有 |
| 快路**算得对**吗？ | 全命中的 65 次里，影子算出来的主线与真算**逐元素相等 65 次、不等 0 次** |
| `rebuild()` 搬了几个节点？ | 稳态 **24 个/次**（= 2 × off-main 卡数），**与 N 无关**；但每次仍要**顺序扫 N 个顶层子节点** |

🔴 **最该改设计文档的一条**：§2.7 的「97% 的**记录**可走 O(1)」与 `branching.ts` 头注的
「~3% **parent** 形成 fork」**换了分母**。n 条记录、F 个分叉时，有 child 的 parent 是 n-1-F 个，
所以 `parent 分叉率 = F/(n-1-F)`，而 `记录未命中率 = (F+1)/n`。3% 的 parent 分叉率
对应的**记录**分叉率是 2.83%（n=600 实测），再加上 root 那一条 ⇒ 命中率 97.00%。
两个数对得上是**巧合级的接近**，不是同一个量。

---

## 1. 复算命令（逐字）

```
cd /home/zbl/文档/claudecode-frontend/cc-monitor && npx vitest run --reporter=verbose tests/scale4-frame-ledger.vitest.ts
```

⚠ 与秤 5 同一个复算陷阱：vitest 4 的默认 reporter 在**非 TTY**（管道/重定向）下会吞掉
**通过**的测试的 `console.log`。要在管道里拿到下面的表，必须加 `--reporter=verbose`。

死值验：

```
cd /home/zbl/文档/claudecode-frontend/cc-monitor && bash tests/evidence/S4-deathvalue.sh
```

---

## 2. 装表法

### 2.1 照着 §6 表装的

§6 表逐字：「`branch-fold.ts:117` 前后夹计时，累加进 `__ccmPerf`」。
**这一条照做了**，但**住址改成符号**（`BranchFolder.computeMain()`）——
`:117` 今天落在 `scheduleLiveRecompute` 的 `const next = this.computeMain();` 上，
本轮一改就不是那一行了。§6 自己的订正段逐字写着「**能点符号就别写行号**」。

三处夹子：

| 夹在哪 | 记什么 |
|---|---|
| `BranchFolder.computeMain()` | 每次 `(N, ms, via, frame)`；`via` 分 `live-frame`/`flush`/`set-records`/`rebuild-now`/`queued-content` |
| `BranchFolder.rebuild()` | 每次 `(扫了几个顶层节点, 解开几个 wrap / 搬回几个节点, 新折几个 wrap / 搬进几个节点)` |
| `BranchFolder.recordAdded()` | §2.7 档 1 的**影子命中判定**（见 2.2） |

### 2.2 「97% 走快路」怎么可能量得到 —— 快路**并没有装**

§2.7 档 1「脏标记快路」今天**不在代码里**，每次仍然全量 `computeMainBranch`。
⇒ 量的是 `noteFastPathShadow()` 里的**影子判定**：「如果装了档 1，这条记录会不会走 O(1)」。
判定只读态、只写账本，**一个字节都不回流到折叠结果**（这一点由 §5 那一组回归判据钉着）。

档 1 的谓词逐字是「新记录的 parent 是当前主线叶子且原本无 child ⇒ 只 `add(uuid)`」，
拆成四问（顺序即优先级，一条记录只记最先踩到的那个原因）：

| # | 问 | 未命中原因名 |
|---|---|---|
| 1 | 没有 `parentUuid` ⇒ 它是 root | `noParent` |
| 2 | `parentUuid` 不在已见集合里 ⇒ 链断，会被当 root | `parentUnknown` |
| 3 | parent **已经有 child** ⇒ **这就是 fork 点**，赢家要重选 | `parentHasChild` |
| 4 | parent 不在主线集合里 ⇒ 接在旧分支上 | `parentOffMain` |

⚠ 第 4 问里的「主线集合」是 `lastMainBranch ∪ 影子快路自己 add 的那些`。
档 1 的快路是「只 `add(uuid)`」，所以同一帧里后来的记录**能看见**前面那几条 ——
不这么建模就量成了「帧合批把快路废掉了多少」而不是谓词本身。
**两个数都要**，所以判据分两种到达节奏各量一次（表 A / 表 C）。

### 2.3 `verify` 档：快路**算得对不对**

`window.__ccmPerf.branchLedgerVerify = true` 时，在「这一段全命中」的那些次真算上，
把影子算出来的主线（`lastMainBranch ∪ 影子 add`）与真算结果**逐元素比一次**。
**生产默认关**（它是 O(N)，不该白付）。判据开着它。
「快而不对」比慢坏得多 —— 这一格专门盯那件事。

---

## 3. 语料：🔴 **结构是构造的**

用 `tests/__fixtures__/scale2-height-records.jsonl`（69 条，秤 2 产出，
**结构采自真机、正文全部合成**）当**内容**。

但那 69 条**没有拓扑可用** —— 现打：

```
n= 69  types { assistant: 46, user: 23 }
roots 68   forkParents 0   parentsWithChild 1
```

**68 个 root、0 个 fork parent**：它的 `parentUuid` 全指向集合外（采样时按内容采的，没留链）。
⇒ 分叉这一层由判据里的 `rewire()` **构造**：uuid / parentUuid / timestamp 全是造的。

🔴 **所以命中率这一档是构造的，带着「构造体像不像真的」这个前提。**
它像不像真的，取决于**一件事**：真机上 ESC 回退的分布是不是
「一条主链 + 均匀撒的分叉点、每个分叉甩掉固定长度的旧分支」。**这一条没有读数**
——`设计/17 §6` 数据源纪律 2026-09-18 改判（用户逐字「这是测试啊 / 不应该进」），
**不许再去读 `~/.claude/projects`**。所以本读数把「分叉密度」做成旋钮扫了一遍（表 B），
让读的人自己看那个假设值多少钱。

内容这一侧是老实的：记录走**生产的抽取器** `extractBranchRecord()` 进来，
不是手搭 `BranchRecord`。

---

## 4. 实际 stdout（逐字，未编辑数字）

```
=== 秤 4 · A 纯谓词命中率（每条一帧；语料 = fixture 69 条内容 + 构造拓扑）===
记录数 N=69，分叉点 F=3，每个分叉甩掉 4 条
分母①「分叉记录 / 全部记录」= 3/69 = 4.35%
分母②「fork parent / 有 child 的 parent」= 3/65 = 4.62%
  ↑ branching.ts 头注那个「~3% parent 成 fork」用的是分母②；
    §2.7 写的「97% 的**记录**可走 O(1)」用的是分母①。两者不是同一个数。
**快路命中率 = 65/69 = 94.20%**
未命中分布：{"noParent":1,"parentUnknown":0,"parentHasChild":3,"parentOffMain":0}
帧级：能整帧跳掉 O(N) 的次数 65/69 = 94.20%
快路正确性（verify 档）：相等 65 次 / 不等 0 次
rebuild：69 次，累计搬动 684 个节点
最终 DOM：3 个 fold-wrap

=== 秤 4 · B 分叉密度 → 命中率（每条一帧，n=600，旧分支长 2）===
旋钮% | 分叉数 | 实得parent% | 分叉/记录 | 快路命中率 | 可跳帧占比
     0 |      0 |     0.00% |     0.00% |    99.83% |      99.83%
     1 |      6 |     1.01% |     1.00% |    98.83% |      98.83%
     3 |     17 |     2.92% |     2.83% |    97.00% |      97.00%
    10 |     54 |     9.91% |     9.00% |    90.83% |      90.83%
    20 |    100 |    20.04% |    16.67% |    83.17% |      83.17%
⚠ 「旋钮%」是按 F/(n-1-F) 反解的目标 parent 分叉率；「实得parent%」是取整后的真值。

=== 秤 4 · C live 合批吃掉多少（n=600，17 个分叉 ≈ 3% parent 分叉率）===
条/帧 |  帧数 | 快路命中率 | 分叉miss | 合批miss | 可跳帧占比
    1 |   600 |    97.00% |      17 |         0 |     97.00%
    2 |   300 |    95.50% |      17 |         9 |     94.00%
    4 |   150 |    92.17% |      17 |        29 |     88.00%
    8 |    75 |    85.50% |      17 |        69 |     76.00%
   16 |    38 |    72.17% |      17 |       149 |     52.63%
「分叉miss」= 真正的 fork 点（档 1 无论如何都跳不掉的那些）
「合批miss」= 纯粹因为主线集合一帧才刷一次而被连累的记录（`parentOffMain`）
⚠ 帧合批（F15）已经在生产里了 ⇒ 每帧的 O(N) 本来就只有一次。
   ⇒ 档 1 真正能省的是「可跳帧占比」那一列，不是「命中率」那一列。

=== 秤 4 · D 重放（batch）===
69 条整批灌入 ⇒ computeMainBranch 调 1 次（N=69）
快路命中率 0.00% —— 档 1 对启动重放**一点用都没有**，
因为那条路上 O(N) 本来就只跑一次（F15 的 batch 模式已经把它合掉了）。

=== 秤 4 · E rebuild 搬动节点数（每条一帧，69 条 / 3 个分叉）===
rebuild 69 次，累计搬动 684 个节点（均 9.91/次，max 24）
末次：扫 69 个顶层节点，解开 3 个 wrap（12 张卡），重折 3 个 wrap（12 张卡）
⚠ 「搬动数」只数 move 的次数，不含浏览器为此付的布局/重绘 —— 那个 jsdom 里量不到。

=== 秤 4 · F computeMainBranch 的 N → ms（node + jsdom，5 次）===
     N | 分叉 |   中位ms |    min |    max
   181 |     5 |    0.068 |    0.060 |    0.098
  1575 |    46 |    0.628 |    0.598 |    0.643
  4566 |   133 |    2.556 |    2.417 |    3.560
对照 §2.7 现打（同为 node）：N=181 → 0.24ms · N=1575 → 1.33ms · N=4566 → 3.45ms
🔴 这一列**不是 WebView2 的读数**。仪表在生产代码里，真机跑一次就有同一列数；本轮没有真机。

 ✓ ★ 反空真：快路与慢路都真的被走到了（两边都用相等断言钉死） 96ms
 ✓ ★ 每条一帧（纯谓词）：97% 那句话的实测命中率 + 快路算得对不对 53ms
 ✓ ★ 分叉密度旋钮：命中率随密度怎么走（3% 那一档是对照 §2.7 的） 499ms
 ✓ ★ live 合批（k 条一帧）：脏标记快路被帧合批吃掉多少 180ms
 ✓ ★ 重放（batch 模式）：快路命中率 0%，而那里本来就只算一次 3ms
 ✓ ★ rebuild 搬了几个节点（DOM 侧，相等断言钉最后一次） 26ms
 ✓ computeMainBranch 的 N → ms（node/jsdom 读数，**一条时间断言都没有**） 30ms

 Test Files  1 passed (1)
      Tests  7 passed (7)
```

---

## 5. 反空真那一格

命中率这个数有个要命的性质：**只走到一条路时它照样算得出来，而判据会照样绿。**
（全命中 ⇒ 100%；全不命中 ⇒ 0%。两个都是「一个数」。）

所以 `★ 反空真` 那一格专门钉两条路都真的被走到了，**四层全是相等断言**：

| 层 | 断言 | 它挡什么 |
|---|---|---|
| ① 语料 | `N - computeMainBranch(recs).size === F × BACK`（= 12） | 构造的分叉**被生产算法自己认了**。退化成一条直链时这里先红 —— 那样慢路一次都不会走到 |
| ② 条数 | `fastPathHit === 65` **且** `fastPathMiss === 4` | 任意一边塌成 0 立刻红 |
| ③ 并列 | `[hit > 0, miss > 0]` 深等于 `[true, true]` | 把「两条路都有」这件事本身写成判据，而不是留给读的人去看数 |
| ④ 原因 | `fastPathMissBy` 深等于 `{noParent:1, parentUnknown:0, parentHasChild:3, parentOffMain:0}` | 那 4 条未命中**确实是 fork 那一支**，不是被别的原因顺手挡下来的 |
| ⑤ DOM | `:scope > .branch-fold-wrap` 有 3 个、里面 12 张卡 | 慢路真的**干了活**（真折出了折叠段），不是只把计数器 +1 |

另外，账本取不到时（`window.__ccmPerf.branchLedger` 不存在）判据**直接抛**，
不会退化成一串 0 然后静默绿 —— 那正是「仪表没装上」最容易变成安慰剂的那一形。

---

## 6. 死值验（四刀，原文）

每刀只改 `src/branch-fold.ts` 里 `noteFastPathShadow` 的**一个字符串**，跑完立刻还原。

```
### 刀⓪ 未变异：判据必须绿（否则下面每一刀都说明不了任何事）
 Test Files  1 passed (1)
      Tests  7 passed (7)

==================================================================
### 刀① fork 判反（原本无 child 这一问反过来）
改：else if (this.ledgerParentSeen.has(p)) miss = "parentHasChild";
为：else if (!this.ledgerParentSeen.has(p)) miss = "parentHasChild";
==================================================================
AssertionError: 快路命中数: expected 3 to be 65 // Object.is equality
AssertionError: expected 3 to be 65 // Object.is equality
AssertionError: rate=0：未命中该正好是「1 个 root + F 个分叉」: expected 600 to be 1 // Object.is equality
AssertionError: k=1：合批下的未命中数与模型不符: expected 583 to be 18 // Object.is equality
AssertionError: expected { Object (noParent, parentUnknown, ...) } to deeply equal { Object (noParent, parentUnknown, ...) }
 Test Files  1 failed (1)
      Tests  5 failed | 2 passed (7)
--- 退出码 1（非 0 = 当场红，这一刀过）

==================================================================
### 刀② fork 当命中（分叉点不再判未命中）
改：else if (this.ledgerParentSeen.has(p)) miss = "parentHasChild";
为：else if (this.ledgerParentSeen.has(p)) miss = null;
==================================================================
AssertionError: 快路命中数: expected 68 to be 65 // Object.is equality
AssertionError: expected 68 to be 65 // Object.is equality
AssertionError: rate=0.01：未命中该正好是「1 个 root + F 个分叉」: expected 1 to be 7 // Object.is equality
AssertionError: k=1：合批下的未命中数与模型不符: expected 1 to be 18 // Object.is equality
AssertionError: 重放期主线集合始终是空的 ⇒ 一条都命中不了: expected 49 to be +0 // Object.is equality
 Test Files  1 failed (1)
      Tests  5 failed | 2 passed (7)
--- 退出码 1（非 0 = 当场红，这一刀过）

==================================================================
### 刀③ 快路恒假（命中率变 0 —— 空真那一形）
改：    else miss = null;
为：    else miss = "parentOffMain";
==================================================================
AssertionError: 快路命中数: expected +0 to be 65 // Object.is equality
AssertionError: expected +0 to be 65 // Object.is equality
AssertionError: rate=0：未命中该正好是「1 个 root + F 个分叉」: expected 600 to be 1 // Object.is equality
AssertionError: k=1：合批下的未命中数与模型不符: expected 600 to be 18 // Object.is equality
 Test Files  1 failed (1)
      Tests  4 failed | 3 passed (7)
--- 退出码 1（非 0 = 当场红，这一刀过）

==================================================================
### 刀④ verify 判反（影子算得对不对 那一格）
改：if (setsEqual(predicted, next)) led.fastPathVerified++;
为：if (!setsEqual(predicted, next)) led.fastPathVerified++;
==================================================================
AssertionError: expected +0 to be 65 // Object.is equality
 Test Files  1 failed (1)
      Tests  1 failed | 6 passed (7)
--- 退出码 1（非 0 = 当场红，这一刀过）

=== 四刀全部当场红，秤 4 的判据有牙 ===
```

⚠ 刀③ 是**空真那一形**：命中率变成 0%，「命中率」这个数照样算得出来。
它红了，说明 §5 那一格真的在挡这件事。
⚠ 刀④ 只红了 1 格（`verify` 那一格）——这是**对的**：verify 是一个独立的量，
它坏了不该把命中率那几格一起带红，否则出问题时指不出是哪一件事坏了。

---

## 7. 三个结构性发现

### (1) 🔴 档 1 的收益被**已经做完的那个优化**吃掉了一大半

F15 的帧末合批（`scheduleLiveRecompute`）已经在生产里：**live 模式下一帧只算一次 O(N)**，
判据里用相等断言复核过（`computes === 帧数`，5 个 k 档全对）。

⇒ 档 1 要再省的不是「97% 的记录」，是「**没有分叉的那些帧**」。而这两个数差很多：

| 到达节奏 | 快路命中率 | **可跳帧占比**（真正省下的 O(N)） |
|---:|---:|---:|
| 1 条/帧 | 97.00% | 97.00% |
| 2 条/帧 | 95.50% | 94.00% |
| 4 条/帧 | 92.17% | 88.00% |
| 8 条/帧 | 85.50% | **76.00%** |
| 16 条/帧 | 72.17% | **52.63%** |

而且掉下来的那部分**不是分叉造成的**：`分叉miss` 恒为 17（就是那 17 个 fork 点），
多出来的全是 `parentOffMain` —— **纯粹因为主线集合一帧才刷一次**，
分叉那条记录自己没进影子主线，于是它后面那几条的 parent 就「不在主线里」，一路传染到帧末。

> ⚠ 这一列的前提是「一帧到 k 条」。**真机上 live 期一帧到几条，这份读数没有**
> （那要真机 + 真 watcher，见 §8）。k 是旋钮，不是读数。

### (2) 🔴 启动重放那条路上，档 1 命中率 **0%**，而且本来就不需要它

batch 模式（`setBatchMode(true)` → 灌 N 条 → `flushPending()`）下：
`computeMainBranch` 调用 **1 次**，快路命中 **0 条**（主线集合全程是空的，
第 1 条之后每条都踩 `parentOffMain`）。

⇒ **§2.7 的档 1 是一个纯 live 优化。** 它和「启动后明显第二次卡顿」那条用户报告
（`events.ts` 注释里记的那条）不是一件事 —— 那条已经被 F15 的 batch 模式处置了。

### (3) `rebuild()` 的 O(N) 不在「搬节点」上，在**扫**上

末次 rebuild 的账本逐字：`扫 69 个顶层节点，解开 3 个 wrap（12 张卡），重折 3 个 wrap（12 张卡）`。

- **搬动数 = 2 × off-main 卡数 = 24**，**与 N 无关**（稳态；69 次 rebuild 累计 684 个、均 9.91、max 24）
- 而 `scanned = N`，另加一次 `querySelectorAll(':scope > .branch-fold-wrap')`

⇒ §6 表里「`rebuild()` 搬了几个节点」这个问法**指错了地方**：要砍的是那趟 O(N) 扫，
不是 move 的次数。（move 在真机上单次更贵 —— 但它的**条数**不随会话长度长。）

---

## 8. 这份读数**答了什么** / **没答什么**

### 答了

1. **「97% 走快路」的实测命中率：在 §2.7 自己引的分叉密度（~3% parent 分叉）下是 97.00%**
   （n=600、17 个分叉、每条一帧）。**声称成立。**
2. 但它成立的方式是**算术复现**，不是独立验证：97% 本来就是从「3% parent 成 fork」推的。
   **那个 3% 这份读数没有复核**（纪律禁止再读真机语料），而命中率对它很敏感（表 B：
   10% 分叉率 → 90.83%；20% → 83.17%）。
3. **两个分母不是一回事**（§0 那条），设计文档里该写清楚。
4. **合批一开，97% 就不成立了**：8 条/帧 85.50%、16 条/帧 72.17%；重放 0%。
5. **快路算得对**：65 次全命中里影子主线与真算逐元素相等 65 次、不等 0 次（`verify` 档）。
6. **`rebuild()` 搬的节点数与 N 无关**（稳态 24），O(N) 在扫那一趟。
7. **一帧只算一次**（F15）在 5 个 k 档上都由相等断言复核过：`computes === ceil(n/k) === frames`。

### 没答（**逐条明说，不推断**）

1. 🔴 **WebView2 上是多少 —— 一个字都没有。** §6 表第 4 行要验的第一件事
   （「3.45 ms 是 node 上的读数，WebView2 上可能不同」）**本轮没答**。
   能答的只是：**仪表已经在生产代码里**，真机跑一次 `window.__ccmPerf.branchLedger`
   就有同一列数，不用再改一行 `src/`。
2. 🔴 **本机 node 的读数与 §2.7 那三个数对不上，差 1.35 ~ 3.5 倍**：

   | N | §2.7 现打 | 本次（node v22 / jsdom，5 次中位） | 比 |
   |---:|---:|---:|---:|
   | 181 | 0.24 ms | 0.068 ms | 3.5× |
   | 1575 | 1.33 ms | 0.628 ms | 2.1× |
   | 4566 | 3.45 ms | 2.556 ms | 1.35× |

   **量级和线性都对上了**（本次 181→4566 是 25.2 倍输入 / 37.6 倍耗时，略超线性），
   绝对值对不上。**原因判不了**，因为 §2.7 那组数**既没记机器也没记语料** ——
   它逐字只写「现打，直接 import 生产模块」。本次的语料是构造的单链 + 均匀分叉，
   真机 jsonl 的链形（attachment/system 夹层、多 root、/compact 边界）都没有。
   ⇒ **给下一个人**：这类读数要连**机器 + 语料的住址**一起写，否则下一轮没法复算。
3. **真机 live 期一帧到几条记录 —— 没有。** 表 C 那一列的 k 是旋钮。
   要把它变成读数，得有真机 watcher 的到达节奏；这份读数没有。
4. **真机的分叉密度 —— 没有。** `branching.ts` 头注那个「1297 条 ~3%」是**一次**样本，
   本轮纪律禁止再采（`设计/17 §6` 数据源纪律 2026-09-18 改判）。
   ⇒ 表 B 的存在就是为了让这个未知数可见。
5. **构造的分叉形状像不像真的 —— 没验。** 本读数的分叉一律是
   「均匀间隔、每个甩掉固定长度的旧分支、新分支永远赢」。真机上 ESC 回退可能扎堆、
   可能甩掉很长的一段、可能同一个 parent 分三次。**这三种都会把命中率往下拉，一种都没量。**
6. **DOM 搬动的真实代价 —— 没量。** 账本只数 move 的次数；布局/重绘 jsdom 里不存在。
7. **帧号是近似的。** `frames` 是**本模块自己观察到的 rAF tick 数**：同一真实帧里两个
   `BranchFolder` 各排一次 rAF 会被记成两帧；同步路径（`flushPending` / `rebuildNow` /
   `setRecordsAndRebuild` / `addQueuedContent`）落在哪一真实帧判不了。
   ⇒ 「一帧调几次」这一列**只在 live 合批那条路上可信**。
8. **`computeMs` 夹的是 `computeMainBranch` + `exemptQueuedLeaves` 两段**，§2.7 只说前者。
   `queuedContents` 为空时后者是一句 `return`（本读数全程为空）；不空时本账本偏大。
9. **§2.7 那个「一个没合批的兄弟」（`addQueuedContent` 非 batch 下同步全量算）没动。**
   账本给它留了 `via: "queued-content"` 这一列，**本轮没有走到那条路的读数**。

---

## 9. 要把 §2.7 档 1 拍板成「做 / 不做」，还差什么

差的**不是**再测一遍命中率，是两个数：

1. **真机 live 期一帧到几条记录**（决定表 C 落在哪一行）
2. **真机 WebView2 上 N=4566 的 ms**（决定「省下的那一次 O(N)」值不值一个脏标记的复杂度）

这两个数**都不用改一行 `src/`**：仪表已经装好了，真机跑一次读 `window.__ccmPerf.branchLedger`
里的 `frames` / `recordsAdded` / `computeSamples` 就有。

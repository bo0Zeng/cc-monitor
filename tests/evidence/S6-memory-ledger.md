# 秤 6「内存」读数（`设计/17 §6` 表第 6 行）

- **判据**：`tests/scale6-memory-ledger.vitest.ts`（13 格，全绿）
- **死值验**：`bash tests/evidence/S6-mutations.sh`（8 刀，**8 刀全红**）
- **装表处**：`src/cards/index.ts`（`resultTextLedger`，纯计数）· `src/tabs.ts`
  （`branchRecordCount` ＋ `debugSnapshot` 里三个账本的条数）
- **语料**：`tests/__fixtures__/scale2-height-records.jsonl`（69 条，**结构采自真机、正文全部合成**）。
  本轮**没有**读 `~/.claude/projects`，**没有**新造含真实会话正文的夹具
  （`设计/17 §6` 数据源纪律 2026-09-18 改判，用户逐字「这是测试啊 / 不应该进」）。
- **日期**：2026-09-18。机器：aya（Linux，node 22 / vitest 4.1.10 / jsdom）。

---

## 0. 🔴 判词：「文本前端零处留存」这句话**今天不成立**

被核对的原话在 `src/tabs.ts` 的 `Tab.userInputs` 字段头注里（`KR45D2` 那一段），逐字：

> 实时这条路**一条记录的文本在前端零处留存**：持有整条 payload 的只有
> `window: TailWindow`（只收**没渲染**的那些，`takeTail` 一取就 `splice` 出账）
> 与 `midBatchBuffer`（每批 flush 后置空）；已渲染那侧的 `RecordTimeline` 条目是
> `{seq, element, kind, toolGroup}`，**没有 `message`、没有 `uuid`**。

**读数：不成立。** 逐处点名如下，前两处有判据钉住，后两处是读代码点名（本轮**没有**加判据，写在这里别当成量过）。

### 有判据钉住的两处

| # | 住址 | 留的是什么 | 钉它的用例 |
|---|---|---|---|
| ① | `src/cards/index.ts` `buildResultBody` 的闭包（`renderMode` / 两个 click 监听 / `onToggle` 都捕获 `text`，而这些监听器挂在卡片的 DOM 上 ⇒ 与卡同寿） | **tool_result 正文全文** | 「展开前整个 DOM 里找不到正文；不喂第二遍、只展开一下 ⇒ 正文一字不差地回来了」 |
| ② | `src/cards/index.ts` `RenderContext.pendingToolResults`（fallback 那一支 `ctx.pendingToolResults.set(id, { block, element })`） | **整条 `block`，含 `content` 原文**；配不上的（tool_use 永远不会来）一直留在表里 | 「第二处：tool_use 没来过 ⇒ 整条 block（含正文）留在 `ctx.pendingToolResults` 里」 |

**① 是怎么在没有 heap snapshot 的情况下定论的**——`设计/17 §2.8` 逐字写着「这条要 heap snapshot 才能定论」。
本轮走的是另一条不需要 retainer 图的路，形状是**活体**而不是快照：

1. 渲染一条 tool_result，正文里深处埋一个记号 `DEEP-MARK-6f3a91`（刻意不在首行 —— 首行会被
   `firstLinePreview(text, 60)` 放进 summary，埋首行的话下一步就是假的）；
2. 此刻断言 **整个 `document.body.textContent` 里搜不到这个记号**，且 `.block-body-result` 不存在
   （懒建：`details` 没展开就不建 body）；
3. 然后**只做一件事**：把 `details.open` 置真、派一个 `toggle`。**没有任何人重新喂过文本**；
4. 正文**一字不差**地出现在 DOM 上（`pre.textContent === LONG_TEXT`）。

⇒ 第 2 步到第 4 步之间，这段文本**不在 DOM 里**、**不在任何我们交出去的引用里**，却能再次出现。
它只能是被前端某处留着的 —— 那处就是 `buildResultBody` 的闭包。**这条推理不依赖 GC 行为，也不依赖 V8 实现。**

### 读代码点名、**本轮没加判据**的两处

- ③ `src/cards/index.ts` fallback 分支里 `makeCollapsible(cls, summaryText, () => { … buildResultBody(container, text, toolName) … })`
  的**工厂闭包**：`text` 在参数里就被捕获了，**工厂跑没跑都捕获**。
  本轮只间接量到它（下面 §1 的「`captured` 此刻是 0，但文本已经被留住了」就是它的影子），**没有单独的一格**。
- ④ 同文件 `buildResultBody` 里的 `textBodyEl` / `mdBodyEl` 两个缓存变量：
  文本↔Markdown 切模式时 `bodyHost.replaceChildren()` 只是把旧的那棵**从 DOM 上摘下来**，
  闭包里那个引用还在 ⇒ **游离 DOM 子树被长期持有**（注释里自陈是「少一次重建」的取舍）。
  这一处连影子读数都没有，纯代码读。

### ⚠ 判词的射程：**别把它读成「前端到处留存」**

判据里专门留了一格**对照组**（「普通 user 文本卡渲完，`ctx` 四张表里一个字都没留」）：
同一份正文走 `user` 纯文本那条路，渲完之后 `pendingToolResults` / `toolUseNames` /
`toolUseElements` 全空，`resultTextLedger.produced === 0`，正文**只在 DOM 上**。
少了这一格，上面的判词就会被读成一句更大的假话。

⇒ **精确的说法**：
- 那句话**按字面**（「一条记录的文本在前端零处留存」）**不成立** —— tool_result 那一支破了它。
- 那句话**被用来论证的那件事**（「已上屏的用户输入没有一个可按 uuid 查询的文本账本，所以
  `Tab.userInputs` 非新建不可」）**不受影响**：破口在 tool_result，而 tool_result 按口径本来就
  不进用户输入清单。⇒ **`KR45D2` 那个决定不需要翻案**，要改的是那句话的**措辞**。

---

## 1. 甲：`buildResultBody` 闭包持有的文本总量

装表处（`设计/17 §5.5` 点名「在 `cards/index.ts:570` 累加 `text.length`」）：

- **出口侧** `produced` / `producedUnits` / `maxUnits` —— 记在 `injectOrBuildToolResult` 里
  `renderResultContent` 的返回处（设计点名的那一处）。
- **闭包侧** `captured` / `capturedUnits` —— 记在 `buildResultBody` 的入口。

两侧分开记不是啰嗦：**它们岔开的那一支正是「留没留存」的判据**（见上面 ③）。

### 69 条语料上的读数（现打）

```
[S6-甲] 语料=tests/__fixtures__/scale2-height-records.jsonl（69 条，结构真/正文合成）
[S6-甲] tool_result 块 12 条；配得上的 tool_use 0 个 ⇒ 全走 fallback
[S6-甲] 闭包持有文本 49549 UTF-16 码元 ≈ 96.8 KiB（V8 双字节口径）
[S6-甲] 单条最长 18577 码元；均值 4129 码元
[S6-甲] 整份夹具 69 条 / 451573 UTF-8 字节
```

| 量 | 读数 |
|---|---|
| tool_result 块数（计数器 / 独立数夹具，两条路） | **12 / 12**（相等断言） |
| 闭包持有文本合计 | **49 549 UTF-16 码元** ≈ **96.8 KiB**（V8 非 Latin-1 双字节口径） |
| 单条最长 | **18 577 码元** |
| 单条均值 | **4 129 码元** |
| 占整份语料的比例 | 49 549 码元 vs 451 573 UTF-8 字节 ⇒ 约 **11%**（码元/字节，单位不同，只看量级） |

### 🔴 这份语料上 12 条 tool_result 的 `tool_use` **一条都不在**

现打：语料里 10 个 `tool_use.id`、12 个 `tool_result.tool_use_id`，**交集 0**。
成因是夹具按字节分位**挑单条**拼出来的，不是一段连续会话。
⇒ **语料这一侧量到的全是 fallback 那条路**（生产主路是内联注入）。
内联注入那条路由「计数器本身先得是准的」那组定向用例量（那组里 `produced === captured === 1`）。
**这句不许省**：省掉它，上面的读数看起来比实际更全。

### §2.8 那个「约 7 MB/会话」对不对

`设计/17 §2.8` 的推法是「4566 条 × 均值 3192 B，若一半是 tool_result ⇒ 约 7 MB/会话（UTF-16 下 ~14 MB）」。
拿本轮的单条均值 4 129 码元代进去：2283 条 × 4 129 ≈ **9.4 M 码元 ≈ 18.9 MB**（V8 双字节）。

⚠ **这是外推，不是读数，而且外推的分母是坏的**：这 12 条是**按字节分位分层挑**出来的，
分层样本的均值**不是**总体均值（长尾那几档被刻意过采样了）。
⇒ 能说的只有一句：**§2.8 那个数量级方向是对的（十 MB 级，不是 KB 级）**，
具体几 MB **今天仍然没有读数** —— 要它得在真机会话上跑一次计数器，那属于 §5.5 没做完的另一半。

---

## 2. 丙：三个账本的大小进 `debugSnapshot`

现打的一行快照（`设计/17 §6` 要的三个数已在里面）：

```
[S6-丙] debugSnapshot = {"sid":"s1","scrollTop":0,"scrollHeight":0,"clientHeight":0,
"distBottom":0,"pending":2,"branchRecords":4,"userInputs":3,"midBuffer":0,
"timeline":2,"foldWraps":0,"sentinel":null,"err":null}
```

| 账本 | 快照字段 | 一条里存的是什么 | 口径 |
|---|---|---|---|
| `TailWindow.pending` | `pending`（本来就有） | **整条 payload** —— 三个里**只有它是真的文本驻留** | 条数 |
| `BranchFolder.records` | `branchRecords`（本轮新加） | `{uuid, parentUuid, timestamp}` 三个短字符串，**不含正文** | 条数 |
| `Tab.userInputs` | `userInputs`（本轮新加） | **截断到 80 字**的摘要（字段头注里有真机分母：中位 0 B / p99 16.6 KB / max 197 KiB） | 条数 |

🔴 **三个数不可加**。它们的「一条」不是同一种东西，加起来没有意义。

### `branchRecords` 为什么是按结构读私有字段

`BranchFolder.records` 是 private，而本轮写区**不含** `src/branch-fold.ts`（同一棵树上还有别路 agent 在写）。
TS 的 `private` 只活在编译期 ⇒ `branchRecordCount` 按名字读一次。**读不到时返 `-1` 而不是 `0`**：
返 0 会被读成「账本是空的」—— 那是一句**朝着「看起来一切正常」方向**的假话，
正是 `设计/17 §6` 反复点名的那一族（坏掉的尺子把真缺陷一起藏起来）。
判据里有一格专钉「它不许是 -1」，死值验 M8 就是拔这根针。

### 反空真自检

「某个账本恒为空时这张表照样会绿」是这杆秤最容易得的病。本轮的解药是两条：

1. **造一个三个账本同时非空、而且三个数互不相同的局面**（`branchRecords=4` / `userInputs=3` / `pending=2`）。
   三个数分开是刻意的：若三个都是 2，把 `branchRecords` 错接成 `userInputs` 这类**接错线**照样会绿。
2. **绿全部来自相等断言**（`toBe`），一处 `toBeGreaterThan` 都没用在账本读数上；
   而且先用**第二条路**（直读 `BranchFolder` 的私有数组 / `tab.userInputs.length` / `tab.window.pendingCount`）
   断言三者确实是 4/3/2，再断言快照等于它们。

另外三格把「写死的常数」这条路也堵了：再喂两条 ⇒ 三个数各自按各自的规则动（4→6 / 3→5 / 2→3）；
重投同 uuid ⇒ 三个数一个都不许涨；上翻补批 ⇒ `pending` 归零而另外两个**一条不少**。

---

## 3. 死值验（`bash tests/evidence/S6-mutations.sh`，8 刀全红）

每刀只拔一根针，跑完立刻还原（`trap` 兜底）。原文：

```
── 刀：M1 甲·出口侧不累加（producedUnits += 0）
  ✓ 判据当场红：Tests  5 failed | 8 passed (13)
── 刀：M2 甲·闭包侧不累加（capturedUnits += 0）
  ✓ 判据当场红：Tests  3 failed | 10 passed (13)
     AssertionError: expected +0 to be 9024 // Object.is equality
── 刀：M3 甲·出口侧不计条数（produced += 0）
  ✓ 判据当场红：Tests  4 failed | 9 passed (13)
── 刀：M4 乙·切断闭包→body 那条线（body 不再拿闭包里那份文本）
  ✓ 判据当场红：Tests  1 failed | 12 passed (13)
     × 展开前整个 DOM 里找不到正文；不喂第二遍、只展开一下 ⇒ 正文一字不差地回来了
     AssertionError: expected '' to be '第一行很短。\n填充正文一二三四五六七八九十。…'
── 刀：M5 丙·branchRecords 写死 0
  ✓ 判据当场红：Tests  3 failed | 10 passed (13)
     AssertionError: expected +0 to be 4 // Object.is equality
── 刀：M6 丙·userInputs 写死 0
  ✓ 判据当场红：Tests  3 failed | 10 passed (13)
     AssertionError: expected +0 to be 3 // Object.is equality
── 刀：M7 丙·pending 写死 0
  ✓ 判据当场红：Tests  2 failed | 11 passed (13)
     AssertionError: expected +0 to be 2 // Object.is equality
── 刀：M8 丙·账本读不到 ⇒ branchRecordCount 返哨兵 -1
  ✓ 判据当场红：Tests  4 failed | 9 passed (13)
     × `branchRecords` 不许是 -1 —— -1 = 那根尺子断了（字段被改名读不到）

全部 8 刀都让判据当场红 ⇒ 秤 6 不是装饰。
```

### 🔴 M8 第一版是一把**坏刀**，而且它当场「看起来是好刀」

M8 原本改的是 `folder as unknown as { records?: unknown }` 里的**类型名**。
那是编译期的东西，vitest 只转译不做类型检查 ⇒ 运行期 `inner.records` 照样读得到，
**判据应该仍绿**。而第一次跑的时候它**红了** —— 红的原因是当时同一棵树上别路 agent 正在改
`src/branch-fold.ts`，文件停在一个 `ReferenceError: branchLedger is not defined` 的中间态，
整个丙组一起炸。

⇒ **坏掉的邻居会把一把坏刀伪装成好刀。** 死值验只有在「除这一刀之外一切正常」时才是判据；
本轮的做法是等邻居那边 `npx tsc --noEmit` 干净、基线 13 格全绿之后**重跑一遍**，
并把 M8 改成运行期真会发生的形状（直接返哨兵 `-1`）。第二遍才是上面那份原文。

---

## 4. ⚠ 它量不到什么（这一段不完整本身就是缺陷）

1. **这不是进程 RSS。** 它答的是「**我们自己的账本/闭包里存了多少**」。
   DOM 节点本身、marked/katex 的中间产物、V8 的字符串内部表示（rope / sliced string / 去重）、
   GC 的实际时机 —— 一个都不在里面。想要 RSS 得在真 WebView2 里用 devtools 或系统计数器量。
2. **`resultTextLedger` 是累计流量，不是瞬时驻留。** 同一条 result 被重渲一次
   （`replaceChildren` 那一支）就再记一次，而旧闭包此刻已可回收 ⇒ 它是**驻留量的上界**。
   要瞬时值还是得 heap snapshot（`设计/17 §5.5` 的另一半，**本轮没做** —— jsdom 里没有 retainer 图）。
3. **单位是 UTF-16 码元，不是字节。** 换算成堆上的量级要 ×2（V8 非 Latin-1 双字节）。
   上面每处都同时给了两个单位，别只抄一个数。
4. **三个账本量的是条数，不是字节。** `pending` 一条是整条 payload（几 KB 级），
   `branchRecords` 一条是三个短字符串（百字节级），`userInputs` 一条是 80 字摘要。
   **不可加。** 「三个账本一共占多少内存」这个问题**今天没有读数**。
5. **语料是 69 条的定长夹具，不是真机会话**，而且 12 条 tool_result 是**分层抽样**的
   ⇒ 单条均值不能当总体均值用（见 §1 末）。「一次真会话到底留了多少 MB」**今天仍然没有读数**。
6. **没量 `midBatchBuffer`。** 那句被核对的原话里它与 `TailWindow` 并列，而设计 §6 只点了三个账本，
   本轮照着做 ⇒ 第四个账本没进快照。它每批 flush 后置空，但「一批有多大」没有读数。
7. **jsdom ≠ 生产。** 这里所有「文本在不在 DOM 上」的断言在真 WebView2 里应当同形（纯 DOM 语义，
   不依赖布局），但**没在真浏览器里复核过**。

---

## 5. 复算

```bash
cd cc-monitor
npx vitest run tests/scale6-memory-ledger.vitest.ts            # 13 格，全绿
npx vitest run tests/scale6-memory-ledger.vitest.ts --reporter=verbose   # 带读数打印
bash tests/evidence/S6-mutations.sh                            # 8 刀，应当 8 刀全红
npx vitest run tests/scale2-height-truth.vitest.ts             # 12 格：证明本轮没动 DOM 产物
```

**没动 DOM 产物的证据**：`tests/scale2-height-truth.vitest.ts` 用 DOM 指纹逐卡钉着卡片产物，
本轮动手**前**（12 passed）与动手**后**（12 passed）各跑一次，两次都绿。
本轮往 `src/cards/index.ts` 加的全是纯计数（三处整数加法），
`createElement` / `textContent` / `innerHTML` / `className` 一个字都没碰。

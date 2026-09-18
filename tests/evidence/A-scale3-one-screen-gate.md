# 秤 3 · 一屏门控行为读数（第一份）

> 出处：`设计/17-算法与复杂度.md §6` 表第 3 行「一屏门控行为读数」。
> 产出日期：2026-09-18 · 产出者：波 0 · A 路
> 仪表：`tests/scale3-one-screen-gate.vitest.ts`

## 复算命令

```
cd /home/zbl/文档/claudecode-frontend/cc-monitor
npx vitest run tests/scale3-one-screen-gate.vitest.ts --reporter=verbose
```

（`--reporter=verbose` 是必须的：读数走 `console.log`，默认 reporter 把 stdout 吞掉。）

## 🔴 先说这杆秤**不是**设计要的那一杆

`设计/17 §6` 给秤 3 的装法逐字是：

> 仓里已有 `debugSnapshot()`（`tabs.ts:785-805`，DEV-only）**已在读前三个**，只差"视口内真实可见卡数" ⇒ 加一行 filter

那一行 filter 要加在 `src/tabs.ts` —— **本轮 `tabs.ts` 归 C 路，A 路不许碰**。
所以这里装的是**替代品**，三条差别必须跟着读数一起走，不许只引数字不引这一节：

| | 设计要的 | 这份读数实际做的 |
|---|---|---|
| 量 `scrollHeight` | 真浏览器的 `el.scrollHeight` | **Σ 各卡的 `contain-intrinsic-size` 估值**（这是浏览器对从未绘制过的 `content-visibility:auto` 卡所做的事，但**没有实测证明**浏览器就是这么加的） |
| 量「视口内真实可见卡高」 | `getBoundingClientRect().height` | **按 `styles.css` token 手算**的真高（`设计/17 §5.3`：这类"估得准不准"今天零读数） |
| 跑在哪 | 真 WebView2 / e2e | **jsdom**（无布局引擎，`scrollHeight`/`offsetHeight` 恒 0） |

⇒ 这份读数**能**证伪的：「估值 × 门控算术」这一条链会不会导致半屏。
⇒ 这份读数**不能**证伪的：浏览器的 `scrollHeight` 到底怎么累加、`branch-fold` reparent
之后 `auto` 记忆还在不在（`设计/17 §5.1`，明标分不清）。**那两条要秤 2 才有答案。**

## 门控仿真的对象

`src/tabs.ts:632-639 materializeUntilFilled`，循环体逐字抄进测试：

```ts
for (let round = 0; round < 4; round++) {
  if (tab.window.pendingCount === 0) return;
  if (round > 0 && el.scrollHeight - el.clientHeight > 1) return;
  this.materializeTail(tab);      // takeTail(MATERIALIZE_TAIL_K = 150)
}
```

生产常数（住址）：`tabs.ts:467 MATERIALIZE_TAIL_K = 150`、`tabs.ts:635 round < 4`、
`styles.css:1599 contain-intrinsic-size: auto 120px`。视口取 `clientHeight = 800px`
（`tabs.ts:470 TOP_TRIGGER_PX = 800` 是同一量级的旁证，**不是同一个量**，别当实测视口引）。

判据逐字（`设计/17 §6`）：**「返回后视口内真实可见卡的累计高度 ≥ clientHeight」**。

## fixture：故意构造的「重试风暴」

`设计/17 §6` 数据源纪律逐字：

> §6 秤 3 额外需要一个**故意构造**的 fixture：150 条里塞 ≥60 条 `card-api-retry`。
> ⚠ **已查证：`evidence/` 里没有现成样本**（`api_error` / `api-retry` / 重试风暴 三个词零命中，
> 整个目录只有 1 个 `.jsonl`）⇒ 这个 fixture 必须手工构造，别去那儿找。

本轮照此手工构造：每 150 条一块，其中 n 条是 `system` + `subtype:"api_error"`
（→ `cards/index.ts:266` 建 `card-api-retry`），其余 150−n 条是普通 `system`
（→ `cards/index.ts:276 return {kind:"skip"}`，**占 `takeTail` 的配额但不产卡**）。账本 4 块共 600 条。

🔴 **缺什么，写在这里**：真实的重试风暴里「每 150 条 payload 有几条 retry、其余是不是 skip」
**我们没有样本**。`~/.claude/projects` 本轮读不到（权限拦截），`tests/evidence/` 按设计自陈也没有。
⇒ 下表里的 n 是**扫出来的整个区间**，不是"真机就是这个数"。真机落在哪一档，**判不了**。

## 读数（逐字 stdout）

```
[秤3] 一屏门控 · retry 密度扫描（每 150 条一批，clientHeight=800px，账本 4 批）
  n/150 | 修前轮数 | 修前真高 | 修前判据 | 修后轮数 | 修后真高 | 修后判据
    5 |  2 |   231px | 🔴红 |  4 |   461px | 🔴红
   10 |  1 |   231px | 🔴红 |  4 |   922px | 绿
   20 |  1 |   461px | 🔴红 |  2 |   922px | 绿
   30 |  1 |   691px | 🔴红 |  2 |  1383px | 绿
   40 |  1 |   922px | 绿 |  1 |   922px | 绿
   60 |  1 |  1383px | 绿 |  1 |  1383px | 绿
   80 |  1 |  1844px | 绿 |  1 |  1844px | 绿
  120 |  1 |  2766px | 绿 |  1 |  2766px | 绿
  150 |  1 |  3458px | 绿 |  1 |  3458px | 绿

[秤3] 设计 §6 指定的 fixture（150 条里 60 条 card-api-retry）：
  修前：1 轮 · 估值 7200px · 真高 1383px · 判据 绿
  修后：1 轮 · 估值 1440px · 真高 1383px · 判据 绿
```

「修前」= `card-api-retry` 估不出高、落 CSS 的 120px 兜底（本轮改常数之前的行为）。
「修后」= 本轮加的 `card-api-retry → 24`。

## 读数：卡型覆盖

人群从 `src/cards/*.ts` 的 `"card card-*"` 字面量**派生**（不是手抄名单——手抄正是
「加了新卡型没人回来加常数」那个病的复发机制）。射程：动态拼 class 名的、在
`src/cards/` 之外建的卡，**扫不到**。

```
[秤3] src/cards/ 派生的卡型（9 个）：card-api-error · card-api-retry · card-assistant ·
      card-bash-input · card-bash-output · card-compact · card-slash · card-tool-group · card-user
```

本轮之后 **9/9 全部估得出高**，零个落 120px 兜底。
（`card-compact` / `card-tool-group` 是 `<details>`，走「折叠态 summary 一行 = 38px」那一支。）

## 这份读数答了什么

### 1. 这杆秤**会响**——它不是一个从来没红过的死检查

`n=20` 一档：修前判据红（真高 461px < 一屏 800px），修后转绿（922px）。
两个断言都在测试里（`🔴 修之前…` / `🟢 修之后…`），**同一份 fixture、只换估高函数**。
⇒ 「没红 ≠ 守住了」这一条在这里不适用：红态被真的看见过。

### 2. 🔴 设计 §6 那句「这条今天**必然**在重试风暴会话上红」——**在它自己指定的密度上不成立**

设计给的 fixture 是「150 条里 ≥60 条 retry」。实际跑出来：60 条 retry 的**真高就有 1383px**，
已经 > 一屏 800px ⇒ **判据绿**，改不改常数都绿。

半屏只发生在 **retry 稀疏**的那一段。三段边界（按 23.05px 真高 / 800px 视口 / 4 轮封顶推，
与上表逐格对得上）：

| 区间 | 修前 | 修后 | 说明 |
|---|---|---|---|
| n ≤ 8 | 红 | **仍红** | 4 轮跑满也只有 4n 张卡，4n×23.05 < 800 |
| 9 ≤ n ≤ 33 | 红 | **转绿** | **这一段就是改常数真正修掉的那块**。🔴 **2026-09-18 复算订正**：原写 `9 ≤ n ≤ 34`，全区间 n=1..150 重跑后是 **[9, 33]**，且 **`n=17` 是一个洞**（改完仍红）、**`n=34` 也仍红**。两个孤点成因同一个：都停在「累计 **34 张卡**」上 —— 估值常数 24px 比手算真高 23.05px 高 4%，`34×24=816 > 801` 判「满了」，而 `34×23.05=783.7 < 800` 实际仍是半屏。**同一个病的残留，不是新病。** ⇒ 要整段成立取 **18–30** 或 **9–16** |
| n ≥ 35 | 绿 | 绿 | 一批 150 条里的 retry 真高本来就够一屏 |

⇒ **设计 §6 的 fixture 规格本身要订正**：要能「红 → 绿」，`≥60 条/150` 应改成 `10–30 条/150`。

⚠ 这不推翻 `设计/17 §2.2` 的**另一半**：「虚高 ⇒ 门控提前停」在 60 条那一档照样成立
（估值 7200px vs 真高 1383px，5.2× 虚高，第二轮直接 return）。§2.2 算的是虚高，
§6 判的是「够不够一屏」——**两件事**，60 条那一档是"虚高但碰巧够一屏"。

### 3. 🔴 改常数只修掉半屏的**一半**

`n=5` 一档：修完常数**仍然红**。估值不再虚高 ⇒ 门控跑满 4 轮，但
`tabs.ts:635` 的 `round < 4` 把一次调用能补的量封死在 4×150 = 600 条，
600 条里只有 20 张卡 ⇒ 461px，仍不足一屏。

⇒ **「轮数封顶」是半屏的第二个成因，改常数碰不到它。** 它归 `设计/17 §2.3`（门控换判据）。
⇒ 任何人拿这份读数说「半屏已修复」都是越读了。

## 这份读数**没有**答什么

1. **真高是手算的，不是量的。** `card-api-retry` 的 23.05px = `padding 3×2 + 11px × 1.55`，
   按 `.card-api-retry` 的 CSS token 推（现打住址 `styles.css:1974`；原写 `:2110` 是漂的，那里今天是一条 `width: 14px`）。`设计/17 §5.3` 明写这类数今天零读数。秤 2 落地后必须回来重校。
2. **浏览器的 `scrollHeight` 是不是真等于 Σ 估值**——没测。规范上 `contain-intrinsic-size`
   对未绘制元素承担尺寸，但「绘制过一次之后 `auto` 记忆接管」的时机、以及 fold reparent
   之后还在不在（`设计/17 §5.1`），都**实现相关，必须实测**。
3. **真机视口是不是 800px**——没测。800 是取的典型值。
4. **真实重试风暴的 retry 密度**——没样本（见上）。
5. **一次强制布局读多少钱**——`设计/17 §5.2` 明标分不清，本秤**刻意没给它任何数**。
6. **`contain-intrinsic-size` 是 content-box 还是 border-box**：`height-estimate.ts:190-192`
   的注释说是 content-box（"盒模型自会另加"），而设计 §2.2 给的 24 / 32 两个数是**含 padding 的
   border-box 值**。本轮按设计给的数照改（§7 逐字指定），**但这两者不自洽**，见报告「我判不了的」。

## 结论一句话

改常数把**虚高提前停**那一半修掉了（红 → 绿的区间 **n ∈ [9, 33] 且 17 是洞**，见上表的复算订正；本句原写 `[7, 34]`，与同文件上表的 `9 ≤ n ≤ 34` **互相不一致**，两个都不对）；
**轮数封顶**那一半没修，最稀疏那一档仍然红。
设计 §6 指定的 fixture 密度（≥60/150）**红不了**，那条 fixture 规格要订正。
以上全部是 jsdom + 手算真高下的读数，**真机行为仍是零读数**。

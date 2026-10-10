/**
 * 秤 3 ——「一屏门控」行为读数 + 回归判据（表第 3 行）。
 *
 * 设计逐字要的是：
 *   「`materializeUntilFilled` 实际跑几轮、每轮的 (scrollHeight, clientHeight,
 *     timeline.size, 视口内真实可见卡数)」
 *   「判据 =「返回后视口内真实可见卡的累计高度 ≥ clientHeight」」
 *
 * 🔴 **本文件量的是「旧门控」，不是今天的生产路径。先读完这一段再看下面任何一个绿。**
 *
 * 写下它时，`materializeUntilFilled` 的停手判据是 `scrollHeight - clientHeight > 1`
 * ——**一份纯算术**，视口外那些从没绘制过的卡按 `contain-intrinsic-size` 估值计入。
 * 本文件把那份算术仿真了一遍（`simulateGate`）。
 * **而生产侧已经改掉了**：停手判据换成 `TabManager::contentReachesBottom()`——
 * 读**最后一张卡的真实 `getBoundingClientRect().bottom`** 有没有够到滚动容器下沿，
 * 不吃任何估值（「门控换判据」落地的那一步，代码在 `tabs.ts`）。
 *
 * ⇒ 所以本文件今天的身份变了，结论也要跟着重述：
 *   **它证的不是「今天的门控是对的」，而是「旧门控会导致半屏 —— 这就是去改它的理由」。**
 *   它是那次改动的**动因存档 + 反向回归判据**（旧算术若被谁改回来，这里的红态区间会重现）。
 * ⚠ **这是一次静默失配**：判据换了，而这杆秤照样全绿 —— 因为它量的是自己抄下来的
 *   那份算术，不是生产代码。**绿在这里不构成「生产路径没坏」的任何证据。**
 *   今天的生产路径由谁来量：只有秤 2（e2e 真值对照）答得了，而那个**还没造**。
 *
 * ⚠ 除此之外，这份实现从一开始就**不是**设计原本设想的那杆秤，差别也必须写在前面：
 *
 * 1. 设计的装法是给 `TabManager::debugSnapshot()` 加一行 filter，在**真浏览器**里
 *    读真的 `scrollHeight`/`offsetHeight`。本文件一行 `src/` 都没改，且跑在 jsdom 里
 *    ——**jsdom 没有布局引擎**，`scrollHeight` 恒 0、`offsetHeight` 恒 0。
 *    （⚠ 设计原文写的是 `tabs.ts:785-805`，那个行号今天指到 `fillAbove` 上了。
 *      **行号是每一轮都会变的量**，本注一律用符号地址，别再抄行号。）
 * 2. 所以这里做的是**门控算术的仿真**：把**旧的** `materializeUntilFilled` 那 5 行循环
 *    原样抄过来，`scrollHeight` 用「Σ 各卡的 contain-intrinsic-size 估值」
 *    代入（这正是浏览器对**从未绘制过**的 `content-visibility: auto` 卡所做的事），
 *    「真实可见高度」用「Σ 按 styles.css token 手算的真高」代入。
 * 3. ⇒ 本文件能证伪的是「**估值 × 门控算术**会不会导致半屏」。它证伪不了
 *    「浏览器的 scrollHeight 是不是真按估值累加」「reparent 之后 auto 记忆还在不在」
 *    （明标分不清）。那些要秤 2（e2e 真值对照）才有答案。
 *
 * ⚠ 另一条纪律：自陈「估得准不准」今天**零读数**。本文件里的
 *    `TRUE_H_PX` 是**按 CSS token 手算**的，不是量出来的。所以下面所有「真高」
 *    都带着这个前提；秤 2 落地后要拿真值回来重校。
 *
 * 复算：`npx vitest run tests/frontend/ui/scale3-one-screen-gate.vitest.ts`
 */
import { describe, it, expect, vi, afterEach } from "vitest";
import { readdirSync, readFileSync } from "node:fs";
import { resolve } from "node:path";
import { estimateStreamNodeHeight, __resetUnknownCardWarnings } from "../../../src/frontend/ui/height-estimate";
import { buildApiRetryCard } from "../../../src/frontend/ui/cards/api-error";
import { buildBashInputCard, buildBashOutputCard } from "../../../src/frontend/ui/cards/bash";

// ── 生产侧常数（逐字抄自源码，改了这里要回去核对住址）────────────────────────
/** `tabs.ts:467 MATERIALIZE_TAIL_K` —— 门控循环每轮取这么多条 payload */
const MATERIALIZE_TAIL_K = 150;
/** `TabManager::materializeUntilFilled` 里那个 `round < 4`（`tabs.ts`，符号地址；原写 `:635` 已漂） */
const MAX_ROUNDS = 4;
/** 典型视口。`tabs.ts:470 TOP_TRIGGER_PX = 800` 是同一量级的旁证，不是同一个量。 */
const CLIENT_HEIGHT = 800;
/** `styles.css:1599 .stream-content > * { contain-intrinsic-size: auto 120px }` */
const CSS_FALLBACK_PX = 120;

// ── 真高：按 styles.css token 手算（**不是实测**，见头注）──────────────────────
const TRUE_H_PX = {
  // .card-api-retry: padding 3×2 + --font-size-xs 11px × 行高 1.55
  "card-api-retry": 3 * 2 + 11 * 1.55,
  // .card-bash-input: padding 6×2 + --font-size-small 12px × 1.55
  "card-bash-input": 6 * 2 + 12 * 1.55,
  // .card-slash: 与 bash-input 同一套紧凑系 token
  "card-slash": 6 * 2 + 12 * 1.55,
} as const;

// ── 门控仿真 ──────────────────────────────────────────────────────────────────

type Estimator = (el: HTMLElement) => number | null;

/**
 * 修之前的估高行为：只认 4 个 class，其余落 CSS 的 120px 兜底。
 * 留着它是为了让这杆秤**能被看到响一次** —— 一个从来没红过的检查，分不清是
 * 「守住了」还是「根本没接上」。
 */
const PRE_FIX_ESTIMATOR: Estimator = (el) => {
  if (el.tagName === "DETAILS" && !(el as HTMLDetailsElement).open) return 38;
  if (el.classList.contains("card-user")) return 40;
  if (el.classList.contains("card-api-error")) return 40;
  if (el.classList.contains("card-slash")) return 34;
  if (el.classList.contains("card-assistant")) return 42;
  return null;
};

interface GateRound {
  round: number;
  /** 该轮开始前的 scrollHeight（= Σ 已材料化卡的 contain-intrinsic-size） */
  scrollHeightBefore: number;
  recordsTaken: number;
  cardsAfter: number;
}

interface GateReading {
  rounds: GateRound[];
  /** 循环退出时一共材料化了几条 payload */
  recordsMaterialized: number;
  cards: number;
  /** 门控**以为**的高度（估值累加）—— 它就是 `scrollHeight` 的替身 */
  estimatedPx: number;
  /** 按 CSS token 手算的真高累加 */
  truePx: number;
  /** 秤 3 的判据：真高 ≥ clientHeight */
  passes: boolean;
}

/** 一条 payload 渲染出来的东西：一张卡，或者什么都不产（`kind:"skip"`） */
type Record = { card: HTMLElement; trueH: number } | null;

/**
 * **旧** `materializeUntilFilled` 的算术仿真（当时的循环体逐字对齐）：
 *
 * ```
 * for (let round = 0; round < 4; round++) {
 *   if (tab.window.pendingCount === 0) return;
 *   if (round > 0 && el.scrollHeight - el.clientHeight > 1) return;   // ← 已被生产侧换掉
 *   this.materializeTail(tab);          // takeTail(MATERIALIZE_TAIL_K)
 * }
 * ```
 *
 * 🔴 **生产侧第二行今天是 `if (round > 0 && this.contentReachesBottom(tab)) return;`**——
 * 读最后一张卡的真实 `bottom`，不吃估值。**本函数刻意保留旧算术**，它存的是
 * 「旧门控为什么必须换」的证据，不是今天的行为。别照它去读生产代码，也别把它改成新的
 *（改了就没人存着那份红态了）。住址用符号名，不写行号——文件头注解释了为什么。
 */
function simulateGate(
  ledger: readonly Record[],
  estimate: Estimator,
  clientHeight = CLIENT_HEIGHT,
): GateReading {
  const materialized: NonNullable<Record>[] = [];
  let estimatedPx = 0;
  let truePx = 0;
  let pending = ledger.length;
  let cursor = 0;
  const rounds: GateRound[] = [];

  for (let round = 0; round < MAX_ROUNDS; round++) {
    if (pending === 0) break;
    if (round > 0 && estimatedPx - clientHeight > 1) break;
    const before = estimatedPx;
    const take = Math.min(MATERIALIZE_TAIL_K, pending);
    for (let i = 0; i < take; i++) {
      const rec = ledger[cursor++];
      if (rec) {
        materialized.push(rec);
        // 估不出高 → 不写 inline style → CSS 的 120px 兜底接管
        estimatedPx += estimate(rec.card) ?? CSS_FALLBACK_PX;
        truePx += rec.trueH;
      }
    }
    pending -= take;
    rounds.push({
      round,
      scrollHeightBefore: before,
      recordsTaken: take,
      cardsAfter: materialized.length,
    });
  }

  return {
    rounds,
    recordsMaterialized: cursor,
    cards: materialized.length,
    estimatedPx,
    truePx,
    passes: truePx >= clientHeight,
  };
}

// ── 故意构造的「重试风暴」fixture ─────────────────────────────────────────────
// 数据源纪律逐字：「§6 秤 3 额外需要一个**故意构造**的 fixture:
// 150 条里塞 ≥60 条 `card-api-retry`」＋「已查证:`evidence/` 里没有现成样本
// ⇒ 这个 fixture 必须手工构造,别去那儿找」。
//
// ⚠ 手工构造的代价写在这里,不许忘:**真实的重试风暴长什么样,我们没有样本**。
// 下面的「每 150 条里几条 retry、其余几条是 skip」是**假设**,不是实测的会话形状。

/** 不建卡的那种记录（自动应答）→ `renderMessage` 交 `{kind:"skip"}`，占配额不产卡 */
const SKIP: Record = null;

function retryRecord(i: number): NonNullable<Record> {
  return {
    card: buildApiRetryCard({
      timeLabel: "12:34",
      retryAttempt: (i % 5) + 1,
      maxRetries: 5,
      reason: "network",
    }),
    trueH: TRUE_H_PX["card-api-retry"],
  };
}

/** n 条 retry + (150 − n) 条 skip，重复 `blocks` 块 */
function retryStorm(retriesPerBlock: number, blocks: number): Record[] {
  const out: Record[] = [];
  for (let b = 0; b < blocks; b++) {
    for (let i = 0; i < MATERIALIZE_TAIL_K; i++) {
      out.push(i < retriesPerBlock ? retryRecord(b * MATERIALIZE_TAIL_K + i) : SKIP);
    }
  }
  return out;
}

// ── 用例 ──────────────────────────────────────────────────────────────────────

describe("秤 3 · 三个细条卡型不再落 CSS 的 120px 兜底", () => {
  it("card-api-retry / card-bash-input / card-bash-output 都估得出高", () => {
    const retry = buildApiRetryCard({ timeLabel: "12:34", retryAttempt: 1, maxRetries: 5 });
    const bashIn = buildBashInputCard({ command: "npm run build" }, "12:34");
    const bashOut = buildBashOutputCard(
      { stdout: "line\n".repeat(5).trim(), stderr: "" },
      "12:34",
    );
    for (const el of [retry, bashIn, bashOut]) {
      expect(estimateStreamNodeHeight(el), el.className).not.toBeNull();
    }
  });

  it("两条细条常数与 CSS token 手算的真高（content-box）相差在 ±20% 内", () => {
    const retry = buildApiRetryCard({ timeLabel: "12:34", retryAttempt: 1, maxRetries: 5 });
    const bashIn = buildBashInputCard({ command: "npm run build" }, "12:34");
    // 🔴 2026-09-18（秤 2 的 B 段，两个真引擎实测）：`contain-intrinsic-size` 是
    //    **content-box** —— 声明 N ⇒ 视口外实际占 N + padding + border。
    //    ⇒ 估值常数要对的是**扣掉 padding 之后**的那个真高，不是上面 `TRUE_H_PX` 的 border-box 值。
    //    同一天 `height-estimate.ts` 的三条常数按这个语义改了（24/32/34 → 17/19/19），
    //    这一格的口径跟着改；比 border-box 是拿两套盒模型对账，那正是被秤 2 抓到的那个病。
    const PAD_PX = { "card-api-retry": 3 * 2, "card-bash-input": 6 * 2 } as const;
    const pairs: [HTMLElement, number][] = [
      [retry, TRUE_H_PX["card-api-retry"] - PAD_PX["card-api-retry"]],
      [bashIn, TRUE_H_PX["card-bash-input"] - PAD_PX["card-bash-input"]],
    ];
    for (const [el, trueH] of pairs) {
      const est = estimateStreamNodeHeight(el);
      expect(est).not.toBeNull();
      const relErr = Math.abs((est as number) - trueH) / trueH;
      // ⚠ 这里比的是「常数 vs 手算」,**不是**「常数 vs 真实布局高度」——后者是秤 2 的活
      //   (`tests/frontend/ui/scale2-height-truth.vitest.ts`,真浏览器金标准,2026-09-18 已落地)。
      // ⚠ 也**不是**「落地值 vs 真高」——那一格的读数在秤 2 里。
      //   🔴 原文写着「`Math.max(24,…)` 地板会把这两个数一律顶成 24
      //   ⇒ 真正写进 style 的仍是 24」。**那个地板已经整个去掉了**（`99` 条 75 /
      // 订正④）⇒ 今天 17/19 **原样出货**，落地值就是 `round(估值)`。
      //   判据本体不受影响（它比的是「常数 vs 手算」），但这句话不改就是一条会骗人的散文。
      expect(relErr, `${el.className} est=${est} hand-calc=${trueH.toFixed(1)}`).toBeLessThan(0.2);
    }
  });
});

describe("秤 3 · 卡型覆盖：没有一个卡型还在落 120px 兜底", () => {
  /**
   * 人群从**源码**派生，不是手抄一份名单 —— 手抄的名单正是那个病
   * （加了新卡型没人回来加常数，静默落回 120px）的复发机制。
   *
   * 射程（要写清，别让人当成"全仓卡型都覆盖了"）：只扫 `src/frontend/ui/cards/*.ts` 里
   * 形如 `"card card-xxx"` 的字面量。动态拼 class 名的、别处建的卡，**扫不到**。
   */
  // `__dirname` 在 vitest 里指向 `tests/`（同 `account-base-semantics.vitest.ts:36` 的用法）
  const CARD_SRC_DIR = resolve(__dirname, "../../../src/frontend/ui/cards");
  /** 这两个是 `<details>`，估高走「折叠态 summary 一行」那一支 */
  const DETAILS_CLASSES = new Set(["card-compact", "card-tool-group"]);

  function cardClassesFromSource(): string[] {
    const found = new Set<string>();
    for (const f of readdirSync(CARD_SRC_DIR).filter((n) => n.endsWith(".ts"))) {
      const src = readFileSync(resolve(CARD_SRC_DIR, f), "utf8");
      for (const m of src.matchAll(/"card (card-[a-z0-9-]+)"/g)) found.add(m[1]);
    }
    return [...found].sort();
  }

  it("`src/frontend/ui/cards/` 里每个 `card card-*` 字面量都估得出高", () => {
    const classes = cardClassesFromSource();
    // 扫不到东西说明正则/住址漂了，那比"全绿"更该红
    expect(classes.length).toBeGreaterThanOrEqual(8);
    const unestimated: string[] = [];
    for (const cls of classes) {
      const el = document.createElement(DETAILS_CLASSES.has(cls) ? "details" : "div");
      el.className = `card ${cls}`;
      if (estimateStreamNodeHeight(el) === null) unestimated.push(cls);
    }
    console.log(`[秤3] src/frontend/ui/cards/ 派生的卡型（${classes.length} 个）：${classes.join(" · ")}`);
    expect(unestimated, "这些卡型会落 styles.css 的 120px 兜底").toEqual([]);
  });
});

describe("秤 3 · 认不出的卡型会出声（修法③）", () => {
  afterEach(() => {
    vi.restoreAllMocks();
    __resetUnknownCardWarnings();
  });

  it("陌生 class 落 null 时 DEV 下喊一次，同一个 class 不重复喊", () => {
    __resetUnknownCardWarnings();
    const warn = vi.spyOn(console, "warn").mockImplementation(() => {});
    const el = document.createElement("div");
    el.className = "card card-brand-new-type";

    expect(estimateStreamNodeHeight(el)).toBeNull();
    expect(estimateStreamNodeHeight(el)).toBeNull();
    expect(estimateStreamNodeHeight(el)).toBeNull();

    // DEV 门控：vitest 里 import.meta.env.DEV 为真 ⇒ 应当恰好喊一次。
    // 若哪天测试环境的 DEV 变成 false，这条会红 —— 那正是要知道的（生产被 vite 消除是对的，
    // 但**测不到**就等于这条守卫没人证明它接上了）。
    expect(warn).toHaveBeenCalledTimes(1);
    expect(String(warn.mock.calls[0][0])).toContain("card-brand-new-type");
  });

  it("不同的陌生 class 各喊各的", () => {
    __resetUnknownCardWarnings();
    const warn = vi.spyOn(console, "warn").mockImplementation(() => {});
    for (const cls of ["card-aaa", "card-bbb", "card-aaa"]) {
      const el = document.createElement("div");
      el.className = `card ${cls}`;
      estimateStreamNodeHeight(el);
    }
    expect(warn).toHaveBeenCalledTimes(2);
  });
});

describe("秤 3 · 一屏门控：这杆秤能红吗", () => {
  // 20 条 retry / 150 条：估值 20×120 = 2400px 早就"够一屏"了,真高只有 20×23 = 461px
  const SPARSE_STORM = retryStorm(20, 4);

  it("🔴 修之前：稀疏重试风暴上判据红（=> 这杆秤确实会响，不是死的）", () => {
    const r = simulateGate(SPARSE_STORM, PRE_FIX_ESTIMATOR);
    expect(r.rounds.length).toBe(1); // 第二轮门控直接 return
    expect(r.passes).toBe(false); // 真高 < 一屏 —— 半屏
    expect(r.truePx).toBeLessThan(CLIENT_HEIGHT);
  });

  it("🟢 修之后：同一个 fixture 上门控继续补批，判据转绿", () => {
    const r = simulateGate(SPARSE_STORM, estimateStreamNodeHeight);
    expect(r.rounds.length).toBeGreaterThan(1); // 估值不再虚高 ⇒ 门控没提前停
    expect(r.passes).toBe(true);
    expect(r.truePx).toBeGreaterThanOrEqual(CLIENT_HEIGHT);
  });

  it("读数：retry 密度扫描（哪一段密度会半屏）", () => {
    const rows: string[] = [];
    for (const n of [5, 10, 20, 30, 40, 60, 80, 120, 150]) {
      const ledger = retryStorm(n, 4);
      const before = simulateGate(ledger, PRE_FIX_ESTIMATOR);
      const after = simulateGate(ledger, estimateStreamNodeHeight);
      rows.push(
        [
          String(n).padStart(3),
          String(before.rounds.length).padStart(2),
          `${before.truePx.toFixed(0).padStart(5)}px`,
          before.passes ? "绿" : "🔴红",
          String(after.rounds.length).padStart(2),
          `${after.truePx.toFixed(0).padStart(5)}px`,
          after.passes ? "绿" : "🔴红",
        ].join(" | "),
      );
    }
    console.log(
      [
        `[秤3] 一屏门控 · retry 密度扫描（每 150 条一批，clientHeight=${CLIENT_HEIGHT}px，账本 4 批）`,
        "  n/150 | 修前轮数 | 修前真高 | 修前判据 | 修后轮数 | 修后真高 | 修后判据",
        ...rows.map((r) => `  ${r}`),
        "  ⚠ 最稀疏那一档（n=5）**改完常数仍然红**：估值不再虚高 ⇒ 门控跑满 4 轮，",
        "     但 `tabs.ts:635` 的 `round < 4` 把一次调用能补的量封在 600 条，",
        "     600 条里只有 20 张卡 ⇒ 461px 仍不足一屏。",
        "     ⇒ 改常数只把「虚高提前停」这一半修掉；「轮数封顶」是**另一半**，",
        "       归（门控换判据）。别拿这份读数当「半屏已修复」。",
        " 另一半已落：生产门控不再到 4 轮为止，一次调用补不满就下一帧接着补 —— 见本文件「丁」段（驱动真 TabStreamView）。",
      ].join("\n"),
    );
    // 这条不断言判据,它只产读数;上面两条才是判据。
    expect(rows.length).toBe(9);
  });

  it("读数：设计 §6 逐字那个 fixture（60 条/150）的实际表现", () => {
    const ledger = retryStorm(60, 4);
    const before = simulateGate(ledger, PRE_FIX_ESTIMATOR);
    const after = simulateGate(ledger, estimateStreamNodeHeight);
    console.log(
      [
        "[秤3] 设计 §6 指定的 fixture（150 条里 60 条 card-api-retry）：",
        `  修前：${before.rounds.length} 轮 · 估值 ${before.estimatedPx.toFixed(0)}px · 真高 ${before.truePx.toFixed(0)}px · 判据 ${before.passes ? "绿" : "红"}`,
        `  修后：${after.rounds.length} 轮 · 估值 ${after.estimatedPx.toFixed(0)}px · 真高 ${after.truePx.toFixed(0)}px · 判据 ${after.passes ? "绿" : "红"}`,
        "  ⇒ 60 条 retry 的真高已经 " +
          before.truePx.toFixed(0) +
          "px > 一屏 800px ⇒ 这一档**不红**。",
        "  ⇒ 设计 §6「这条今天必然在重试风暴会话上红」在这个密度上**不成立**；",
        "     半屏只发生在 retry 稀疏的那一段（见上面的密度扫描）。",
      ].join("\n"),
    );
    // 只产读数,不拿它当门禁 —— 它记的是一条「设计的断言没复现」的事实。
    expect(before.truePx).toBeGreaterThan(CLIENT_HEIGHT);
  });
});

// ─────────────────────────────────────────────────────────────────────────────
// 丁：驱动**生产的** `TabStreamView`（W5-RENDER R7）
// ─────────────────────────────────────────────────────────────────────────────
//
// 上面的仿真抄的是循环的复制品（`MAX_ROUNDS = 4`），它钉的是「旧门控为什么必须换」的红态，不是今天的行为。
// 本段让**真的** `TabManager` / `TabStreamView` 跑一次启动重放：jsdom 没有布局 ⇒ 给 `getBoundingClientRect`
// 装一个按 DOM 序堆叠的假布局（卡高 = 上面同一份 token 手算真高 `TRUE_H_PX`，视口 800 px），
// 生产的 `contentReachesBottom` 就读得到「最后一张卡够没够到下沿」。
//
// 守的要求：「判据换成不依赖估值的量；**并连轮数封顶（`round < 4`）一起处置**」·
// 「物化循环硬上限 4 轮 × 150 条 —— 秤 3 证实最稀疏那一档估高再准也补不满一屏」。
// 判据：重放结束、帧跑完之后，「真实可见卡高 ≥ 视口」或「账本空了」二者必居其一（稀疏风暴 8 块 × 150 条，每块 5 条 retry）；
// 同时**一次同步调用**的物化量仍有界（批结束那一下只取 ≤ 4 × 150 条）—— 余下的下一帧接着补。

vi.mock("@tauri-apps/api/core", async () => {
  const rig = await import("../../test-support/session-viewer-rig");
  const { withSessionReads } = await import("../../test-support/chan-fake");
  return {
    invoke: vi.fn(
      withSessionReads(async (cmd: string, args: { [k: string]: unknown }) =>
        cmd === "list_user_inputs" ? rig.answerListUserInputs(args as { fromOffset: number }) : undefined,
      ),
    ),
  };
});
vi.mock("@tauri-apps/plugin-opener", () => ({ openPath: vi.fn().mockResolvedValue(undefined) }));
vi.mock("../../../src/frontend/ui/tasks-panel", () => ({ fetchSessionTasks: vi.fn().mockResolvedValue([]) }));
vi.mock("../../../src/frontend/ui/turn-notify", () => ({ turnEndNotifier: { observe: vi.fn() } }));
vi.mock("../../../src/frontend/ui/fork-flow", () => ({ runForkFlow: vi.fn().mockResolvedValue(undefined) }));

import { installViewerRig, line as rigLine, withSession, type RigPayload } from "../../test-support/session-viewer-rig";
import { TabManager, type Tab } from "../../../src/frontend/ui/tabs";

describe("秤 3 · 丁：生产门控（驱动真 TabStreamView ＋ 假布局）", () => {
  const VIEW_H = CLIENT_HEIGHT;
  const heightOf = (el: Element): number =>
    el.classList.contains("card-api-retry") ? TRUE_H_PX["card-api-retry"] : 0;
  const rect = (top: number, h: number): DOMRect =>
    ({ top, bottom: top + h, height: h, left: 0, right: 780, width: 780, x: 0, y: top, toJSON: () => ({}) }) as DOMRect;
  let restoreRect: (() => void) | null = null;

  /** 假布局：`.stream` = 视口；`.stream-content` 的直接子节点按 DOM 序堆叠、各取 token 手算真高。 */
  function installFakeLayout(): void {
    const proto = HTMLElement.prototype;
    const orig = proto.getBoundingClientRect;
    proto.getBoundingClientRect = function (this: HTMLElement): DOMRect {
      if (this.classList.contains("stream")) return rect(0, VIEW_H);
      const parent = this.parentElement;
      if (parent?.classList.contains("stream-content")) {
        let top = 0;
        for (const sib of Array.from(parent.children)) {
          if (sib === this) break;
          top += heightOf(sib);
        }
        return rect(top, heightOf(this));
      }
      return rect(0, 0);
    };
    restoreRect = () => {
      proto.getBoundingClientRect = orig;
    };
  }

  afterEach(() => {
    restoreRect?.();
    restoreRect = null;
    vi.unstubAllGlobals();
  });

  const retry = (seq: number): RigPayload =>
    withSession(
      rigLine(seq, { t: "retry", id: `r${seq}`, at: "2026-09-10T00:00:00.000Z", reason: "network", attempt: (seq % 5) + 1, max: 5 }),
      "s2",
    );
  const skip = (seq: number): RigPayload =>
    withSession(
      rigLine(seq, { t: "reply", id: `k${seq}`, at: "2026-09-10T00:00:00.000Z", blocks: [], autoReply: true, endsTurn: false }),
      "s2",
    );

  /** 稀疏风暴：`blocks` 块 × 150 条，每块前 `perBlock` 条是 retry、其余不建卡。 */
  function storm(blocks: number, perBlock: number): RigPayload[][] {
    const out: RigPayload[][] = [];
    for (let b = 0; b < blocks; b++) {
      const blk: RigPayload[] = [];
      for (let i = 0; i < MATERIALIZE_TAIL_K; i++) {
        const seq = b * MATERIALIZE_TAIL_K + i;
        blk.push(i < perBlock ? retry(seq) : skip(seq));
      }
      out.push(blk);
    }
    return out;
  }

  interface Reading {
    /** 批结束那一次同步调用之后：已建卡的真高 / 已出账（建过卡或跳过）的条数 */
    syncPx: number;
    syncTaken: number;
    /** 帧跑完之后 */
    settledPx: number;
    settledPending: number;
    frames: number;
  }

  function run(blocks: number, perBlock: number): Reading {
    const rig = installViewerRig();
    installFakeLayout();
    const barEl = document.createElement("div");
    const streamRootEl = document.createElement("div");
    document.body.append(barEl, streamRootEl);
    const tm = new TabManager(barEl, streamRootEl);
    const tabOf = (sid: string): Tab =>
      (tm as unknown as { store: { tabs: Map<string, Tab> } }).store.tabs.get(sid)!;
    const blks = storm(blocks, perBlock);
    const total = blocks * MATERIALIZE_TAIL_K;
    // 启动重放的到达序：末块先发（钉 floor、直渲），其余块整块收纳
    tm.onBatchStart();
    for (const p of blks[blocks - 1]) tm.onLine(p as never);
    for (let b = 0; b < blocks - 1; b++) for (const p of blks[b]) tm.onLine(p as never);
    tm.switchTo("s2");
    tm.onBatchEnd();
    const tab = tabOf("s2");
    const px = (): number =>
      Array.from(tab.stream.contentElement.children).reduce((s, c) => s + heightOf(c), 0);
    const syncPx = px();
    const syncTaken = total - tab.window.pendingCount;
    let frames = 0;
    for (; frames < 50; frames++) {
      const before = tab.window.pendingCount;
      rig.flushRaf(1);
      if (tab.window.pendingCount === before) break;
    }
    return { syncPx, syncTaken, settledPx: px(), settledPending: tab.window.pendingCount, frames };
  }

  it("稀疏风暴（8 块 × 150 条、每块 5 条 retry）：帧跑完之后满一屏或账本空；一次同步调用 ≤ 末块 ＋ 4 × 150 条", () => {
    const r = run(8, 5);
    console.log(
      `[秤3·丁] 同步：真高 ${r.syncPx.toFixed(0)}px · 出账 ${r.syncTaken} 条 ｜ 帧跑完（${r.frames} 帧）：` +
        `真高 ${r.settledPx.toFixed(0)}px · 账本余 ${r.settledPending} 条`,
    );
    // 反空真：假布局真的被读到了（同步那一下至少末块的 5 张卡在）
    expect(r.syncPx).toBeGreaterThan(0);
    // 一次同步调用的量仍有界：末块直渲 150 ＋ 物化 ≤ 4 × 150
    expect(r.syncTaken).toBeLessThanOrEqual(MATERIALIZE_TAIL_K * (1 + MAX_ROUNDS));
    // 🔴 判据：满一屏，或者账本空了（真没有更多可补）
    expect(r.settledPx >= VIEW_H || r.settledPending === 0).toBe(true);
  });
});

/**
 * 「物化队列重建只收 `floorSeq === null` 的 tab ⇒ 钉过水位但还有 pending 的 tab
 * 只剩上翻一条路」。判据（两向相等）：批结束后空闲物化真的补到的后台 tab 集合 == 手写期望集合；
 * 以及切进一个「钉过水位、没满一屏、`scrollHeight` 被估值撑高」的 tab 时，按真实布局补到满一屏。
 * 同一个假布局台子（本文件「丁」段）：卡高按类 —— retry 23.05 px、assistant 1000 px（一张就满屏）。
 */
describe("秤 3 · 戊：B5 空闲物化队列收哪些后台 tab", () => {
  const VIEW_H = CLIENT_HEIGHT;
  const heightOf = (el: Element): number =>
    el.classList.contains("card-api-retry")
      ? TRUE_H_PX["card-api-retry"]
      : el.classList.contains("card-assistant")
        ? 1000
        : 0;
  const rect = (top: number, h: number): DOMRect =>
    ({ top, bottom: top + h, height: h, left: 0, right: 780, width: 780, x: 0, y: top, toJSON: () => ({}) }) as DOMRect;
  let restore: Array<() => void> = [];

  function installFakeLayout(): void {
    const proto = HTMLElement.prototype;
    const orig = proto.getBoundingClientRect;
    proto.getBoundingClientRect = function (this: HTMLElement): DOMRect {
      if (this.classList.contains("stream")) return rect(0, VIEW_H);
      const parent = this.parentElement;
      if (parent?.classList.contains("stream-content")) {
        let top = 0;
        for (const sib of Array.from(parent.children)) {
          if (sib === this) break;
          top += heightOf(sib);
        }
        return rect(top, heightOf(this));
      }
      return rect(0, 0);
    };
    restore.push(() => {
      proto.getBoundingClientRect = orig;
    });
  }

  afterEach(() => {
    for (const f of restore) f();
    restore = [];
    vi.unstubAllGlobals();
  });

  const sys = (sid: string, seq: number, retryCard: boolean): RigPayload =>
    withSession(
      rigLine(
        seq,
        retryCard
          ? { t: "retry", id: `${sid}-r${seq}`, at: "2026-09-10T00:00:00.000Z", reason: "network", attempt: 1, max: 5 }
          : { t: "reply", id: `${sid}-k${seq}`, at: "2026-09-10T00:00:00.000Z", blocks: [], autoReply: true, endsTurn: false },
      ),
      sid,
    );
  const tall = (sid: string, seq: number): RigPayload =>
    withSession(
      rigLine(seq, {
        t: "reply",
        id: `${sid}-a${seq}`,
        at: "2026-09-10T00:00:00.000Z",
        blocks: [{ type: "text", text: "一张很高的卡" }],
        autoReply: false,
        endsTurn: false,
      }),
      sid,
    );

  it("批结束后空闲物化补到的后台 tab == {virgin 的 a, 钉过水位没满屏的 b}；满屏的 c · 账本空的 d · 已结束的 e · 当前的 act 都不补", () => {
    installViewerRig();
    installFakeLayout();
    const idle: IdleRequestCallback[] = [];
    vi.stubGlobal("requestIdleCallback", (cb: IdleRequestCallback) => idle.push(cb));
    const barEl = document.createElement("div");
    const streamRootEl = document.createElement("div");
    document.body.append(barEl, streamRootEl);
    const tm = new TabManager(barEl, streamRootEl);
    const tabOf = (sid: string): Tab =>
      (tm as unknown as { store: { tabs: Map<string, Tab> } }).store.tabs.get(sid)!;
    // 非批期：act（第一个 ⇒ 当前）· b（一张细条卡钉水位）· c（一张高卡钉水位）· d（一张细条卡钉水位）
    tm.onLine(sys("act", 5000, true) as never);
    tm.onLine(sys("b", 5000, true) as never);
    tm.onLine(tall("c", 5000) as never);
    tm.onLine(sys("d", 5000, true) as never);
    // 批期：a / e 是 virgin 后台 tab；b / c 收到更早的行（seq < floor ⇒ 收纳）
    tm.onBatchStart();
    for (const sid of ["a", "b", "c", "e"]) for (let s = 0; s < 150; s++) tm.onLine(sys(sid, s, s % 10 === 0) as never);
    tm.archiveTab("e");
    const before = new Map(["a", "b", "c", "d", "e", "act"].map((sid) => [sid, tabOf(sid).window.pendingCount]));
    tm.onBatchEnd();
    for (let i = 0; i < 20 && idle.length > 0; i++) idle.shift()!({ didTimeout: false, timeRemaining: () => 50 });
    const touched = new Set([...before].filter(([sid, n]) => tabOf(sid).window.pendingCount < n).map(([sid]) => sid));
    console.log(`[秤3·戊] 空闲物化补到：${[...touched].sort().join(",")}`);
    expect(before.get("b"), "反空真：b 的账本得真压着历史").toBeGreaterThan(0);
    expect(touched).toEqual(new Set(["a", "b"]));
  });

  it("切进钉过水位、没满一屏、scrollHeight 被估值撑高的 tab ⇒ 按真实布局补满", () => {
    const rig = installViewerRig();
    installFakeLayout();
    vi.stubGlobal("requestIdleCallback", () => 0); // 空闲队列不跑：只看切进来那一下
    const barEl = document.createElement("div");
    const streamRootEl = document.createElement("div");
    document.body.append(barEl, streamRootEl);
    const tm = new TabManager(barEl, streamRootEl);
    const tabOf = (sid: string): Tab =>
      (tm as unknown as { store: { tabs: Map<string, Tab> } }).store.tabs.get(sid)!;
    tm.onLine(sys("act", 5000, true) as never);
    tm.onLine(sys("b", 5000, true) as never);
    tm.onBatchStart();
    for (let s = 0; s < 1200; s++) tm.onLine(sys("b", s, s % 30 === 0) as never);
    tm.onBatchEnd();
    const b = tabOf("b");
    // 估值把 scrollHeight 撑成「滚得动」（真浏览器里没渲染过的卡贡献的是估值）
    Object.defineProperty(b.streamEl, "scrollHeight", { configurable: true, get: () => 5000 });
    Object.defineProperty(b.streamEl, "clientHeight", { configurable: true, get: () => VIEW_H });
    tm.switchTo("b");
    for (let i = 0; i < 50; i++) rig.flushRaf(1);
    const px = Array.from(b.stream.contentElement.children).reduce((s, c) => s + heightOf(c), 0);
    console.log(`[秤3·戊] 切进 b 之后：真高 ${px.toFixed(0)}px · 账本余 ${b.window.pendingCount}`);
    expect(px >= VIEW_H || b.window.pendingCount === 0).toBe(true);
  });
});

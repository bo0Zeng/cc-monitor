/**
 * 秤 3 ——「一屏门控」行为读数 + 回归判据（`设计/17 §6` 表第 3 行）。
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
 * 不吃任何估值（`设计/17 §2.3` 的「门控换判据」落地的那一步，代码在 `tabs.ts`）。
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
 *    （`设计/17 §5.1`，明标分不清）。那些要秤 2（e2e 真值对照）才有答案。
 *
 * ⚠ 另一条纪律：`设计/17 §5.3` 自陈「估得准不准」今天**零读数**。本文件里的
 *    `TRUE_H_PX` 是**按 CSS token 手算**的，不是量出来的。所以下面所有「真高」
 *    都带着这个前提；秤 2 落地后要拿真值回来重校。
 *
 * 复算：`npx vitest run tests/scale3-one-screen-gate.vitest.ts`
 * 读数： `tests/evidence/A-scale3-one-screen-gate.md`
 */
import { describe, it, expect, vi, afterEach } from "vitest";
import { readdirSync, readFileSync } from "node:fs";
import { resolve } from "node:path";
import { estimateStreamNodeHeight, __resetUnknownCardWarnings } from "../src/height-estimate";
import { buildApiRetryCard } from "../src/cards/api-error";
import { buildBashInputCard, buildBashOutputCard } from "../src/cards/bash";

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
 * 修 `设计/17 §2.2` 之前的估高行为：只认 4 个 class，其余落 CSS 的 120px 兜底。
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
  /** `设计/17 §6` 秤 3 的判据：真高 ≥ clientHeight */
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
// `设计/17 §6` 数据源纪律逐字：「§6 秤 3 额外需要一个**故意构造**的 fixture:
// 150 条里塞 ≥60 条 `card-api-retry`」＋「已查证:`evidence/` 里没有现成样本
// ⇒ 这个 fixture 必须手工构造,别去那儿找」。
//
// ⚠ 手工构造的代价写在这里,不许忘:**真实的重试风暴长什么样,我们没有样本**。
// 下面的「每 150 条里几条 retry、其余几条是 skip」是**假设**,不是实测的会话形状。

/** `system` 且 `subtype !== "api_error"` → `cards/index.ts:276 return {kind:"skip"}`，占配额不产卡 */
const SKIP: Record = null;

function retryRecord(i: number): NonNullable<Record> {
  return {
    card: buildApiRetryCard({
      timeLabel: "12:34",
      retryAttempt: (i % 5) + 1,
      maxRetries: 5,
      error: { formatted: "Connection error (ECONNRESET)" },
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
    const bashIn = buildBashInputCard({ command: "npm run build" }, "2026-09-18T12:34:56Z", () => "12:34");
    const bashOut = buildBashOutputCard(
      { stdout: "line\n".repeat(5).trim(), stderr: "" },
      "2026-09-18T12:34:56Z",
      () => "12:34",
    );
    for (const el of [retry, bashIn, bashOut]) {
      expect(estimateStreamNodeHeight(el), el.className).not.toBeNull();
    }
  });

  it("两条细条常数与 CSS token 手算的真高（content-box）相差在 ±20% 内", () => {
    const retry = buildApiRetryCard({ timeLabel: "12:34", retryAttempt: 1, maxRetries: 5 });
    const bashIn = buildBashInputCard({ command: "npm run build" }, "2026-09-18T12:34:56Z", () => "12:34");
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
      //   (`tests/scale2-height-truth.vitest.ts`,真浏览器金标准,2026-09-18 已落地)。
      // ⚠ 也**不是**「落地值 vs 真高」:`applyIntrinsicSize` 的 `Math.max(24,…)` 地板会把
      //   这两个数一律顶成 24 ⇒ 真正写进 style 的仍是 24。那一格的读数在秤 2 里。
      expect(relErr, `${el.className} est=${est} hand-calc=${trueH.toFixed(1)}`).toBeLessThan(0.2);
    }
  });
});

describe("秤 3 · 卡型覆盖：没有一个卡型还在落 120px 兜底", () => {
  /**
   * 人群从**源码**派生，不是手抄一份名单 —— 手抄的名单正是 `设计/17 §2.2` 那个病
   * （加了新卡型没人回来加常数，静默落回 120px）的复发机制。
   *
   * 射程（要写清，别让人当成"全仓卡型都覆盖了"）：只扫 `src/cards/*.ts` 里
   * 形如 `"card card-xxx"` 的字面量。动态拼 class 名的、别处建的卡，**扫不到**。
   */
  // `__dirname` 在 vitest 里指向 `tests/`（同 `account-base-semantics.vitest.ts:36` 的用法）
  const CARD_SRC_DIR = resolve(__dirname, "../src/cards");
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

  it("`src/cards/` 里每个 `card card-*` 字面量都估得出高", () => {
    const classes = cardClassesFromSource();
    // 扫不到东西说明正则/住址漂了，那比"全绿"更该红
    expect(classes.length).toBeGreaterThanOrEqual(8);
    const unestimated: string[] = [];
    for (const cls of classes) {
      const el = document.createElement(DETAILS_CLASSES.has(cls) ? "details" : "div");
      el.className = `card ${cls}`;
      if (estimateStreamNodeHeight(el) === null) unestimated.push(cls);
    }
    console.log(`[秤3] src/cards/ 派生的卡型（${classes.length} 个）：${classes.join(" · ")}`);
    expect(unestimated, "这些卡型会落 styles.css 的 120px 兜底").toEqual([]);
  });
});

describe("秤 3 · 认不出的卡型会出声（`设计/17 §2.2` 修法③）", () => {
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
        "       归 `设计/17 §2.3`（门控换判据）。别拿这份读数当「半屏已修复」。",
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

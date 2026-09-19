/**
 * 秤 4「每帧账本」（`调研/设计/17-算法与复杂度.md` §6 表第 **4** 行）。
 *
 * # 它量什么
 *
 * §6 表逐字：「`computeMainBranch` 一帧调几次、每次的 N 与 ms；`rebuild()` 搬了几个节点」，
 * 验的是 §2.7 的两条：
 *  1. **3.45 ms 是 node 上的读数** —— WebView2 上可能不同（这一条本文件答不了，见末尾「没答」）
 *  2. **「97% 走快路」的实际命中率到底是多少** —— 这一条是本文件的重点。
 *
 * 🔴 那个 97% 今天是**声称**：它是从 `src/branching.ts` 头注一句「实测 1297 条记录的真实
 * jsonl：~3% parent 形成 fork」**算**出来的，不是量出来的。而且 §2.7 写的是
 * 「**97% 的记录**可走 O(1)」—— 「3% 的 **parent**」与「97% 的**记录**」换了分母，
 * 这两个数不是同一件事的两种说法。本文件把两个分母都摆出来。
 *
 * # 快路并没有装
 *
 * §2.7 档 1「脏标记快路」**今天不在代码里**：`BranchFolder.computeMain()` 每次仍然全量
 * `computeMainBranch`。所以这里量的是 `src/branch-fold.ts` 里那个**影子判定**
 * （`noteFastPathShadow`）——「如果装了档 1，这条记录会不会走 O(1)」。
 * 影子判定只读态、只写账本，**一个字节都不回流到折叠结果**。
 *
 * # 语料：**结构是构造的，内容是合成的**
 *
 * 用仓里现成的 `tests/__fixtures__/scale2-height-records.jsonl`（69 条，秤 2 产出，
 * **结构采自真机、正文全部合成**）当**内容**，但那 69 条的 `parentUuid` **一条链都不成**
 * （现打：68 个 root、0 个 fork parent —— 它是给估高用的，采样时没留拓扑）。
 * ⇒ 分叉这一层由本文件 `rewire()` **构造**：uuid / parentUuid / timestamp 全是造的。
 *
 * 🔴 **所以命中率这一档是构造的，带着「构造体像不像真的」这个前提。**
 * 它像不像真的，取决于**一件事**：真机上 ESC 回退的分布是不是「一条主链 + 均匀撒的分叉点」。
 * 本文件把分叉密度做成旋钮扫了一遍，正是因为那个密度是外来假设而不是读数
 * （`设计/17 §6` 数据源纪律 2026-09-18 改判：**不许再去读 `~/.claude/projects`**）。
 *
 * # 判据钉什么
 *
 * 钉的是**命中/未命中的条数**与**每帧算了几次**，都是可判定的整数；
 * 「快不快」只当读数打印，**一条时间断言都没有**（jsdom/node ≠ WebView2）。
 *
 * ⚠ **反空真**：命中率这个数，只走到一条路时照样算得出来、判据照样绿。
 * 所以有一格（`★ 反空真`）专门用**相等断言**钉住两条路都真的被走到了。
 *
 * 读数：`tests/evidence/S4-frame-ledger.md`（含死值验原文）
 */
import { describe, it, expect, beforeEach, afterEach, vi } from "vitest";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { BranchFolder, type BranchFrameLedger } from "../src/branch-fold";
import { computeMainBranch, extractBranchRecord, type BranchRecord } from "../src/branching";

// `__dirname` 在 vitest 里指向 `tests/`（同 `scale2-height-truth.vitest.ts` 的用法）
const FIXTURE = resolve(__dirname, "__fixtures__/scale2-height-records.jsonl");

// ===========================================================================
// 语料
// ===========================================================================

interface Payload {
  type: string;
  message?: { content?: unknown };
}

interface RawRec {
  type: string;
  uuid: string;
  parentUuid: string | null;
  timestamp: string;
  message?: { content?: unknown };
}

/** fixture 的 69 条：只取 `type` 与 `message`（拓扑不要——它那份是断的，见头注）。 */
function fixturePayloads(): Payload[] {
  return readFileSync(FIXTURE, "utf8")
    .split("\n")
    .filter((l) => l.trim().length > 0)
    .map((l) => JSON.parse(l) as Payload);
}

/** 把 fixture 的 type/内容序列循环铺到 n 条（内容对象是**共享引用**，不复制）。 */
function tiled(base: Payload[], n: number): Payload[] {
  const out: Payload[] = [];
  for (let i = 0; i < n; i++) out.push(base[i % base.length]);
  return out;
}

const BASE_MS = Date.UTC(2026, 0, 1, 0, 0, 0);

/**
 * 把一串 payload 重接成「一条主链 + 若干 ESC 回退分叉」。
 *
 * - 非分叉点：`parent = 前一条` ⇒ 单链（真机上绝大多数记录就是这个形状）
 * - 分叉点 i：`parent = 第 i-1-back 条` ⇒ 那条已经有 child 了，于是成 fork；
 *   而 i 的 timestamp 最大 ⇒ `computeMainBranch` 选它当赢家，
 *   被甩掉的 `i-back … i-1` 共 **back 条**转 off-main（ESC 回退的形状）
 *
 * 🔴 uuid / parentUuid / timestamp **全是本函数造的**。
 */
function rewire(payloads: Payload[], forkAt: ReadonlySet<number>, back: number): RawRec[] {
  const out: RawRec[] = [];
  for (let i = 0; i < payloads.length; i++) {
    const step = forkAt.has(i) ? back : 0;
    const parentIdx = i - 1 - step;
    out.push({
      type: payloads[i].type,
      uuid: `s4-${String(i).padStart(6, "0")}`,
      parentUuid: parentIdx >= 0 ? out[parentIdx].uuid : null,
      timestamp: new Date(BASE_MS + i * 1000).toISOString(),
      message: payloads[i].message,
    });
  }
  return out;
}

/** 走生产的抽取器（`extractBranchRecord`），不手搭 `BranchRecord`。 */
function toBranchRecords(raw: RawRec[]): BranchRecord[] {
  return raw.map((r, i) => {
    const br = extractBranchRecord(r);
    if (!br) throw new Error(`第 ${i} 条被 extractBranchRecord 吞了（type=${r.type}）——语料坏了`);
    return br;
  });
}

/**
 * 均匀摆 `count` 个分叉点。间距不足以容下一个 `back` 长的旧分支时直接抛 ——
 * 悄悄摆密了会让「分叉密度」这个旋钮名不副实。
 */
function forkIndices(n: number, count: number, back: number): Set<number> {
  const out = new Set<number>();
  if (count === 0) return out;
  const step = Math.floor(n / (count + 1));
  if (step < back + 3) {
    throw new Error(`n=${n} 摆不下 ${count} 个间距 ≥${back + 3} 的分叉点（算出来 step=${step}）`);
  }
  for (let k = 1; k <= count; k++) out.add(k * step);
  return out;
}

/**
 * 「~R% 的 parent 成 fork」要摆几个分叉点。
 *
 * n 条记录、F 个分叉 ⇒ 边 n-1 条、**有 child 的 parent** 共 n-1-F 个、其中 F 个有两个 child
 * ⇒ `F / (n-1-F) = R`。这就是 `branching.ts` 头注那个 3% 的分母。
 */
function forksForParentRate(n: number, rate: number): number {
  return Math.round((rate * (n - 1)) / (1 + rate));
}

// ===========================================================================
// 账本 / 帧 / 挂载
// ===========================================================================

interface PerfBag {
  domContentLoaded: number;
  branchLedgerVerify?: boolean;
  branchLedger?: BranchFrameLedger;
}

function installPerf(verify: boolean): void {
  (globalThis as unknown as { __ccmPerf?: PerfBag }).__ccmPerf = {
    domContentLoaded: 0,
    branchLedgerVerify: verify,
  };
}

/** 取账本。**取不到就抛** —— 仪表没装上时下面每一个数都是空的，绝不能静默绿。 */
function ledger(): BranchFrameLedger {
  const bag = (globalThis as unknown as { __ccmPerf?: PerfBag }).__ccmPerf;
  const led = bag?.branchLedger;
  if (!led) {
    throw new Error(
      "window.__ccmPerf.branchLedger 不存在 —— 秤 4 的仪表没装上（或 branch-fold.ts 的 branchLedger() " +
        "拿不到 __ccmPerf）。这种情况下命中率、每帧次数全是 0，判据必须在这里炸而不是往下走。",
    );
  }
  return led;
}

let rafQueue: FrameRequestCallback[] = [];

/** 放行**一帧**：只跑这一刻已经排好的回调；回调里再排的算下一帧。 */
function flushFrame(): number {
  const q = rafQueue.splice(0, rafQueue.length);
  for (const cb of q) cb(0);
  return q.length;
}

function mount(): { el: HTMLElement; folder: BranchFolder } {
  const el = document.createElement("div");
  document.body.replaceChildren(el);
  return { el, folder: new BranchFolder(el) };
}

function appendCard(el: HTMLElement, uuid: string): void {
  const card = document.createElement("div");
  card.className = "card";
  card.dataset.uuid = uuid;
  el.append(card);
}

/** 每条一帧：一条 `recordAdded` 放行一帧 ⇒ 主线集合每条都是新的（**纯谓词**那一档）。 */
function driveOnePerFrame(folder: BranchFolder, recs: BranchRecord[], el?: HTMLElement): void {
  for (const r of recs) {
    if (el) appendCard(el, r.uuid);
    folder.recordAdded(r);
    flushFrame();
  }
}

/** k 条一帧：模拟 live 合批（F15）——主线集合一帧才刷一次。 */
function drivePerFrame(
  folder: BranchFolder,
  recs: BranchRecord[],
  k: number,
  el?: HTMLElement,
): void {
  for (let i = 0; i < recs.length; i++) {
    if (el) appendCard(el, recs[i].uuid);
    folder.recordAdded(recs[i]);
    if ((i + 1) % k === 0) flushFrame();
  }
  flushFrame(); // 收尾那半帧（正好整除时这里是空跑）
}

/**
 * 「k 条一帧」下**预期未命中的记录下标**。
 *
 * 🔴 **只由语料参数算**（条数 / 每帧条数 / 分叉位置），不重跑任何谓词 ——
 * 重跑谓词就是把判据写成仪表的副本，那样仪表错了判据也跟着错。
 *
 * 两条规则，都是档 1 的定义直接推出来的：
 *  - **第 0 帧整帧不命中**：这之前一次真算都没有过 ⇒ 主线集合是空的，
 *    连第 1 条（parent = 第 0 条）都认不出来。
 *  - **一帧里踩到分叉之后，这一帧剩下的全不命中**：分叉那条自己没进影子主线，
 *    于是下一条的 parent 不在主线里，依次传染到帧末；帧末真算一次才复位。
 */
function missesUnderFrameBatching(n: number, perFrame: number, forks: ReadonlySet<number>): number {
  let miss = 0;
  for (let start = 0; start < n; start += perFrame) {
    const end = Math.min(n, start + perFrame);
    let stale = start === 0;
    for (let i = start; i < end; i++) {
      if (forks.has(i)) stale = true;
      if (stale) miss++;
    }
  }
  return miss;
}

/** 同上口径：有几帧**至少含一条未命中** ⇒ 档 1 跳不掉那一帧的 O(N)。 */
function unskippableFrames(n: number, perFrame: number, forks: ReadonlySet<number>): number {
  let frames = 0;
  for (let start = 0; start < n; start += perFrame) {
    const end = Math.min(n, start + perFrame);
    let stale = start === 0;
    for (let i = start; i < end; i++) if (forks.has(i)) stale = true;
    if (stale) frames++;
  }
  return frames;
}

function pct(a: number, b: number): string {
  return b === 0 ? "n/a" : `${((100 * a) / b).toFixed(2)}%`;
}

function median(xs: number[]): number {
  const s = [...xs].sort((a, b) => a - b);
  return s[Math.floor(s.length / 2)];
}

// ===========================================================================

const BACK = 4; // 一个分叉甩掉的旧分支长度（= off-main 卡片数/分叉）
const FIXTURE_FORKS = new Set([20, 40, 60]);

describe("秤 4 每帧账本", () => {
  beforeEach(() => {
    rafQueue = [];
    vi.stubGlobal("requestAnimationFrame", (cb: FrameRequestCallback) => {
      rafQueue.push(cb);
      return rafQueue.length;
    });
  });
  afterEach(() => {
    vi.unstubAllGlobals();
    delete (globalThis as unknown as { __ccmPerf?: PerfBag }).__ccmPerf;
  });

  // -------------------------------------------------------------------------

  it("★ 反空真：快路与慢路都真的被走到了（两边都用相等断言钉死）", () => {
    installPerf(true);
    const payloads = fixturePayloads();
    const recs = toBranchRecords(rewire(payloads, FIXTURE_FORKS, BACK));
    const N = recs.length;
    const F = FIXTURE_FORKS.size;

    // ① 语料自检：分叉**真的造出来了**，而且是 computeMainBranch 自己认的分叉，
    //    不是我说它是。off-main 条数 = 分叉数 × 旧分支长度。
    expect(N, "fixture 该是 69 条").toBe(69);
    expect(F).toBe(3);
    const main = computeMainBranch(recs);
    expect(
      N - main.size,
      "构造的分叉没被 computeMainBranch 认出来 —— 语料退化成一条直链了，" +
        "那样「慢路」一次都不会走到，下面的命中率是空真的",
    ).toBe(F * BACK);

    const { el, folder } = mount();
    driveOnePerFrame(folder, recs, el);
    const led = ledger();

    // ② 两条路的条数：**都是相等断言**，任意一边变 0 立刻红
    expect(led.fastPathHit, "快路命中数").toBe(N - 1 - F);
    expect(led.fastPathMiss, "慢路（未命中）数").toBe(F + 1);
    expect(
      [led.fastPathHit > 0, led.fastPathMiss > 0],
      "只走到一条路时命中率没有意义，而判据会照样绿 —— 这一格就是为这件事存在的",
    ).toEqual([true, true]);

    // ③ 慢路里那 F 条**确实是 fork 那一支**（不是被别的原因顺手挡下来的）
    expect(led.fastPathMissBy).toEqual({
      noParent: 1, // 第 0 条是 root
      parentUnknown: 0,
      parentHasChild: F, // 三个分叉点
      parentOffMain: 0, // 每条一帧 ⇒ 主线永远是刚算的，不会踩到这一条
    });

    // ④ 慢路真的**干了活**：DOM 上折出了 F 个 wrap、共 F×BACK 张卡
    const wraps = el.querySelectorAll(":scope > .branch-fold-wrap");
    expect(wraps.length, "折叠容器数").toBe(F);
    expect(el.querySelectorAll(".branch-fold-body-inner > [data-uuid]").length).toBe(F * BACK);
  });

  // -------------------------------------------------------------------------

  it("★ 每条一帧（纯谓词）：97% 那句话的实测命中率 + 快路算得对不对", () => {
    installPerf(true);
    const payloads = fixturePayloads();
    const recs = toBranchRecords(rewire(payloads, FIXTURE_FORKS, BACK));
    const N = recs.length;
    const F = FIXTURE_FORKS.size;

    const { el, folder } = mount();
    driveOnePerFrame(folder, recs, el);
    const led = ledger();

    expect(led.recordsAdded).toBe(N);
    expect(led.fastPathHit + led.fastPathMiss).toBe(N);
    expect(led.computes, "每条一帧 ⇒ 每条算一次").toBe(N);
    expect(led.frames).toBe(N);
    expect(led.computeSamples.length).toBe(N);
    expect(new Set(led.computeSamples.map((s) => s.frame)).size, "一帧只算一次").toBe(N);
    expect(new Set(led.computeSamples.map((s) => s.via))).toEqual(new Set(["live-frame"]));
    expect(led.computeSamples.map((s) => s.n)).toEqual(recs.map((_, i) => i + 1));

    // 档 1 能跳掉的那些次 = 全命中的那些次
    expect(led.computesSkippable).toBe(N - 1 - F);
    expect(led.computesUnskippable).toBe(F + 1);
    expect(led.computesNoArrivals).toBe(0);

    // 🔴 快路**算得对不对**：全命中那些次上，影子算出来的主线与真算逐元素相等。
    //    「快而不对」比慢坏得多，这两行专门盯它。
    expect(led.verify, "verify 档没开 ⇒ 下面两行是空真").toBe(true);
    expect(led.fastPathVerified).toBe(N - 1 - F);
    expect(led.fastPathWrong, "影子快路算出来的主线与真算不符").toBe(0);

    const parentsWithChild = N - 1 - F;
    console.log(
      [
        "",
        "=== 秤 4 · A 纯谓词命中率（每条一帧；语料 = fixture 69 条内容 + 构造拓扑）===",
        `记录数 N=${N}，分叉点 F=${F}，每个分叉甩掉 ${BACK} 条`,
        `分母①「分叉记录 / 全部记录」= ${F}/${N} = ${pct(F, N)}`,
        `分母②「fork parent / 有 child 的 parent」= ${F}/${parentsWithChild} = ${pct(F, parentsWithChild)}`,
        `  ↑ branching.ts 头注那个「~3% parent 成 fork」用的是分母②；`,
        `    §2.7 写的「97% 的**记录**可走 O(1)」用的是分母①。两者不是同一个数。`,
        `**快路命中率 = ${led.fastPathHit}/${N} = ${pct(led.fastPathHit, N)}**`,
        `未命中分布：${JSON.stringify(led.fastPathMissBy)}`,
        `帧级：能整帧跳掉 O(N) 的次数 ${led.computesSkippable}/${led.computes} = ${pct(
          led.computesSkippable,
          led.computes,
        )}`,
        `快路正确性（verify 档）：相等 ${led.fastPathVerified} 次 / 不等 ${led.fastPathWrong} 次`,
        `rebuild：${led.rebuilds} 次，累计搬动 ${led.rebuildNodesMoved} 个节点`,
        `最终 DOM：${el.querySelectorAll(":scope > .branch-fold-wrap").length} 个 fold-wrap`,
      ].join("\n"),
    );
  });

  // -------------------------------------------------------------------------

  it("★ 分叉密度旋钮：命中率随密度怎么走（3% 那一档是对照 §2.7 的）", () => {
    const n = 600;
    const back = 2; // 密档要摆得下，旧分支短一点；密度只由分叉**个数**决定
    const rows: string[] = [];
    let checked = 0;

    for (const rate of [0, 0.01, 0.03, 0.1, 0.2]) {
      installPerf(false);
      const F = forksForParentRate(n, rate);
      const forks = forkIndices(n, F, back);
      expect(forks.size, `rate=${rate} 的分叉点没摆够`).toBe(F);
      const recs = toBranchRecords(rewire(tiled(fixturePayloads(), n), forks, back));

      const { folder } = mount(); // 这一档不建卡：量的是谓词，不是 DOM
      driveOnePerFrame(folder, recs);
      const led = ledger();

      expect(led.fastPathHit + led.fastPathMiss, `rate=${rate}`).toBe(n);
      expect(led.fastPathMiss, `rate=${rate}：未命中该正好是「1 个 root + F 个分叉」`).toBe(F + 1);
      expect(led.fastPathMissBy.parentHasChild, `rate=${rate}`).toBe(F);
      checked++;

      const parentsWithChild = n - 1 - F;
      rows.push(
        [
          String(Math.round(rate * 100)).padStart(6),
          String(F).padStart(6),
          pct(F, parentsWithChild).padStart(9),
          pct(F, n).padStart(9),
          pct(led.fastPathHit, n).padStart(9),
          pct(led.computesSkippable, led.computes).padStart(11),
        ].join(" | "),
      );
    }
    expect(checked, "五档一档都没跑成").toBe(5);

    console.log(
      [
        "",
        `=== 秤 4 · B 分叉密度 → 命中率（每条一帧，n=${n}，旧分支长 ${back}）===`,
        "旋钮% | 分叉数 | 实得parent% | 分叉/记录 | 快路命中率 | 可跳帧占比",
        ...rows,
        "⚠ 「旋钮%」是按 F/(n-1-F) 反解的目标 parent 分叉率；「实得parent%」是取整后的真值。",
      ].join("\n"),
    );
  });

  // -------------------------------------------------------------------------

  it("★ live 合批（k 条一帧）：脏标记快路被帧合批吃掉多少", () => {
    const n = 600;
    const F = forksForParentRate(n, 0.03); // 对齐 branching.ts 头注那个 ~3%
    const forks = forkIndices(n, F, BACK);
    const rows: string[] = [];
    let checked = 0;

    for (const k of [1, 2, 4, 8, 16]) {
      installPerf(false);
      const recs = toBranchRecords(rewire(tiled(fixturePayloads(), n), forks, BACK));
      const { folder } = mount();
      drivePerFrame(folder, recs, k);
      const led = ledger();

      const expFrames = Math.ceil(n / k);
      expect(led.frames, `k=${k}：帧数`).toBe(expFrames);
      expect(led.computes, `k=${k}：F15 合批 —— 一帧只算一次`).toBe(expFrames);
      expect(led.computeSamples.length).toBe(expFrames);
      expect(new Set(led.computeSamples.map((s) => s.frame)).size).toBe(expFrames);

      // 未命中数**由语料参数算**（见 missesUnderFrameBatching 头注），不是重跑谓词
      expect(led.fastPathHit + led.fastPathMiss).toBe(n);
      expect(led.fastPathMiss, `k=${k}：合批下的未命中数与模型不符`).toBe(
        missesUnderFrameBatching(n, k, forks),
      );
      expect(led.computesUnskippable, `k=${k}`).toBe(unskippableFrames(n, k, forks));
      expect(led.computesSkippable, `k=${k}`).toBe(expFrames - unskippableFrames(n, k, forks));
      expect(led.computesNoArrivals).toBe(0);
      // 反空真：每一档都必须两条路都有
      expect([led.fastPathHit > 0, led.fastPathMiss > 0], `k=${k}`).toEqual([true, true]);
      checked++;

      rows.push(
        [
          String(k).padStart(5),
          String(led.frames).padStart(5),
          pct(led.fastPathHit, n).padStart(9),
          String(led.fastPathMissBy.parentHasChild).padStart(7),
          String(led.fastPathMissBy.parentOffMain).padStart(9),
          pct(led.computesSkippable, led.computes).padStart(10),
        ].join(" | "),
      );
    }
    expect(checked, "五档一档都没跑成").toBe(5);

    console.log(
      [
        "",
        `=== 秤 4 · C live 合批吃掉多少（n=${n}，${F} 个分叉 ≈ 3% parent 分叉率）===`,
        "条/帧 |  帧数 | 快路命中率 | 分叉miss | 合批miss | 可跳帧占比",
        ...rows,
        "「分叉miss」= 真正的 fork 点（档 1 无论如何都跳不掉的那些）",
        "「合批miss」= 纯粹因为主线集合一帧才刷一次而被连累的记录（`parentOffMain`）",
        "⚠ 帧合批（F15）已经在生产里了 ⇒ 每帧的 O(N) 本来就只有一次。",
        "   ⇒ 档 1 真正能省的是「可跳帧占比」那一列，不是「命中率」那一列。",
      ].join("\n"),
    );
  });

  // -------------------------------------------------------------------------

  it("★ 重放（batch 模式）：快路命中率 0%，而那里本来就只算一次", () => {
    installPerf(false);
    const payloads = fixturePayloads();
    const recs = toBranchRecords(rewire(payloads, FIXTURE_FORKS, BACK));
    const N = recs.length;
    const F = FIXTURE_FORKS.size;

    const { folder } = mount();
    folder.setBatchMode(true);
    for (const r of recs) folder.recordAdded(r);
    expect(flushFrame(), "batch 模式不许排 rAF").toBe(0);
    folder.flushPending();
    const led = ledger();

    expect(led.computes, "整批只算一次").toBe(1);
    expect(led.computeSamples[0].via).toBe("flush");
    expect(led.computeSamples[0].n).toBe(N);
    expect(led.fastPathHit, "重放期主线集合始终是空的 ⇒ 一条都命中不了").toBe(0);
    expect(led.fastPathMiss).toBe(N);
    expect(led.fastPathMissBy).toEqual({
      noParent: 1,
      parentUnknown: 0,
      parentHasChild: F,
      parentOffMain: N - 1 - F,
    });
    expect(led.computesUnskippable).toBe(1);
    expect(led.computesSkippable).toBe(0);

    console.log(
      [
        "",
        "=== 秤 4 · D 重放（batch）===",
        `${N} 条整批灌入 ⇒ computeMainBranch 调 ${led.computes} 次（N=${led.computeSamples[0].n}）`,
        `快路命中率 ${pct(led.fastPathHit, N)} —— 档 1 对启动重放**一点用都没有**，`,
        "因为那条路上 O(N) 本来就只跑一次（F15 的 batch 模式已经把它合掉了）。",
      ].join("\n"),
    );
  });

  // -------------------------------------------------------------------------

  it("★ rebuild 搬了几个节点（DOM 侧，相等断言钉最后一次）", () => {
    installPerf(false);
    const payloads = fixturePayloads();
    const recs = toBranchRecords(rewire(payloads, FIXTURE_FORKS, BACK));
    const N = recs.length;
    const F = FIXTURE_FORKS.size;

    const { el, folder } = mount();
    driveOnePerFrame(folder, recs, el);
    const led = ledger();

    expect(led.rebuilds, "每条都改了主线 ⇒ 每条都重折一次").toBe(N);
    expect(led.rebuildSamples.length).toBe(N);
    const last = led.rebuildSamples[led.rebuildSamples.length - 1];
    expect(
      { ...last, frame: 0 },
      "最后一次 rebuild：扫 69 个顶层子节点，解开上一轮 3 个 wrap 里的 12 张卡，再折回 3 个 wrap、12 张卡",
    ).toEqual({
      scanned: N,
      unwrapped: F * BACK,
      unwrappedWraps: F,
      wrapped: F * BACK,
      wraps: F,
      frame: 0,
    });
    expect(led.standaloneUnwraps, "本路径不走裸 unwrapAll()").toBe(0);

    const moves = led.rebuildSamples.map((s) => s.unwrapped + s.wrapped);
    console.log(
      [
        "",
        "=== 秤 4 · E rebuild 搬动节点数（每条一帧，69 条 / 3 个分叉）===",
        `rebuild ${led.rebuilds} 次，累计搬动 ${led.rebuildNodesMoved} 个节点` +
          `（均 ${(led.rebuildNodesMoved / led.rebuilds).toFixed(2)}/次，max ${Math.max(...moves)}）`,
        `末次：扫 ${last.scanned} 个顶层节点，解开 ${last.unwrappedWraps} 个 wrap（${last.unwrapped} 张卡），` +
          `重折 ${last.wraps} 个 wrap（${last.wrapped} 张卡）`,
        "⚠ 「搬动数」只数 move 的次数，不含浏览器为此付的布局/重绘 —— 那个 jsdom 里量不到。",
      ].join("\n"),
    );
  });

  // -------------------------------------------------------------------------

  it("computeMainBranch 的 N → ms（node/jsdom 读数，**一条时间断言都没有**）", () => {
    const REPS = 5;
    const rows: string[] = [];
    for (const n of [181, 1575, 4566]) {
      installPerf(false);
      const F = forksForParentRate(n, 0.03);
      const recs = toBranchRecords(
        rewire(tiled(fixturePayloads(), n), forkIndices(n, F, BACK), BACK),
      );

      const { folder } = mount(); // 容器空 ⇒ rebuild 不搬节点，夹到的就是纯算的时间
      folder.setBatchMode(true);
      for (const r of recs) folder.recordAdded(r);
      folder.setBatchMode(false);
      for (let i = 0; i < REPS; i++) folder.flushPending();

      const led = ledger();
      expect(led.computes, `n=${n}：该算 ${REPS} 次`).toBe(REPS);
      expect(new Set(led.computeSamples.map((s) => s.n)), `n=${n}`).toEqual(new Set([n]));
      const ms = led.computeSamples.map((s) => s.ms);
      rows.push(
        [
          String(n).padStart(6),
          String(F).padStart(5),
          median(ms).toFixed(3).padStart(8),
          Math.min(...ms)
            .toFixed(3)
            .padStart(8),
          Math.max(...ms)
            .toFixed(3)
            .padStart(8),
        ].join(" | "),
      );
    }
    console.log(
      [
        "",
        `=== 秤 4 · F computeMainBranch 的 N → ms（node + jsdom，${REPS} 次）===`,
        "     N | 分叉 |   中位ms |    min |    max",
        ...rows,
        "对照 §2.7 现打（同为 node）：N=181 → 0.24ms · N=1575 → 1.33ms · N=4566 → 3.45ms",
        "🔴 这一列**不是 WebView2 的读数**。仪表在生产代码里，真机跑一次就有同一列数；本轮没有真机。",
      ].join("\n"),
    );
  });
});

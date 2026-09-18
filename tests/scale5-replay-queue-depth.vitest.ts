/**
 * 「秤 5：启动重放队列深度」——`调研/设计/17-算法与复杂度.md` §6 表第 5 行。
 *
 * # 它要量什么
 *
 * §2.10 说 `events.ts` 的 drain 循环用 `Array.prototype.shift()` 当队列出队，
 * 并给了两个现打数：n=4566 → 0.83ms、**n=20000 → 15.64ms（290×）**。
 * 那两个数**只有在队列真能长到那个量级时才值得担心**，所以秤 5 量的是
 * 「启动重放时 `queue.length` 的峰值」——它决定 §2.10 落在「可忽略」还是「不可忽略」。
 *
 * # 为什么不照文档给的装表法
 *
 * §6 表里写的装表法是「`events.ts:368` 与 `:402` 后加一行 `perf.queueMax = Math.max(...)`」。
 * **本文件一行 `src/` 都不改**（那份文件归别人），改走**纯外部观测**。
 *
 * # 外部怎么量得到一个闭包私有的 `queue`
 *
 * `bindEvents` 里的 `const queue: QueueItem[] = []` 外面拿不到，但队列是 FIFO 且
 * **两端都在本文件手里**：
 *
 * - **入队端**：喂事件的人就是本文件 ⇒ 每一条 push 进去的是什么 kind、什么时候 push 的，逐条可知。
 * - **出队端**：`bindEvents` 收的 `EventHandlers` 全是本文件的假 handler ⇒
 *   `onLine` 被调了几次 = **payload 条目**出队了几条。
 *
 * ⇒ `queue.length(t) = 已 push(t) − 已 dispatch(t)`。
 *
 * # 哨兵这一半：诚实边界（不编）
 *
 * queue 里除 payload 外还有 `batch-start` / `batch-end` 两种哨兵，它们**不走 payload handler**：
 *
 * - `batch-start` **可观测**：`enterBatchMode` 会调 `onBatchStart`。本文件的每条流里
 *   `batch-start` 只有**一个**（只在 `chunkIndex===0` 时 push），所以
 *   「`onBatchStart` 被调过」⇔「那一个 batch-start 已出队」，是**精确**的。
 * - `batch-end` **不可直接观测**：它出队时只调 `scheduleBatchEnd()`（排一个 300ms 定时器），
 *   没有任何 handler 被调。
 *
 * 但**这笔账的误差有硬上界**：本文件喂的流里**任意两个哨兵都不相邻**
 * （chunk0 = `[batch-start, payload×k, batch-end]`；chunk i>0 = `[payload×k, batch-end]`，
 * 每块 k ≥ 1）⇒ 由 `onLine` 次数反演出的「已 dispatch 条目数」是一个**宽度 ≤ 1 的区间**。
 * 所以每个深度读数都是 `[lo, hi]` 且 `hi − lo ≤ 1`，而不是一个编出来的确数
 * ——`assertsBracketTight` 那条判据就钉这个宽度，宽度一破（有人让哨兵相邻了）它先红。
 *
 * # 让出机制：本文件**把 `drain` 的让出逼到 `setTimeout` 兜底那一支**
 *
 * `events.ts` 的 `makeYieldToMain` 会**特性探测** `MessageChannel`：探到就用它让出
 * （宏任务，且规范没给它嵌套 timer 那条 4ms 钳制），探不到才退回 `setTimeout(run, 0)`。
 * jsdom **有** `MessageChannel`，而假定时器**驱动不了它** ⇒ 不干预的话 `runAllTimersAsync`
 * 一条都排不出来。所以本文件在 `bindEvents` **之前** `vi.stubGlobal("MessageChannel", undefined)`，
 * 把它逼回那条**生产上真实存在**的兜底路（探测失败的环境走的就是这条）。
 *
 * ⚠ **这件事改变了读数的适用范围，必须跟着读数一起说**：本文件量到的是
 * **`setTimeout` 兜底那条路**上的队列深度。`MessageChannel` 那条路没有 4ms 钳制，
 * 让出更密 ⇒ 同样输入下峰值只会**更低**，低多少取决于真 handler 的每条成本
 * ——**那个本文件没测**（这里的 handler 是 `vi.fn()` 空壳）。
 *
 * # 它**不**是什么（§5「分不清」纪律）
 *
 * 量出来的是**队列深度**，不是「重放到底卡不卡」。深度 → 耗时那一跳还要过
 * 真机 WebView2 的 V8、真 payload 的大小、真 handler 的成本，这里一个都没有。
 * 读数 `tests/evidence/A-scale5-replay-queue-depth.md` 里「没答什么」那一节写全了。
 *
 * ⚠ 本文件**不引 `src/events.ts` 的行号**：它正在被另一路 agent 改，行号一天一个样
 * （§6 表里那两个 `:368` / `:402` 今天已经指不到东西了）。引的一律是**构造名**。
 */
import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";

type Cb = (e: { payload: unknown }) => void;
const subs = new Map<string, Cb>();

vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn((event: string, cb: Cb) => {
    subs.set(event, cb);
    return Promise.resolve(() => {});
  }),
}));
vi.mock("@tauri-apps/api/webviewWindow", () => ({
  getCurrentWebviewWindow: () => ({
    // `emitPerfSummary` 会读 `.label`，给个值省得读出 undefined 混进日志。
    label: "main",
    listen: vi.fn((event: string, cb: Cb) => {
      subs.set(event, cb);
      return Promise.resolve(() => {});
    }),
  }),
}));
// ⚠ 这里**返回 Promise**（不是 `undefined`）：`emitPerfSummary` 末尾那一跳会对返回值调
// `.catch`，桩返回 undefined 会走「出声」那条岔路，把本文件的 stdout 灌满无关 warn。
// 那条岔路本身由 `events-burst.vitest.ts` 钉着，本文件不重复钉。
vi.mock("../src/ipc/commands", () => ({
  commands: new Proxy({}, { get: () => vi.fn(() => Promise.resolve()) }),
}));

import { bindEvents } from "../src/events";

// ───────────────────────── 镜像常量（下面第 3 条判据机检它们没漂）─────────────────────────

/** `src/events.ts` 的 `BATCH_SIZE`：问不到 `isInputPending` 时，每个 drain tick 至多处理这么多条。 */
const FRONT_BATCH_SIZE = 40;
/** `src/bridge/src/event_replay.rs:48`：后端 replay 的切块大小。 */
const BACKEND_CHUNK_SIZE = 600;
/** `src/bridge/src/event_replay.rs:47`：低于此条数后端**不切块**，单次 emit。 */
const BACKEND_SINGLE_CHUNK_THRESHOLD = 200;
/** `src/bridge/src/event_replay.rs:50`：块与块之间后端 sleep 这么久。 */
const BACKEND_CHUNK_PAUSE_MS = 10;

// ───────────────────────── 队列深度探针 ─────────────────────────

type Kind = "payload" | "batch-start" | "batch-end";

/**
 * 入队端的账本 + 由 `onLine` 次数反演深度。
 *
 * 这**不是**对 `events.ts` 的仿真——它只记「本文件让 `events.ts` push 了什么」，
 * 出队那一半完全来自 `events.ts` 真的调了几次 handler。
 */
class QueueDepthProbe {
  private kinds: Kind[] = [];
  /** `payloadPos[j]` = 第 j+1 条 payload 在 `kinds` 里的下标。 */
  private payloadPos: number[] = [];

  push(k: Kind): void {
    if (k === "payload") this.payloadPos.push(this.kinds.length);
    this.kinds.push(k);
  }

  get pushed(): number {
    return this.kinds.length;
  }

  get payloadsPushed(): number {
    return this.payloadPos.length;
  }

  /**
   * 由「已 dispatch 的 payload 条数」反演「已 dispatch 的**条目**数」。
   *
   * 合法的 d 满足「`kinds[0..d)` 里恰有 `onLineCount` 条 payload」，这是一个连续区间：
   * 下界 = 第 `onLineCount` 条 payload 之后一位；上界 = 下一条 payload 所在下标。
   * 区间宽度 = 边界处**连续哨兵的条数**——本文件的流里恒为 1。
   */
  private dispatchedBracket(onLineCount: number, batchStartDispatched: boolean): [number, number] {
    let lo = onLineCount === 0 ? 0 : this.payloadPos[onLineCount - 1] + 1;
    const hi =
      onLineCount === this.payloadPos.length
        ? this.kinds.length
        : this.payloadPos[onLineCount];
    // 那个唯一的 batch-start 已经出队了（`onBatchStart` 被调过）⇒ d ≥ 1。
    if (batchStartDispatched && lo < 1) lo = 1;
    return [lo, hi];
  }

  /** 此刻 `queue.length` 的 `[下界, 上界]`（上界 = 没把边界那个哨兵算出队）。 */
  depthBracket(onLineCount: number, batchStartDispatched: boolean): [number, number] {
    const [dLo, dHi] = this.dispatchedBracket(onLineCount, batchStartDispatched);
    return [this.pushed - dHi, this.pushed - dLo];
  }
}

function makePayload(seq: number): unknown {
  return {
    session_id: "s",
    cwd: "/p",
    path: "/p/s.jsonl",
    seq,
    message: { type: "assistant", uuid: `u-${seq}` },
  };
}

/**
 * 复刻 `src/bridge/src/event_replay.rs` 的 `build_chunks`：**从尾往前切**，所以
 * `chunks[0..n-2]` 各 `CHUNK_SIZE` 条，**最后一块**才是余数（可能小于 CHUNK_SIZE）。
 * 另按 `replay_and_mark_ready` 的 `n < SINGLE_CHUNK_THRESHOLD` 那一支：低于阈值后端根本不切块，单块 emit。
 */
function buildChunkSizes(total: number): number[] {
  if (total < BACKEND_SINGLE_CHUNK_THRESHOLD) return [total];
  const sizes: number[] = [];
  let end = total;
  while (end > 0) {
    const start = Math.max(0, end - BACKEND_CHUNK_SIZE);
    sizes.push(end - start);
    end = start;
  }
  return sizes;
}

interface Sample {
  at: string;
  lo: number;
  hi: number;
}

interface ReplayResult {
  total: number;
  chunks: number;
  ticksPerGap: number;
  samples: Sample[];
  pushSamples: Sample[];
  peak: Sample;
  finalLo: number;
  finalHi: number;
  onLineCount: number;
  onBatchStartCount: number;
  onBatchEndCount: number;
  maxBracketWidth: number;
}

/**
 * 跑一次「启动重放」。
 *
 * `ticksPerGap` = **模型参数，不是测量值**：后端每块之间 sleep `CHUNK_PAUSE_MS=10`ms，
 * 那 10ms 里前端能跑几个 `setTimeout(drain, 0)` tick，取决于真机上 0 延时定时器被钳到多少
 * （浏览器嵌套深度 > 5 后通常钳到 4ms）。这里把它**摆成旋钮**而不是假装量到了：
 *   - 0  → 后端把所有块推完前端一个 slot 都没拿到（上界档）
 *   - 2  → 按 4ms 钳位折算
 *   - 10 → 按 1ms 钳位折算（乐观档）
 */
async function runReplay(total: number, ticksPerGap: number): Promise<ReplayResult> {
  subs.clear();
  const onLine = vi.fn();
  const onBatchStart = vi.fn();
  const onBatchEnd = vi.fn();
  await bindEvents({
    onLine,
    onSessionEnded: vi.fn(),
    onBatchStart,
    onBatchEnd,
  } as never);

  const cb = subs.get("jsonl-batch");
  // 抽取器自检：没订上就什么都没测。
  expect(cb, "没订到 jsonl-batch —— 本条会零命中地绿（检查 listen 的 mock）").toBeTruthy();

  const probe = new QueueDepthProbe();
  const samples: Sample[] = [];
  const pushSamples: Sample[] = [];
  const observe = (at: string, into?: Sample[]): Sample => {
    const [lo, hi] = probe.depthBracket(
      onLine.mock.calls.length,
      onBatchStart.mock.calls.length > 0,
    );
    const s = { at, lo, hi };
    samples.push(s);
    into?.push(s);
    return s;
  };

  const sizes = buildChunkSizes(total);
  let seq = 0;
  for (let i = 0; i < sizes.length; i++) {
    const size = sizes[i];
    const payloads: unknown[] = [];
    for (let j = 0; j < size; j++) payloads.push(makePayload(seq + j));
    seq += size;
    // 账本按 `events.ts` 的 `jsonl-batch` 订阅里真实的 push 顺序记：chunkIndex===0 时先一个 batch-start，
    // 然后 N 个 payload，最后一个 batch-end。
    if (i === 0) probe.push("batch-start");
    for (let j = 0; j < size; j++) probe.push("payload");
    probe.push("batch-end");
    cb!({ payload: { chunkIndex: i, chunkTotal: sizes.length, payloads } });

    if (i === 0) {
      // 抽取器自检：第一块推完后，`ensureScheduled` 必须已经把 drain 排在一个**假定时器**上。
      // 这条一红 = `events.ts` 的让出机制不再走 `setTimeout` 兜底了（比如探测换了写法、
      // 或 MessageChannel 那支不再可被 stub 绕开）⇒ **本文件的驱动方式要重做**，
      // 而不是「判据坏了」。没有这一格，那种情况会退化成一条指不出原因的 onLine=0。
      expect(
        vi.getTimerCount(),
        "★ 推完第一块后一个假定时器都没有 —— `events.ts` 的 drain 让出没走 `setTimeout` 兜底。\n" +
          "本文件靠 `vi.stubGlobal(\"MessageChannel\", undefined)` 把 `makeYieldToMain` 逼到那一支；\n" +
          "它一旦失效，队列就完全排不出来（onLine 恒 0），读数全部作废。",
      ).toBeGreaterThan(0);
    }
    observe(`推完第 ${i} 块`, pushSamples);
    for (let t = 0; t < ticksPerGap; t++) {
      if (onLine.mock.calls.length === probe.payloadsPushed) break; // 队列已空，别去踩 300ms 那个定时器
      await vi.advanceTimersToNextTimerAsync();
      observe(`第 ${i} 块后第 ${t + 1} 个 drain tick`);
    }
  }

  // 全部跑完：drain 链 + 那个 300ms grace 定时器。
  await vi.runAllTimersAsync();
  const last = observe("全部 drain 完");

  let peak = samples[0];
  let maxBracketWidth = 0;
  for (const s of samples) {
    if (s.hi > peak.hi) peak = s;
    if (s.hi - s.lo > maxBracketWidth) maxBracketWidth = s.hi - s.lo;
  }

  return {
    total,
    chunks: sizes.length,
    ticksPerGap,
    samples,
    pushSamples,
    peak,
    finalLo: last.lo,
    finalHi: last.hi,
    onLineCount: onLine.mock.calls.length,
    onBatchStartCount: onBatchStart.mock.calls.length,
    onBatchEndCount: onBatchEnd.mock.calls.length,
    maxBracketWidth,
  };
}

// ───────────────────────── A. 队列深度曲线 ─────────────────────────

/** 档位：150 = 后端不切块的单块；4566 / 20000 = §2.10 现打的那两个数；13081 = §6 表里那个。 */
const SCALES = [150, 4566, 13081, 20000];
const PACINGS = [0, 2, 10];

describe("秤 5 · A：启动重放的队列深度曲线（纯外部观测，0 行 src 改动）", () => {
  beforeEach(() => {
    subs.clear();
    vi.useFakeTimers();
    // 见文件头「让出机制」：把 `makeYieldToMain` 的特性探测逼到 `setTimeout` 兜底那一支，
    // 否则它走 `MessageChannel`，而假定时器驱动不了 MessageChannel ⇒ 一条都排不出来。
    // ⚠ 必须在 `bindEvents` 之前 —— 那个探测是在 bind 时做一次的。
    vi.stubGlobal("MessageChannel", undefined);
  });

  // 收尾纪律同 `events-burst.vitest.ts`：那个 300ms grace 定时器必须**在桩还装着的时候跑完**，
  // 否则它在测试之外触发、一路跑进 `emitPerfSummary` 的遥测跳，成一条点不出判据名的 unhandled error。
  afterEach(() => {
    vi.runOnlyPendingTimers();
    vi.useRealTimers();
    vi.unstubAllGlobals();
  });

  it(
    "峰值随输入规模长，且前端 drain 追不上后端推送",
    async () => {
      const table: ReplayResult[] = [];
      for (const pacing of PACINGS) {
        for (const total of SCALES) {
          table.push(await runReplay(total, pacing));
        }
      }

      const lines: string[] = [];
      lines.push("");
      lines.push("=== 秤 5 · A 队列深度（queue.length 峰值，[下界,上界] 宽度 ≤1） ===");
      lines.push(
        `模型：后端 CHUNK_SIZE=${BACKEND_CHUNK_SIZE} / CHUNK_PAUSE_MS=${BACKEND_CHUNK_PAUSE_MS}` +
          ` / SINGLE_CHUNK_THRESHOLD=${BACKEND_SINGLE_CHUNK_THRESHOLD}；前端 BATCH_SIZE=${FRONT_BATCH_SIZE}`,
      );
      lines.push(
        "gap内drain | 输入条数 | 块数 | 队列峰值[lo,hi] | 峰值/输入 | 峰值出现在 | 收尾深度 | onLine",
      );
      for (const r of table) {
        lines.push(
          `${String(r.ticksPerGap).padStart(9)} | ${String(r.total).padStart(8)} | ` +
            `${String(r.chunks).padStart(4)} | ` +
            `${`[${r.peak.lo},${r.peak.hi}]`.padStart(15)} | ` +
            `${(r.peak.hi / r.total).toFixed(3).padStart(9)} | ` +
            `${r.peak.at.padEnd(26)} | ` +
            `[${r.finalLo},${r.finalHi}]`.padStart(8) +
            ` | ${r.onLineCount}`,
        );
      }
      console.log(lines.join("\n"));

      for (const r of table) {
        // ① 反演区间必须是紧的 —— 这条一红，说明流里出现了相邻哨兵，上面那笔账的
        //    「误差 ≤ 1」不再成立，所有深度读数都要重新说。
        expect(
          r.maxBracketWidth,
          `★ total=${r.total} pacing=${r.ticksPerGap}：深度反演区间宽到 ${r.maxBracketWidth} 条。\n` +
            "本文件的账只在「任意两个哨兵不相邻」时误差 ≤1。宽度破了 ⇒ 读数不再是 ±1 的，别照抄。",
        ).toBeLessThanOrEqual(1);

        // ② 一条都不能漏 / 不能重：喂进去多少 payload，onLine 就得被调多少次。
        expect(
          r.onLineCount,
          `★ total=${r.total} pacing=${r.ticksPerGap}：喂了 ${r.total} 条 payload，onLine 只被调了 ${r.onLineCount} 次。`,
        ).toBe(r.total);

        // ③ 收尾必须排空（下界=0）——否则「峰值」是在一个永远没排空的队列上读的。
        expect(
          r.finalLo,
          `★ total=${r.total} pacing=${r.ticksPerGap}：所有定时器跑完后队列没排空。`,
        ).toBe(0);

        // ④ **首块整块入队**：push 是同步的，drain 由 setTimeout 驱动 ⇒ 第一块的
        //    payload + 哨兵在任何 drain 之前就全在队列里。这是峰值的**硬下界**。
        const firstChunk = buildChunkSizes(r.total)[0];
        expect(
          r.peak.hi,
          `★ total=${r.total} pacing=${r.ticksPerGap}：峰值 ${r.peak.hi} < 首块 ${firstChunk} 条。\n` +
            "首块是同步 push 进去的、drain 走 setTimeout ⇒ 峰值不可能低于首块大小。这条一红说明喂法错了。",
        ).toBeGreaterThanOrEqual(firstChunk);

        // ⑤ 峰值不可能超过推进去的总条目数。
        expect(r.peak.hi, `★ total=${r.total}：峰值超过了 push 总量，账本算错了。`).toBeLessThanOrEqual(
          r.total + r.chunks + 1,
        );
      }

      // ⑥ **追不上**：多块场景下，每推完一个**满块**（CHUNK_SIZE=600 条），
      //    「推完第 i 块」这个同位置读数就比上一次严格更大 —— 后端 600 条/10ms 的
      //    推送速率对上前端 40 条/tick 的 drain 吞吐，在本文件试过的每一档 pacing 下
      //    队列都只涨不落。⚠ 断的是 **`setTimeout` 兜底那条让出路**（见文件头），
      //    不是 `MessageChannel` 那条。
      //
      //    ⚠ **余块（最后一块）排除在外，而且这不是为了让判据变绿**：`build_chunks`
      //    是**从尾往前切**的，余数落在**最后一块**（4566→366、20000→200、13081→481）。
      //    余块比一个 gap 的 drain 配额（pacing=10 时 10×40=400 条）还小时，深度**本来
      //    就该回落**——实测 pacing=10 / total=20000 的峰值正是落在第 32 块（最后一个满块）
      //    而不是第 33 块。把回落也断成「必须涨」是断错了物理，不是断严了。
      for (const r of table) {
        if (r.chunks < 2) continue;
        const sizes = buildChunkSizes(r.total);
        for (let i = 1; i < r.pushSamples.length; i++) {
          if (sizes[i] !== BACKEND_CHUNK_SIZE) continue; // 余块：见上方注释
          expect(
            r.pushSamples[i].hi,
            `★ total=${r.total} pacing=${r.ticksPerGap}：第 ${i} 个**满块**推完时深度 ${r.pushSamples[i].hi} ` +
              `没超过第 ${i - 1} 块推完时的 ${r.pushSamples[i - 1].hi}。\n` +
              "这条钉的是「drain 追不上后端推送」。它一红 = 前端在块间隙里追平了，\n" +
              "那 §2.10 的队列量级就得重算（峰值会被钉在一块的量级上，而不是全量）。",
          ).toBeGreaterThan(r.pushSamples[i - 1].hi);
        }
      }

      // ⑦ 峰值恒出现在**某次 push 刚完成**的时刻，不在任何 drain tick 之后
      //    —— 队列只在 push 时长、只在 drain 时消，这条把「峰值该在哪」钉死。
      for (const r of table) {
        expect(
          r.peak.at.startsWith("推完第"),
          `★ total=${r.total} pacing=${r.ticksPerGap}：峰值出现在「${r.peak.at}」而不是某次 push 之后。`,
        ).toBe(true);
      }

      // ⑧ 峰值随输入单调长（同一 pacing 下横跨四档）。
      for (const pacing of PACINGS) {
        const row = table.filter((r) => r.ticksPerGap === pacing);
        for (let i = 1; i < row.length; i++) {
          expect(
            row[i].peak.hi,
            `★ pacing=${pacing}：输入从 ${row[i - 1].total} 涨到 ${row[i].total}，峰值反而没涨。`,
          ).toBeGreaterThan(row[i - 1].peak.hi);
        }
      }
    },
    120_000,
  );

  it("单块重放（后端不切块那一档）：峰值 = 整块 + 两个哨兵", async () => {
    const r = await runReplay(150, 0);
    expect(r.chunks, "150 < SINGLE_CHUNK_THRESHOLD=200 时后端应该单块 emit").toBe(1);
    console.log(
      `[秤5·A 单块] total=150 → 峰值[${r.peak.lo},${r.peak.hi}]（= 150 payload + batch-start + batch-end）`,
    );
    // 单块场景下上界是确数：150 + 2。
    expect(
      r.peak.hi,
      "★ 单块 150 条时峰值应当恰是 152（150 payload + 2 个哨兵）——账本或喂法有问题。",
    ).toBe(152);
  });
});

// ───────────────────────── B. shift() vs 头指针 微基准 ─────────────────────────

/**
 * ⚠ **本节只产读数，一条时间断言都没有**（CI 上钉毫秒必 flaky）。
 * 唯一的断言是：两条路排出来的元素序列**逐条一致** —— 也就是「换头指针是等价替换」这一条。
 */
describe("秤 5 · B：shift() 排空 vs 头指针排空（只产读数，不拿时间做断言）", () => {
  interface Bench {
    n: number;
    shiftMs: number;
    headMs: number;
    ratio: number;
  }

  const drainByShift = (n: number): { order: number[]; ms: number } => {
    const arr: { id: number }[] = [];
    for (let i = 0; i < n; i++) arr.push({ id: i });
    const order: number[] = [];
    const t0 = performance.now();
    while (arr.length > 0) {
      const it = arr.shift();
      if (it) order.push(it.id);
    }
    const ms = performance.now() - t0;
    return { order, ms };
  };

  const drainByHead = (n: number): { order: number[]; ms: number } => {
    const arr: { id: number }[] = [];
    for (let i = 0; i < n; i++) arr.push({ id: i });
    const order: number[] = [];
    const t0 = performance.now();
    let head = 0;
    while (head < arr.length) {
      const it = arr[head++];
      if (it) order.push(it.id);
    }
    const ms = performance.now() - t0;
    return { order, ms };
  };

  const median = (xs: number[]): number => {
    const s = [...xs].sort((a, b) => a - b);
    return s[Math.floor(s.length / 2)];
  };

  it(
    "两条路的出队序列逐条一致；耗时只打印",
    () => {
      // 预热，别让第一档吃 JIT 的账。
      drainByShift(2000);
      drainByHead(2000);

      const rows: Bench[] = [];
      for (const n of SCALES) {
        const shiftRuns: number[] = [];
        const headRuns: number[] = [];
        let shiftOrder: number[] = [];
        let headOrder: number[] = [];
        for (let rep = 0; rep < 5; rep++) {
          const a = drainByShift(n);
          const b = drainByHead(n);
          shiftRuns.push(a.ms);
          headRuns.push(b.ms);
          shiftOrder = a.order;
          headOrder = b.order;
        }
        // ★ 唯一的断言：等价替换。
        expect(shiftOrder.length, `n=${n}：shift 路少排了元素`).toBe(n);
        expect(headOrder.length, `n=${n}：头指针路少排了元素`).toBe(n);
        expect(
          headOrder.join(","),
          `★ n=${n}：头指针排出来的序列与 shift() 不一致 —— 「换头指针是等价替换」这句话不成立，\n` +
            "§2.10 那条优化建议的前提就没了。",
        ).toBe(shiftOrder.join(","));

        const shiftMs = median(shiftRuns);
        const headMs = median(headRuns);
        rows.push({ n, shiftMs, headMs, ratio: headMs > 0 ? shiftMs / headMs : Number.POSITIVE_INFINITY });
      }

      const lines: string[] = [];
      lines.push("");
      lines.push("=== 秤 5 · B 排空耗时（node/jsdom，5 次取中位数；**不是**真机 WebView2） ===");
      lines.push("      n | shift() 排空 ms | 头指针排空 ms |   倍数");
      for (const r of rows) {
        lines.push(
          `${String(r.n).padStart(7)} | ${r.shiftMs.toFixed(3).padStart(15)} | ` +
            `${r.headMs.toFixed(3).padStart(13)} | ${r.ratio.toFixed(1).padStart(7)}×`,
        );
      }
      lines.push(
        "对照 §2.10 现打：n=4566 → 0.83ms（vs 头指针 0.042ms，20×）；n=20000 → 15.64ms（290×）",
      );
      console.log(lines.join("\n"));
    },
    120_000,
  );
});

// ───────────────────────── C. 镜像常量防漂 ─────────────────────────

describe("秤 5 · C：本文件镜像的那 4 个常量没有漂（读，不改）", () => {
  const read = (rel: string): string =>
    readFileSync(fileURLToPath(new URL(rel, import.meta.url)), "utf8");

  it("events.ts 的 BATCH_SIZE 与 event_replay.rs 的三个切块常量都还是本文件写的那个值", () => {
    const events = read("../src/events.ts");
    const replay = read("../src/bridge/src/event_replay.rs");
    // 抽取器自检：文件必须真的读到了东西。
    expect(events.length, "src/events.ts 读出来是空的 —— 本条会零命中地绿").toBeGreaterThan(1000);
    expect(replay.length, "event_replay.rs 读出来是空的 —— 本条会零命中地绿").toBeGreaterThan(1000);

    // ⚠ 用**正则**而不是逐字 `includes`：`src/events.ts` 正被另一路 agent 改，
    // 空格/分号这种排版抖动不该让本条红。红只应该由**数值真的变了**引起。
    const pins: { where: string; src: string; re: RegExp; mine: number }[] = [
      {
        where: "src/events.ts",
        src: events,
        re: /const\s+BATCH_SIZE\s*=\s*(\d+)\s*;/,
        mine: FRONT_BATCH_SIZE,
      },
      {
        where: "src/bridge/src/event_replay.rs",
        src: replay,
        re: /const\s+SINGLE_CHUNK_THRESHOLD\s*:\s*usize\s*=\s*(\d+)\s*;/,
        mine: BACKEND_SINGLE_CHUNK_THRESHOLD,
      },
      {
        where: "src/bridge/src/event_replay.rs",
        src: replay,
        re: /const\s+CHUNK_SIZE\s*:\s*usize\s*=\s*(\d+)\s*;/,
        mine: BACKEND_CHUNK_SIZE,
      },
      {
        where: "src/bridge/src/event_replay.rs",
        src: replay,
        re: /const\s+CHUNK_PAUSE_MS\s*:\s*u64\s*=\s*(\d+)\s*;/,
        mine: BACKEND_CHUNK_PAUSE_MS,
      },
    ];

    for (const pin of pins) {
      const m = pin.re.exec(pin.src);
      expect(
        m,
        `★ ${pin.where} 里用 ${pin.re} 抽不到那个常量 —— 要么它被删/改名了，要么本条的抽取器坏了。\n` +
          "两种都意味着：本文件复刻的分块/批量形状不再是生产的形状，读数要重跑。",
      ).not.toBeNull();
      expect(
        Number(m![1]),
        `★ ${pin.where} 的 ${pin.re} 现在是 ${m![1]}，本文件按 ${pin.mine} 算的读数。\n` +
          "`tests/evidence/A-scale5-replay-queue-depth.md` 里的表**作废**，重跑一遍再改那份读数。",
      ).toBe(pin.mine);
    }
  });
});

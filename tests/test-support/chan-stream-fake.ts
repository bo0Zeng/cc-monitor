/**
 * 〔CF2 · 第四波 4B〕`events.ts` 的会话内容从通道 `subscribe` 来（`src/ipc/chan.ts`）。测它的队列与批调度时，
 * 把 `src/ipc/chan` 换成本桩：记下每条订阅的 sink，由判据**按句柄的形状**往里灌 ——
 * 一格一行（`{"line": …}`）；成批那一段首块以 `{"batch":"start"}` 开头、每块以 `{"batch":"end"}` 收尾
 * （形状的另一侧是 `src/frontend/shell/src/event_replay.rs::batch_chunks`，本桩不从它派生）。
 *
 * 用法（每个判据文件各一次）：
 * ```ts
 * vi.mock("../src/ipc/chan", async () => (await import("./test-support/chan-stream-fake.ts")).chanStreamModule);
 * import { streamFake } from "./test-support/chan-stream-fake.ts";
 * await bindEvents(handlers, { streams: [{ origin: "<local>", kind: "session-lines" }] });
 * streamFake.lines([payload(1), payload(2)]);
 * ```
 */
import { vi } from "vitest";

interface Rec {
  origin: string;
  kind: string;
  sink: (items: unknown[]) => void;
  /** 每次 `want(n)` 的 n（判据核「还 credit」用）。 */
  wants: number[];
}

const recs: Rec[] = [];
let pos = 0;

export const chanStreamModule = {
  chan: {
    subscribe: vi.fn(
      (origin: string, kind: string, _from: unknown, _want: number, sink: (items: unknown[]) => void) => {
        const rec: Rec = { origin, kind, sink, wants: [] };
        recs.push(rec);
        return Promise.resolve({ want: (n: number) => rec.wants.push(n), stop: () => {} });
      },
    ),
  },
};

const frame = (body: unknown): unknown => ({ t: "frame", seq: pos++, body: JSON.stringify(body) });

function need(i: number): Rec {
  const r = recs[i];
  if (!r) throw new Error(`第 ${i} 条会话流没订上 —— bindEvents 忘了传 streams？（本桩会零命中地绿）`);
  return r;
}

export const streamFake = {
  reset(): void {
    recs.length = 0;
    pos = 0;
  },
  get subscriptions(): readonly Rec[] {
    return recs;
  },
  /** 逐行来的实时格（小批：没有批边界）。 */
  lines(payloads: unknown[], i = 0): void {
    need(i).sink(payloads.map((p) => frame({ line: p })));
  },
  /** 成批那一段的一块：`first` 时以 `start` 开头；每块以 `end` 收尾。 */
  chunk(first: boolean, payloads: unknown[], i = 0): void {
    const items: unknown[] = [];
    if (first) items.push(frame({ batch: "start" }));
    for (const p of payloads) items.push(frame({ line: p }));
    items.push(frame({ batch: "end" }));
    need(i).sink(items);
  },
  /** 〔MIG-1〕起停 / 状态那几种格（`{"ended": …}` 等，形状另一侧是 `bridge.rs::SessionStreamFrame`）：一格一个，不吃 credit。 */
  lifecycle(bodies: unknown[], i = 0): void {
    need(i).sink(bodies.map((b) => frame(b)));
  },
  /** 句柄说「丢了位置 [from, to)」。 */
  gap(from: number, to: number, i = 0): void {
    need(i).sink([{ t: "gap", fromSeq: from, toSeq: to }]);
  },
};

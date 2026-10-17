/**
 * 机器状态成品的推送照生成的类型交：本机那一帧（`machine-state`）与远端那几格（`changed/machine` 的 `body`）原样到订的人手里；
 * 帧里没带成品的那一格 ⇒ 整份重问 `machine-read`，回包里每台各交一次。
 */
import { beforeEach, describe, expect, it, vi } from "vitest";
import golden from "../../__fixtures__/machine-state.golden.json";
import type { MachineState } from "../../../src/frontend/ui/generated/MachineState";

type Listener = (e: { payload: unknown }) => void;
const listeners = new Map<string, Listener>();
let sink: ((items: unknown[]) => void) | null = null;
const reads: string[] = [];
let readReply: { machines: Record<string, MachineState> } = { machines: {} };

vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn((event: string, cb: Listener) => {
    listeners.set(event, cb);
    return Promise.resolve(() => {});
  }),
}));
vi.mock("../../../src/comms/inward/chan", () => ({
  chan: {
    subscribe: vi.fn((_origin: string, _kind: string, _from: unknown, _want: number, s: (items: unknown[]) => void) => {
      sink = s;
      return Promise.resolve({ want: () => {}, stop: () => {} });
    }),
  },
}));
vi.mock("../../../src/frontend/ui/ipc/chan-caller", () => ({
  budgetWithin: () => ({}),
  callTyped: vi.fn((_origin: string, cmd: string) => {
    reads.push(cmd);
    return Promise.resolve(readReply);
  }),
}));

import { MACHINE_STATE_EVENT, onMachineState } from "../../../src/frontend/ui/machine-feed";

// 金样里的成品各取几形（哪一形不要紧：这里只核原样交到）。
const [up, connecting, installing, , , , , needsUpdate] = (golden.cases as unknown as { machine: MachineState }[]).map((c) => c.machine);
const frame = (seq: number, body: unknown) => ({ t: "frame", seq, body: JSON.stringify(body) });
const flush = () => new Promise((r) => setTimeout(r, 0));

beforeEach(() => {
  listeners.clear();
  sink = null;
  reads.length = 0;
});

describe("机器状态成品的推送", () => {
  it("★ 本机那一帧原样交（形状是壳导出的类型）", () => {
    const got: [string, MachineState][] = [];
    onMachineState((o, m) => got.push([o, m]));
    listeners.get(MACHINE_STATE_EVENT)!({ payload: { origin: "<local>", machine: connecting } });
    expect(got).toEqual([["<local>", connecting]]);
  });

  it("★ 远端那几格：带了成品的原样交；没带的那一格 ⇒ 整份重问 machine-read、每台交一次", async () => {
    const got: [string, MachineState][] = [];
    onMachineState((o, m) => got.push([o, m]));
    await flush();
    sink!([frame(0, { key: "box", body: needsUpdate })]);
    expect(got).toEqual([["box", needsUpdate]]);
    expect(reads, "带了成品不重问").toEqual([]);
    readReply = { machines: { box: up, lab: installing } };
    sink!([frame(1, { key: "lab" })]);
    await flush();
    expect(reads).toEqual(["machine-read"]);
    expect(got.slice(1)).toEqual([
      ["box", up],
      ["lab", installing],
    ]);
  });
});

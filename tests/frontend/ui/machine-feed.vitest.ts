/**
 * 机器状态成品的推送照生成的类型交：各台（含本机）那几格（`changed/machine` 的 `body`）与本机后端起没起来那一格（`local-backend`）原样到订的人手里；
 * 订上就问一次 `machine-read`（订的那一刻那台已经接着时，壳不再交 `seen`，只等它就一台都拿不到）；帧里没带成品的那一格 ⇒ 整份重问，回包里每台各交一次。
 * 那台「连上了」那一刻：只在从别的样子变成连着时叫一次（起着 → 连着 · 断了 → 又连着），连着时再来的帧不重复叫。
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
vi.mock("../../../src/frontend/ui/ipc/chan-caller", async (actual) => ({
  ...(await actual<typeof import("../../../src/frontend/ui/ipc/chan-caller")>()),
  budgetWithin: () => ({}),
  callTyped: vi.fn((_origin: string, cmd: string) => {
    reads.push(cmd);
    return Promise.resolve(readReply);
  }),
}));

import { machineName } from "../../../src/frontend/ui/ipc/chan-caller";
import { LOCAL_BACKEND_EVENT, onLocalBackend, onMachineState, onMachineUp } from "../../../src/frontend/ui/machine-feed";
import type { LocalBackend } from "../../../src/frontend/ui/generated/LocalBackend";
import { copyText } from "../../../src/frontend/ui/copy-table";
import { WALL_CEILING_MS } from "../../test-support/wall-ceiling";
import { SUBSCRIBE_COALESCE_MS } from "../../../src/frontend/ui/events";

// 金样里的成品各取几形（哪一形不要紧：这里只核原样交到）。
const [up, connecting, installing, , , , , needsUpdate] = (golden.cases as unknown as { machine: MachineState }[]).map((c) => c.machine);
const frame = (seq: number, body: unknown) => ({ t: "frame", seq, body: JSON.stringify(body) });
const flush = () => new Promise((r) => setTimeout(r, 0));

beforeEach(() => {
  listeners.clear();
  sink = null;
  reads.length = 0;
  readReply = { machines: {} };
});

describe("机器状态成品的推送", () => {
  it("★ 订上就问一次各台此刻的成品（不等 seen）", async () => {
    readReply = { machines: { devbox: up } };
    const got: [string, MachineState][] = [];
    const off = onMachineState((o, m) => got.push([o, m]));
    await flush();
    await flush();
    expect(reads).toEqual(["machine-read"]);
    expect(got).toEqual([["devbox", up]]);
    off();
  });

  it("★ 本机后端起没起来那一格原样交（形状是壳导出的类型），不冒充哪一台的成品", () => {
    const got: LocalBackend[] = [];
    const states: string[] = [];
    onLocalBackend((l) => got.push(l));
    onMachineState((o) => states.push(o));
    const down: LocalBackend = { state: "down", word: "w", said: "s", next: null };
    listeners.get(LOCAL_BACKEND_EVENT)!({ payload: down });
    expect(got).toEqual([down]);
    expect(states).toEqual([]);
  });

  it("★ 远端那几格：带了成品的原样交；没带的那一格 ⇒ 整份重问 machine-read、每台交一次", async () => {
    const got: [string, MachineState][] = [];
    onMachineState((o, m) => got.push([o, m]));
    await flush();
    // 订上那一问（回包空）不算在下面。
    reads.length = 0;
    // 推来的那一批过了 `subscribeTopics` 的合批窗口才交 ⇒ 等它到。
    sink!([frame(0, { key: "box", body: needsUpdate })]);
    await vi.waitFor(() => expect(got).toEqual([["box", needsUpdate]]), { timeout: WALL_CEILING_MS });
    expect(reads, "带了成品不重问").toEqual([]);
    readReply = { machines: { box: up, lab: installing } };
    sink!([frame(1, { key: "lab" })]);
    await vi.waitFor(() => expect(got).toHaveLength(3), { timeout: WALL_CEILING_MS });
    expect(reads).toEqual(["machine-read"]);
    expect(got.slice(1)).toEqual([
      ["box", up],
      ["lab", installing],
    ]);
  });
});

describe("机器名照成品叫", () => {
  it("★ 后端那一行的 name 换个字，machineName 跟着换（本机远端同一条路）", async () => {
    readReply = { machines: { "<local>": { ...up, name: "甲字" }, devbox: { ...up, name: "乙字" } } };
    const off = onMachineState(() => {});
    await flush();
    await flush();
    expect([machineName("<local>"), machineName("devbox")]).toEqual(["甲字", "乙字"]);
    sink!([frame(0, { key: "<local>", body: { ...up, name: "丙字" } })]);
    await vi.waitFor(() => expect(machineName("<local>")).toBe("丙字"), { timeout: WALL_CEILING_MS });
    off();
  });
});

describe("onMachineUp", () => {
  it("只在变成连着的那一刻叫，每台各算", async () => {
    const got: string[] = [];
    const off = onMachineUp((o) => got.push(o));
    await flush();
    let seq = 0;
    // 每一帧等过合批窗口再推下一帧（同一台一窗里来的几帧只交最后那一份）。
    const push = async (origin: string, m: MachineState, want: number) => {
      sink!([frame(seq++, { key: origin, body: m })]);
      await new Promise((r) => setTimeout(r, SUBSCRIBE_COALESCE_MS + 50));
      expect(got.length).toBe(want);
    };
    await push("<local>", connecting, 0);
    await push("<local>", up, 1);
    await push("<local>", up, 1);
    await push("pi", up, 2);
    await push("<local>", needsUpdate, 2);
    await push("<local>", up, 3);
    expect(got).toEqual(["<local>", "pi", "<local>"]);
    off();
  });
});

describe("机器名还没到", () => {
  it("★ 那台的成品还没到 ⇒ 说不清是哪台（不拿本机 / 远端各一条字顶）", () => {
    expect([machineName("<never-seen>"), machineName("never-seen-host")]).toEqual([
      copyText("control.machine.unsure"),
      copyText("control.machine.unsure"),
    ]);
  });
});

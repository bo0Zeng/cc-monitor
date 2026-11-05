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

// 壳的 `backend_status`（订上之后问一次本机后端那一格）：默认不回（不影响别的几条）；要它回的那一条自己定。
const status = vi.hoisted(() => ({ reply: null as Promise<unknown> | null, asked: [] as unknown[] }));
vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn((cmd: string, args: unknown) => {
    if (cmd === "backend_status") status.asked.push(args);
    return status.reply ?? new Promise(() => {});
  }),
}));
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
import { onLocalBackend, onMachineState, onMachineUp } from "../../../src/frontend/ui/machine-feed";
import { LOCAL_BACKEND } from "../../../src/frontend/ui/generated/shell-names";
import type { LocalBackend } from "../../../src/frontend/ui/generated/LocalBackend";
import { copyText } from "../../../src/frontend/ui/copy-table";
import { KEYS_NOT_NAMES } from "../../../src/frontend/ui/generated/judgment-rules.ts";
import { WALL_CEILING_MS } from "../../test-support/wall-ceiling";
import { SUBSCRIBE_COALESCE_MS } from "../../../src/frontend/ui/events";

// 金样里的成品各取几形（哪一形不要紧：这里只核原样交到）。
const [up, connecting, installing, , keyRejected, , , needsUpdate] = (golden.cases as unknown as { machine: MachineState }[]).map((c) => c.machine);
const frame = (seq: number, body: unknown) => ({ t: "frame", seq, body: JSON.stringify(body) });
const flush = () => new Promise((r) => setTimeout(r, 0));

beforeEach(() => {
  listeners.clear();
  sink = null;
  reads.length = 0;
  readReply = { machines: {} };
});

// 放在最前：后面几条会喂进各台（含本机）的成品，名字记在模块里。
describe("机器名还没到", () => {
  it("★ 远端那台的成品还没到 ⇒ 叫它在机器表里的键（键就是名字）", () => {
    expect(machineName("devbox-no-product")).toBe("devbox-no-product");
  });

  it("★ 键不能当名字（本机那个哨兵）、成品还没到 ⇒ 机器不明（不拿一条「本机」顶）", () => {
    expect(KEYS_NOT_NAMES, "生成物里没有本机那个哨兵 —— 下一句在空表上恒红").toContain("<local>");
    expect(machineName("<local>")).toBe(copyText("control.machine.unsure"));
  });
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
    const down: LocalBackend = { state: "down", dot: "failed", tone: "fail", word: "w", said: "s", fixes: [], next: null };
    listeners.get(LOCAL_BACKEND)!({ payload: down });
    expect(got).toEqual([down]);
    expect(states).toEqual([]);
  });

  it("★ 订上就问一次壳那一格（`backend_status` 本机的 `local`）：订上之前起不来的那几帧收不到，问回来的照交（A2-17-15-1-1）", async () => {
    const down: LocalBackend = { state: "down", dot: "failed", tone: "fail", word: "w", said: "起不来那一句", fixes: [], next: null };
    status.asked.length = 0;
    status.reply = Promise.resolve({ local: down });
    const got: LocalBackend[] = [];
    const off = onLocalBackend((l) => got.push(l));
    await flush();
    await flush();
    status.reply = null;
    expect(status.asked).toEqual([{ origin: "<local>" }]);
    expect(got).toEqual([down]);
    off();
  });

  it("推来的那一帧先到 ⇒ 问回来的那一份不盖它（推的更新）", async () => {
    const pushed: LocalBackend = { state: "up" };
    let answer: (v: unknown) => void = () => {};
    status.reply = new Promise((r) => (answer = r));
    const got: LocalBackend[] = [];
    const off = onLocalBackend((l) => got.push(l));
    await flush();
    listeners.get(LOCAL_BACKEND)!({ payload: pushed });
    answer({ local: { state: "down", dot: "failed", tone: "fail", word: "w", said: "旧的", fixes: [], next: null } });
    await flush();
    status.reply = null;
    expect(got).toEqual([pushed]);
    off();
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
    await flush();
    vi.useFakeTimers();
    let seq = 0;
    // 每一帧走过合批窗口再推下一帧（同一台一窗里来的几帧只交最后那一份）。
    const push = async (origin: string, m: MachineState, want: number) => {
      sink!([frame(seq++, { key: origin, body: m })]);
      await vi.advanceTimersByTimeAsync(SUBSCRIBE_COALESCE_MS);
      expect(got.length).toBe(want);
    };
    try {
      await push("<local>", connecting, 0);
      await push("<local>", up, 1);
      await push("<local>", up, 1);
      await push("pi", up, 2);
      // 那台旧了照样连着（成品的 `linked`）⇒ 不算断开、回来不再叫。
      await push("<local>", needsUpdate, 2);
      await push("<local>", up, 2);
      await push("<local>", keyRejected, 2);
      await push("<local>", up, 3);
    } finally {
      vi.useRealTimers();
    }
    expect(got).toEqual(["<local>", "pi", "<local>"]);
    off();
  });
});

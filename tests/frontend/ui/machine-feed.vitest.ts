/**
 * 远端各台的状态成品：订上就问一次 `machine-read`。
 * 订的那一刻那台已经接着时，壳不再交 `seen`（`event_replay` 只在接没接着变了时说一格），只等 `seen` 才问 ⇒ 晚开的窗口一台成品都拿不到。
 * 那台「连上了」那一刻：只在从别的样子变成连着时叫一次（起着 → 连着 · 断了 → 又连着），连着时再来的帧不重复叫。
 */
import { describe, expect, it, vi } from "vitest";

const h = vi.hoisted(() => ({
  cb: null as ((e: { payload: unknown }) => void) | null,
  calls: [] as string[],
  machines: {} as Record<string, unknown>,
}));
vi.mock("@tauri-apps/api/event", () => ({
  listen: (_name: string, cb: (e: { payload: unknown }) => void) => {
    h.cb = cb;
    return Promise.resolve(() => {});
  },
}));
vi.mock("../../../src/comms/inward/chan", () => ({
  // 订上了、一格都不交（那台早就接着）。
  chan: { subscribe: vi.fn(() => Promise.resolve({ want: () => undefined, stop: () => undefined })) },
}));
vi.mock("../../../src/frontend/ui/ipc/chan-caller", () => ({
  budgetWithin: (ms: number) => ms,
  callTyped: vi.fn((_origin: string, op: string) => {
    h.calls.push(op);
    return Promise.resolve({ machines: h.machines });
  }),
}));

import { onMachineState, onMachineUp } from "../../../src/frontend/ui/machine-feed";
import golden from "../../__fixtures__/machine-state.golden.json";
import { UNTIL_CEILING } from "../../test-support/wall-ceiling";

/** 一帧：形状取跨语言金样里「连着」那一形（键集与后端成品逐格相等），只换 `state`。 */
const UP = (golden.cases as { name: string; machine: Record<string, unknown> }[]).find((c) => c.name === "连着")!.machine;

function frame(origin: string, state: string): void {
  h.cb?.({ payload: { origin, machine: { ...UP, state } } });
}

describe("机器状态推送", () => {
  it("订上就问一次各台此刻的成品（不等 seen）", async () => {
    h.calls.length = 0;
    h.machines = { devbox: { ...UP, state: "up" } };
    const got: [string, string | undefined][] = [];
    const off = onMachineState((origin, m) => got.push([origin, m?.state]));
    await vi.waitFor(() => expect(got).toEqual([["devbox", "up"]]), UNTIL_CEILING);
    expect(h.calls).toEqual(["machine-read"]);
    off();
  });
});

describe("onMachineUp", () => {
  it("只在变成连着的那一刻叫，每台各算", async () => {
    h.calls.length = 0;
    h.machines = {};
    const got: string[] = [];
    const off = onMachineUp((o) => got.push(o));
    await vi.waitFor(() => expect(h.calls).toEqual(["machine-read"]), UNTIL_CEILING);
    frame("<local>", "connecting");
    frame("<local>", "up");
    frame("<local>", "up");
    frame("pi", "up");
    frame("<local>", "down");
    frame("<local>", "up");
    expect(got).toEqual(["<local>", "pi", "<local>"]);
    off();
  });
});

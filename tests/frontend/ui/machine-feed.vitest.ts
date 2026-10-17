/**
 * 远端各台的状态成品：订上就问一次 `machine-read`。
 * 订的那一刻那台已经接着时，壳不再交 `seen`（`event_replay` 只在接没接着变了时说一格），只等 `seen` 才问 ⇒ 晚开的窗口一台成品都拿不到。
 */
import { describe, expect, it, vi } from "vitest";

const calls: string[] = [];
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(() => Promise.resolve(() => undefined)) }));
vi.mock("../../../src/comms/inward/chan", () => ({
  // 订上了、一格都不交（那台早就接着）。
  chan: { subscribe: vi.fn(() => Promise.resolve({ want: () => undefined, stop: () => undefined })) },
}));
vi.mock("../../../src/frontend/ui/ipc/chan-caller", () => ({
  budgetWithin: (ms: number) => ms,
  callTyped: vi.fn((_origin: string, op: string) => {
    calls.push(op);
    return Promise.resolve({
      machines: {
        devbox: { state: "up", reason: null, stage: null, os: "Linux", version: "4.1.1", versionRelation: "same", word: "", said: "", fixes: [], seenHostKey: null, detail: null },
      },
    });
  }),
}));

import { onMachineState } from "../../../src/frontend/ui/machine-feed";
import { UNTIL_CEILING } from "../../test-support/wall-ceiling";

describe("机器状态推送", () => {
  it("订上就问一次各台此刻的成品（不等 seen）", async () => {
    const got: [string, string | undefined][] = [];
    const off = onMachineState((origin, m) => got.push([origin, m?.state]));
    await vi.waitFor(() => expect(got).toEqual([["devbox", "up"]]), UNTIL_CEILING);
    expect(calls).toEqual(["machine-read"]);
    off();
  });
});

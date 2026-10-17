// 那台「连上了」那一刻：只在从别的样子变成连着时叫一次（起着 → 连着 · 断了 → 又连着），连着时再来的帧不重复叫。
import { describe, expect, it, vi } from "vitest";

const h = vi.hoisted(() => ({
  cb: null as ((e: { payload: unknown }) => void) | null,
}));
vi.mock("@tauri-apps/api/event", () => ({
  listen: (_name: string, cb: (e: { payload: unknown }) => void) => {
    h.cb = cb;
    return Promise.resolve(() => {});
  },
}));

import { onMachineUp } from "../../../src/frontend/ui/machine-feed";
import golden from "../../__fixtures__/machine-state.golden.json";

/** 一帧：形状取跨语言金样里「连着」那一形（键集与后端成品逐格相等），只换 `state`。 */
const UP = (golden.cases as { name: string; machine: Record<string, unknown> }[]).find((c) => c.name === "连着")!.machine;

function frame(origin: string, state: string): void {
  h.cb?.({ payload: { origin, machine: { ...UP, state } } });
}

describe("onMachineUp", () => {
  it("只在变成连着的那一刻叫，每台各算", () => {
    const got: string[] = [];
    onMachineUp((o) => got.push(o));
    frame("<local>", "connecting");
    frame("<local>", "up");
    frame("<local>", "up");
    frame("pi", "up");
    frame("<local>", "down");
    frame("<local>", "up");
    expect(got).toEqual(["<local>", "pi", "<local>"]);
  });
});

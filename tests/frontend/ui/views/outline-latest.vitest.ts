/**
 * A2-15 · Ctrl+F 大纲缺最新几句人说的 —— 两处根因各一格。
 *
 * - 首屏：骨架索引要到记录文件末尾（连尾段，A17-45-3-3：尾段的切点也要在账本里），大纲拿它种上，尾段里人说的那几句就在种子里，不另要。
 * - 按行号补：切进来补上流前那一段（`onSessionWatch` → `catchUpTo`）走批语义、不出「真用户输入上屏」，
 *   补进来的那句人说的也要进大纲（不重开查找）。
 *
 * 台子同 `live-user-inputs.vitest.ts`：渲染管线是真的，只 mock IPC；清单由 `outlineBackend` 替身回答，每条由用例写明。
 * 索引替身照后端 `index_page` 的口径认 `limit`：只回前 limit 行，`end` 是界外第一行的起点（台子里第 k 行的起点就是 k）。
 */
import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { owned } from "../../../test-support/owned-tab-manager.ts";
import { announced } from "../../../test-support/session-faces";

const indexStub = vi.hoisted(() => ({
  rows: null as null | Array<Record<string, unknown>>,
  limit: null as null | number,
}));
const linesStub = vi.hoisted(() => ({ answer: null as null | ((args: { from: number; until?: number }) => unknown) }));

vi.mock("@tauri-apps/api/core", async () => {
  const rig = await import("../../../test-support/session-viewer-rig");
  const { withSessionReads, isChanCall, chanArgsJson } = await import("../../../test-support/chan-fake");
  const reads = withSessionReads(async (cmd: string, raw: Record<string, unknown>) => {
    const args = raw as { fromOffset: number };
    if (cmd === "list_user_inputs") return rig.answerListUserInputs(args);
    if (cmd === "read_session_index" && indexStub.rows && args.fromOffset === 0) {
      const rows = indexStub.limit === null ? indexStub.rows : indexStub.rows.slice(0, indexStub.limit);
      return { available: true, from: 0, end: rows.length, rows };
    }
    if (cmd === "read_session_lines" && linesStub.answer) return linesStub.answer(raw as { from: number; until?: number });
    return undefined;
  });
  return {
    invoke: vi.fn(async (cmd: string, raw?: unknown) => {
      if (isChanCall(cmd, raw, "history-index")) {
        const b = chanArgsJson(raw) as { offset?: number; limit?: number };
        if (!b.offset) indexStub.limit = b.limit ?? null; // 只记从 0 起那一问（续传那几问替身不答）
      }
      return reads(cmd, raw);
    }),
  };
});
vi.mock("@tauri-apps/plugin-opener", () => ({ openPath: vi.fn().mockResolvedValue(undefined) }));
vi.mock("../../../../src/frontend/ui/tasks-panel", () => ({ fetchSessionTasks: vi.fn().mockResolvedValue([]) }));
vi.mock("../../../../src/frontend/ui/behavior", () => ({
  getBehavior: vi.fn().mockResolvedValue({ resumeCommand: "cct" }),
}));
vi.mock("../../../../src/frontend/ui/fork-flow", () => ({ runForkFlow: vi.fn().mockResolvedValue(undefined) }));

import { invoke } from "@tauri-apps/api/core";
import { sessionReadCalls } from "../../../test-support/chan-fake";
import {
  installViewerRig,
  assistantLine,
  userLine,
  outlineBackend,
  outlineEntry,
  settleOutline,
  type RigPayload,
} from "../../../test-support/session-viewer-rig";
import { TabManager, type Tab } from "../../../../src/frontend/ui/tabs";
import { LOCAL_ORIGIN } from "../../../../src/frontend/ui/ipc/origin";
import { UNTIL_CEILING } from "../../../test-support/wall-ceiling";

const outlineCalls = (): unknown[] => sessionReadCalls(vi.mocked(invoke).mock.calls, "list_user_inputs");

let tm: TabManager;

const feed = (p: RigPayload): void => tm.onLine(p as never);
const peek = (sid: string): Tab =>
  (tm as unknown as { store: { tabs: Map<string, Tab> } }).store.tabs.get(sid)!;
const rowsOf = (sid = "s1"): string[] => [
  ...peek(sid).inputsEl.querySelectorAll<HTMLButtonElement>(".user-input-row"),
].map((r) => r.dataset.inputUuid ?? "");

beforeEach(() => {
  vi.mocked(invoke).mockClear();
  installViewerRig();
  const barEl = document.createElement("div");
  const streamRootEl = document.createElement("div");
  document.body.append(barEl, streamRootEl);
  tm = owned(announced(new TabManager(barEl, streamRootEl)));
});

afterEach(() => {
  indexStub.rows = null;
  indexStub.limit = null;
  linesStub.answer = null;
  document.body.replaceChildren();
});

describe("A2-15 大纲含到此刻为止人说的每一句", () => {
  it("🔴 首屏索引要到末尾 ⇒ 尾段里那句人说的就在种子里、排在最末，不另要清单", async () => {
    // 400 行：第 1 行与第 390 行是人说的，其余是回复。重放尾块先到（同 live）：第 390 行钉 floor；索引不照 floor 截，
    // 尾段里那一句在种子里。
    const N = 400;
    const said: Record<number, string> = { 1: "很久以前那一句", 390: "最新那一句" };
    indexStub.rows = Array.from({ length: N }, (_, k) => ({
      o: k,
      n: 1,
      t: said[k] !== undefined ? "said" : "reply",
      u: `u${k}`,
      ...(said[k] !== undefined ? { x: said[k], ts: `t${k}` } : {}),
    }));
    tm.onBatchStart();
    const order = [...Array.from({ length: N - 390 }, (_, k) => 390 + k), ...Array.from({ length: 390 }, (_, k) => k)];
    const lineAt = (k: number): RigPayload => (said[k] !== undefined ? userLine(k, `u${k}`, said[k]) : assistantLine(k, `u${k}`, `回复 ${k}`));
    feed(lineAt(order[0]));
    // jsdom 没布局 ⇒ 「满没满一屏」退回算术判据；给一个已满屏的几何，不然批结束把账本整份物化、floor 落到 0（同 `tabs.vitest.ts::replay`）
    Object.defineProperty(peek("s1").streamEl, "scrollHeight", { value: 2000, configurable: true });
    Object.defineProperty(peek("s1").streamEl, "clientHeight", { value: 800, configurable: true });
    for (const k of order.slice(1)) feed(lineAt(k));
    tm.onBatchEnd();
    const floor = peek("s1").window.floorSeq ?? -1;
    expect(floor, "前置：尾块的第一行钉 floor").toBe(390);
    // 清单替身：问到哪一份都是假的那几条（不该被问到）。
    outlineBackend.entries = Array.from({ length: N }, (_, k) => outlineEntry(`never-${k}`));
    await vi.waitFor(async () => {
      await settleOutline();
      expect(indexStub.limit, "索引那一问照 floor 截了 ⇒ 尾段不在账本里").toBeNull();
      expect(rowsOf()).toEqual(["u1", "u390"]);
    }, UNTIL_CEILING);
    expect(
      outlineCalls().filter((a) => (a as { fromOffset: number }).fromOffset < N),
      "种子已经到末尾：要也只从种子的 end 接着要，不往回要",
    ).toEqual([]);
  });

  it("🔴 已要到过之后，切进来按行号补进一句人说的 ⇒ 不重开查找，大纲里有它、排在最末", async () => {
    // 切进来那一下的停留判定（`STAY_MS` 之后「有新行才要」）要先过去：远端按行号补得比它慢，补进来时它早问过了。
    // 只认这一个计时（150ms）跑完，别的照常。
    let stayed = false;
    const real = globalThis.setTimeout;
    vi.spyOn(globalThis, "setTimeout").mockImplementation(((fn: () => void, ms?: number, ...rest: unknown[]) =>
      real(ms === 150 ? () => { fn(); stayed = true; } : fn, ms, ...rest)) as typeof setTimeout);
    outlineBackend.entries = [outlineEntry("u1", "一号")];
    feed(userLine(1, "u1", "一号"));
    await vi.waitFor(async () => {
      await settleOutline();
      expect(stayed, "前置：停留判定已经跑过").toBe(true);
    }, UNTIL_CEILING);
    vi.mocked(globalThis.setTimeout).mockRestore();
    expect(rowsOf(), "前置：清单已经要到过").toEqual(["u1"]);

    // 没在看的时候那台不上流：在终端 / 手机上又说了一句，切回来按行号补（[2, 3)）。
    outlineBackend.entries.push(outlineEntry("u2", "在终端上说的"));
    linesStub.answer = ({ from, until }) => ({
      from,
      next: until ?? 3,
      eof: until === undefined,
      payloads: [userLine(2, "u2", "在终端上说的")],
    });
    await settleOutline();
    expect(rowsOf(), "前置：没有在途的那一趟替它要到").toEqual(["u1"]);
    vi.mocked(invoke).mockClear();
    tm.onSessionWatch(LOCAL_ORIGIN, { sid: "s1", path: "/p/s1.jsonl", seq: 3 });
    await vi.waitFor(async () => {
      await settleOutline();
      expect(peek("s1").seenSeqs.has(2), "前置：补的那一行真喂进来了").toBe(true);
      expect(rowsOf()).toEqual(["u1", "u2"]);
    }, UNTIL_CEILING);
    expect(outlineCalls(), "从上次的 end 接着要").toEqual([{ origin: "<local>", jsonlPath: "/p/s1.jsonl", fromOffset: 1 }]);
  });
});

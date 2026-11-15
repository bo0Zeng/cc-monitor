/**
 * 任务清单的重问（`TabManager.refreshTasks`，含 all）与建 tab 那一问：同一（台 · 会话）一次只在飞一问，在飞时再要只补一趟；
 * 补之前那个会话的 tab 关了 ⇒ 不补。那台答得慢、`changed/tasks` 推得密也不堆。期望手写。
 */
import { describe, expect, it, vi } from "vitest";
import { owned } from "../../test-support/owned-tab-manager.ts";
import { announced, goldenFace } from "../../test-support/session-faces";

/** 替身那台：每一问压住，`release` 才回。 */
const there = vi.hoisted(() => ({ asked: [] as string[], held: [] as Array<() => void> }));

vi.mock("@tauri-apps/api/core", async () => {
  const rig = await import("../../test-support/session-viewer-rig");
  const { withSessionReads } = await import("../../test-support/chan-fake");
  return {
    invoke: vi.fn(
      withSessionReads(async (cmd: string, args: Record<string, unknown>) =>
        cmd === "list_user_inputs" ? rig.answerListUserInputs(args as { fromOffset: number }) : undefined,
      ),
    ),
  };
});
vi.mock("@tauri-apps/plugin-opener", () => ({ openPath: vi.fn().mockResolvedValue(undefined) }));
vi.mock("../../../src/frontend/ui/tasks-panel", () => ({
  fetchSessionTasks: vi.fn(async (sid: string, origin: string) => {
    there.asked.push(`${origin} ${sid}`);
    await new Promise<void>((r) => there.held.push(r));
    return { tasks: [] };
  }),
}));
vi.mock("../../../src/frontend/ui/behavior", () => ({
  getBehavior: vi.fn().mockResolvedValue({ resumeCommand: "cct" }),
}));
vi.mock("../../../src/frontend/ui/fork-flow", () => ({ runForkFlow: vi.fn().mockResolvedValue(undefined) }));

import { TabManager } from "../../../src/frontend/ui/tabs";
import { installViewerRig } from "../../test-support/session-viewer-rig";

const flush = () => new Promise((r) => setTimeout(r, 0));
async function release(): Promise<void> {
  for (const r of there.held.splice(0)) r();
  await flush();
  await flush();
}

function rig(): TabManager {
  installViewerRig();
  const barEl = document.createElement("div");
  const streamRootEl = document.createElement("div");
  document.body.replaceChildren(barEl, streamRootEl);
  return owned(announced(new TabManager(barEl, streamRootEl)));
}

describe("任务清单推得密时同一会话不堆", () => {
  it("★ 回包压住、refreshTasks(all) 连叫 5 次 ⇒ 每会话 1 问在飞（建 tab 那一问算在内）；放回 ⇒ 恰补 1 问；补之前关掉 tab 的不补", async () => {
    const tm = rig();
    tm.ensureTab("t1", "/p", "/p/t1.jsonl", "box-t");
    tm.ensureTab("t2", "/p", "/p/t2.jsonl", "box-t");
    tm.ensureTab("t3", "/p", "/p/t3.jsonl", "box-t");
    await flush();
    expect(there.asked.sort()).toEqual(["box-t t1", "box-t t2", "box-t t3"]);
    there.asked.length = 0;
    for (let i = 0; i < 5; i++) tm.refreshTasks("box-t", [], true);
    tm.refreshTasks("box-t", ["t1"], false);
    await flush();
    expect(there.asked).toEqual([]);
    // 关得掉的只有已结束的那张脸（在跑的不许关）：先让它结束、再关。
    tm.tabOf("t3")!.face = goldenFace("ended");
    tm.closeTab("t3");
    expect(tm.tabOf("t3")).toBeNull();
    await release();
    expect(there.asked.sort()).toEqual(["box-t t1", "box-t t2"]);
    there.asked.length = 0;
    await release();
    expect(there.asked).toEqual([]);
  });
});

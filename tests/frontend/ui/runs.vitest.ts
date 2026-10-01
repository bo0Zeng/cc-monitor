/**
 * 要求：子 agent 的流归各自的运行、主 tab 上常驻一行「<标签> · 在跑 · 最近：<那件事>」，跑完收到派出它的那张工具卡上；界面只收通用形
 * （运行表 ＋ 归一事件），不认任何一家的目录、字段、事件名 —— 用户原话「是不是把子agent放进主agent了? 子agent好像没显示在跑」。
 *
 * 两段：纯状态（`LiveCards` ＋ 运行表，期望手写）· 真 TabManager（主 tab 尾巴上的行、派出它的那张卡收到完成态）。
 */
import { describe, it, expect, vi } from "vitest";

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
vi.mock("../../../src/frontend/ui/tasks-panel", () => ({ fetchSessionTasks: vi.fn().mockResolvedValue([]) }));
vi.mock("../../../src/frontend/ui/turn-notify", () => ({ turnEndNotifier: { observe: vi.fn() } }));
vi.mock("../../../src/frontend/ui/behavior", () => ({
  getBehavior: vi.fn().mockResolvedValue({ resumeCommandLocal: "", resumeCommandRemote: "cct" }),
}));
vi.mock("../../../src/frontend/ui/fork-flow", () => ({ runForkFlow: vi.fn().mockResolvedValue(undefined) }));

import { LiveCards, type TapPayload } from "../../../src/frontend/ui/live-card";
import { copyText } from "../../../src/frontend/ui/copy-table";
import type { RunInfo } from "../../../src/frontend/ui/generated/RunInfo";
import type { StreamEv } from "../../../src/frontend/ui/generated/StreamEv";
import { TabManager } from "../../../src/frontend/ui/tabs";
import { livePainter } from "../../../src/frontend/ui/live-card-view";
import { installViewerRig, line } from "../../test-support/session-viewer-rig";

const O = "<local>";
const SID = "s1";

function taps(resp: number, evs: StreamEv[], run?: string): TapPayload[] {
  return evs.map((ev, n) => ({ origin: O, stream: SID, resp, n, ev, ...(run ? { run } : {}) }));
}

const running = (run: string, label: string, tool: string): RunInfo => ({ run, label, tool, state: "running" });

describe("运行表 × 归一流（期望手写）", () => {
  it("两段子运行流各归各行；主 tab 的活卡只有主运行那段；子运行收场 ⇒ 那一行撤掉、交出它", () => {
    const live = new LiveCards(
      (o, s) => (o === O && s === SID ? s : null),
      () => null,
    );
    live.onRuns(SID, [running("w1", "扫目录", "t1"), running("w2", "写报告", "t2")]);
    for (const t of taps(1, [{ t: "start", rid: "r1" }, { t: "block", i: 0, kind: "tool", tool: "Bash" }], "w1")) live.onTap(t);
    for (const t of taps(2, [{ t: "start", rid: "r2" }, { t: "block", i: 0, kind: "thinking" }], "w2")) live.onTap(t);
    for (const t of taps(0, [{ t: "start", rid: "r0" }, { t: "block", i: 0, kind: "text" }, { t: "text", i: 0, s: "主" }])) live.onTap(t);

    const state = copyText("runs.state.running");
    expect(live.rowsOf(SID)).toEqual([
      { run: "w1", text: copyText("runs.row.text", { label: "扫目录", state, last: copyText("liveCard.block.toolUse", { tool: "Bash" }) }) },
      { run: "w2", text: copyText("runs.row.text", { label: "写报告", state, last: copyText("runs.last.think") }) },
    ]);
    // 主活卡：只有主运行那段（子运行的「准备调用 Bash」不在这里）。
    const main = live.core.cardsOf(SID);
    expect(main.map((c) => c.messageId)).toEqual(["r0"]);
    expect(main.flatMap((c) => c.blocks.map((b) => b.tool ?? b.kind))).toEqual(["text"]);
    expect(live.core.cardsOf(SID, "w1").map((c) => c.messageId)).toEqual(["r1"]);

    const finished = live.onRuns(SID, [{ ...running("w1", "扫目录", "t1"), state: "done" }, running("w2", "写报告", "t2")]);
    expect(finished.map((r) => r.run)).toEqual(["w1"]);
    expect(live.rowsOf(SID).map((r) => r.run)).toEqual(["w2"]);
    expect(live.core.cardsOf(SID, "w1")).toEqual([], "收场的子运行，它在攒的那几段一起撤");
  });

  it("没有流的时候「最近」用运行表给的那一件；什么都没有 ⇒ 只有标签与状态；没标签 ⇒ 通用叫法", () => {
    const live = new LiveCards(() => SID, () => null);
    live.onRuns(SID, [
      { run: "a", label: "L", state: "running", last: { t: "tool", name: "Grep" } },
      { run: "b", state: "running" },
    ]);
    const state = copyText("runs.state.running");
    expect(live.rowsOf(SID).map((r) => r.text)).toEqual([
      copyText("runs.row.text", { label: "L", state, last: copyText("runs.last.tool", { tool: "Grep" }) }),
      copyText("runs.row.bare", { label: copyText("runs.label.unnamed"), state }),
    ]);
  });
});

describe("真 TabManager：主 tab 尾巴上每个在跑的子运行一行，跑完收到那张工具卡上", () => {
  it("运行表到了 ⇒ 尾巴上一行；收场 ⇒ 行撤掉、派出它的那张卡标上完成", () => {
    installViewerRig();
    const barEl = document.createElement("div");
    const streamRootEl = document.createElement("div");
    document.body.append(barEl, streamRootEl);
    const tm = new TabManager(barEl, streamRootEl);
    tm.setLivePainter(livePainter({ timeline: () => document.createElement("div"), closed: () => {} }));
    tm.onLine(
      line(0, {
        type: "assistant",
        uuid: "a0",
        timestamp: "2026-09-10T00:00:00.000Z",
        message: { id: "m0", role: "assistant", content: [{ type: "tool_use", id: "t1", name: "Spawn", input: {} }] },
        sessionId: SID,
        requestId: null,
        parentUuid: null,
        forkedFrom: null,
        isApiErrorMessage: false,
        error: null,
        apiErrorStatus: null,
        toolCards: { t1: "agent" },
        childRuns: { t1: { label: "扫目录", kind: "Explore" } },
      }) as never,
    );
    const card = streamRootEl.querySelector<HTMLElement>('[data-run-tool="t1"]');
    expect(card?.querySelector("summary")?.textContent).toBe(copyText("runCard.summary.text", { kind: "Explore", label: "扫目录" }));
    const rows = (): string[] => [...streamRootEl.querySelectorAll<HTMLElement>("[data-run]")].filter((e) => e.getAttribute("role") === "button").map((e) => e.textContent ?? "");

    tm.onSessionRuns({ session_id: SID, runs: [running("w1", "扫目录", "t1")] });
    expect(rows()).toEqual([copyText("runs.row.bare", { label: "扫目录", state: copyText("runs.state.running") })]);

    tm.onSessionRuns({ session_id: SID, runs: [{ ...running("w1", "扫目录", "t1"), state: "done" }] });
    expect(rows()).toEqual([]);
    expect(card?.dataset.run).toBe("w1");
    expect(card?.querySelector("summary")?.textContent).toBe(
      copyText("runCard.summary.state", {
        title: copyText("runCard.summary.text", { kind: "Explore", label: "扫目录" }),
        state: copyText("runs.state.done"),
      }),
    );
  });
});

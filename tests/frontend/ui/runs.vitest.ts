/**
 * 要求：子运行不进主 tab 的消息流，在跑的只在 agent 面板里列（有上界），状态只读后端给的那一份 —— 用户原话
 * 「全是agent的 / 我们不是有agent部分吗, 为什么全部放主界面」；派出它的那张工具卡标状态。界面只收通用形
 * （运行表 ＋ 归一事件），不认任何一家的目录、字段、事件名。
 *
 * 三段：纯状态（`LiveCards` ＋ 运行表）· 面板的上界（`panelGroups`，期望手写）· 真 TabManager ＋ 真面板
 * （主 tab 零子运行行、面板分组与五态、后台派出只拿到「已启动」那次结果的不是完成、被叫停的是「已停止」）。
 */
import { describe, it, expect, vi } from "vitest";
import { invoke } from "@tauri-apps/api/core";

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
  getBehavior: vi.fn().mockResolvedValue({ resumeCommand: "cct" }),
}));
vi.mock("../../../src/frontend/ui/fork-flow", () => ({ runForkFlow: vi.fn().mockResolvedValue(undefined) }));

import { LiveCards, type TapPayload } from "../../../src/frontend/ui/live-card";
import { copyText } from "../../../src/frontend/ui/copy-table";
import type { RunInfo } from "../../../src/frontend/ui/generated/RunInfo";
import type { RunState } from "../../../src/frontend/ui/generated/RunState";
import type { StreamEv } from "../../../src/frontend/ui/generated/StreamEv";
import { TabManager } from "../../../src/frontend/ui/tabs";
import { AgentsPanel } from "../../../src/frontend/ui/agents-panel";
import { livePainter } from "../../../src/frontend/ui/live-card-view";
import { panelGroups, RECENT_ENDED, runStateMark } from "../../../src/frontend/ui/runs";
import { installViewerRig, line, userLine } from "../../test-support/session-viewer-rig";

const O = "<local>";
const SID = "s1";

function taps(resp: number, evs: StreamEv[], run?: string): TapPayload[] {
  return evs.map((ev, n) => ({ origin: O, stream: SID, resp, n, ev, ...(run ? { run } : {}) }));
}

const run = (id: string, state: RunState, label = id, tool?: string): RunInfo => ({ run: id, label, state, ...(tool ? { tool } : {}) });

/** 状态那一格画的是什么：在跑 ＝ 呼吸的状态点（V10），其余四态 ＝ Phosphor 图标（经 kit/icon）；不许是字符。 */
function markOf(host: Element | null): string | undefined {
  if (!host) return undefined;
  if ((host.textContent ?? "") !== "") return `text:${host.textContent}`;
  const svg = host.querySelector<SVGElement>("svg[data-icon]");
  if (svg) return `icon:${svg.dataset.icon}`;
  const dot = host.querySelector<HTMLElement>("[data-state]");
  return dot ? `dot:${dot.dataset.state}` : undefined;
}
const MARK: Record<RunState, string> = { running: "dot:running", done: "icon:check", failed: "icon:failed", stopped: "icon:stop", unknown: "icon:question" };

describe("runStateMark：五态各画什么", () => {
  it("在跑是状态点，其余是图标，都不带字", () => {
    for (const s of Object.keys(MARK) as RunState[]) {
      const host = document.createElement("span");
      host.appendChild(runStateMark(s));
      expect(markOf(host), s).toBe(MARK[s]);
    }
  });
});

describe("运行表 × 归一流", () => {
  it("两段子运行流各归各的运行；主 tab 的活卡只有主运行那段；子运行收场 ⇒ 它在攒的那几段撤掉、交出它", () => {
    const live = new LiveCards(
      (o, s) => (o === O && s === SID ? s : null),
      () => null,
    );
    live.onRuns(SID, [run("w1", "running", "扫目录", "t1"), run("w2", "running", "写报告", "t2")]);
    for (const t of taps(1, [{ t: "start", rid: "r1" }, { t: "block", i: 0, kind: "tool", tool: "Bash" }], "w1")) live.onTap(t);
    for (const t of taps(2, [{ t: "start", rid: "r2" }, { t: "block", i: 0, kind: "thinking" }], "w2")) live.onTap(t);
    for (const t of taps(0, [{ t: "start", rid: "r0" }, { t: "block", i: 0, kind: "text" }, { t: "text", i: 0, s: "主" }])) live.onTap(t);

    const main = live.core.cardsOf(SID);
    expect(main.map((c) => c.messageId)).toEqual(["r0"]);
    expect(main.flatMap((c) => c.blocks.map((b) => b.tool ?? b.kind))).toEqual(["text"]);
    expect(live.core.cardsOf(SID, "w1").map((c) => c.messageId)).toEqual(["r1"]);
    expect(live.core.liveBlockOf(SID, "w2")?.kind).toBe("thinking");

    const finished = live.onRuns(SID, [run("w1", "stopped", "扫目录", "t1"), run("w2", "running", "写报告", "t2")]);
    expect(finished.map((r) => r.run)).toEqual(["w1"]);
    expect(live.core.cardsOf(SID, "w1"), "收场的子运行，它在攒的那几段一起撤").toEqual([]);
  });
});

describe("面板的上界：在跑的全列 ＋ 最近结束的几个，其余与状态不明的收进「更早的」", () => {
  it("运行表（最早动过的在前）⇒ 三组，每组最近的在前；最近结束的恰好 RECENT_ENDED 个", () => {
    expect(RECENT_ENDED).toBe(5);
    const table: RunInfo[] = [
      run("e1", "done"),
      run("u1", "unknown"),
      run("e2", "failed"),
      run("r1", "running"),
      run("e3", "done"),
      run("e4", "stopped"),
      run("e5", "done"),
      run("r2", "running"),
      run("e6", "done"),
      run("u2", "unknown"),
      run("e7", "failed"),
      run("r3", "running"),
    ];
    const g = panelGroups(table);
    const ids = (v: RunInfo[]) => v.map((r) => r.run);
    expect(ids(g.running)).toEqual(["r3", "r2", "r1"]);
    expect(ids(g.recent)).toEqual(["e7", "e6", "e5", "e4", "e3"]);
    expect(ids(g.older)).toEqual(["u2", "e2", "u1", "e1"]);
    // 两向：三组恰好分完整张表，不重不漏。
    expect([...ids(g.running), ...ids(g.recent), ...ids(g.older)].sort()).toEqual(ids(table).sort());
  });
});

/** 一条主运行的记录：派出一个子运行（后端给的卡型与标签）。 */
function dispatch(seq: number, tool: string, label: string): never {
  return line(seq, {
    type: "assistant",
    uuid: `a${seq}`,
    timestamp: "2026-09-10T00:00:00.000Z",
    message: { id: `m${seq}`, role: "assistant", content: [{ type: "tool_use", id: tool, name: "Spawn", input: {} }] },
    sessionId: SID,
    requestId: null,
    parentUuid: null,
    forkedFrom: null,
    isApiErrorMessage: false,
    error: null,
    apiErrorStatus: null,
    toolCards: { [tool]: "agent" },
    childRuns: { [tool]: { label, kind: "Explore" } },
  }) as never;
}

/** 那次调用当场拿到的结果（后台派出那一形：一拿到就是「已启动」，子运行还在跑）。 */
function launched(seq: number, tool: string): never {
  return line(seq, {
    type: "user",
    uuid: `u${seq}`,
    timestamp: "2026-09-10T00:00:01.000Z",
    message: { role: "user", content: [{ type: "tool_result", tool_use_id: tool, content: "x", is_error: false }] },
    sessionId: SID,
  }) as never;
}

describe("真 TabManager ＋ 真 agent 面板", () => {
  function rig() {
    installViewerRig();
    const barEl = document.createElement("div");
    const streamRootEl = document.createElement("div");
    const panel = new AgentsPanel();
    document.body.replaceChildren(barEl, streamRootEl, panel.summaryElement, panel.pageElement);
    const tm = new TabManager(barEl, streamRootEl, undefined, undefined, panel);
    tm.setLivePainter(livePainter);
    return { tm, streamRootEl, panel };
  }
  const rowsIn = (panel: AgentsPanel) =>
    [...panel.pageElement.querySelectorAll<HTMLElement>(".agent-row")].map((r) => ({
      run: r.dataset.run,
      icon: markOf(r.querySelector(".agent-icon")),
      state: r.querySelector(".agent-state")?.textContent,
      cls: [...r.classList].find((c) => c.startsWith("agent-") && c !== "agent-row" && c !== "agent-row-clickable"),
    }));
  const groupsIn = (panel: AgentsPanel) => [...panel.pageElement.querySelectorAll(".agent-group")].map((g) => g.textContent);
  const st = (s: RunState) => ({ icon: MARK[s], state: copyText(`runs.state.${s}`), cls: `agent-${s}` });

  // ① 前台跑完 ② 后台完成通知 ③ 后台失败 ④ 被叫停 ⑤ 被额度打断（无通知、子记录停写）⑥ 真在跑 —— 状态是后端判好给的。
  const SIX: RunInfo[] = [
    run("w5", "unknown", "额度打断"),
    run("w1", "done", "前台跑完"),
    run("w2", "done", "后台完成"),
    run("w3", "failed", "后台失败"),
    run("w4", "stopped", "被叫停"),
    run("w6", "running", "扫目录", "t1"),
  ];

  it("★ 主 tab 的消息流里零子运行行；面板：在跑的一组、最近结束的一组、状态不明的收进「更早的」；①–⑤ 一个都不是在跑", () => {
    const { tm, streamRootEl, panel } = rig();
    tm.onLine(dispatch(0, "t1", "扫目录"));
    tm.switchTo(SID);
    tm.onSessionRuns({ session_id: SID, runs: SIX, ended: [] });

    // 主 tab：只在运行表里出现的那几个标签，消息流里一个字都没有；也没有任何按运行标的行。
    const mainText = streamRootEl.textContent ?? "";
    for (const r of SIX.filter((x) => x.run !== "w6")) expect(mainText, `主 tab 里出现了子运行「${r.label}」`).not.toContain(r.label);
    expect(streamRootEl.querySelectorAll("[data-run]").length, "主 tab 里有按运行标的行").toBe(0);
    // 正控：同一批标签在面板里（「更早的」那一组收着）。
    const panelText = panel.pageElement.textContent ?? "";
    for (const r of SIX.filter((x) => x.run !== "w5")) expect(panelText).toContain(r.label);
    expect(panelText).not.toContain("额度打断");

    expect(groupsIn(panel)).toEqual([
      copyText("agentsPanel.group.running"),
      copyText("agentsPanel.group.recent"),
      copyText("agentsPanel.group.older", { n: 1 }),
    ]);
    expect(rowsIn(panel)).toEqual([
      { run: "w6", ...st("running") },
      { run: "w4", ...st("stopped") },
      { run: "w3", ...st("failed") },
      { run: "w2", ...st("done") },
      { run: "w1", ...st("done") },
    ]);
    panel.pageElement.querySelector<HTMLButtonElement>(".agent-older-toggle")!.click();
    expect(rowsIn(panel).at(-1)).toEqual({ run: "w5", ...st("unknown") });
    expect(
      rowsIn(panel)
        .filter((r) => r.run !== "w6")
        .map((r) => r.state),
      "跑完的 / 败了的 / 被叫停的 / 状态不明的显示成了在跑",
    ).not.toContain(copyText("runs.state.running"));
  });

  it("★ 运行表先到、交回那一条后到 ⇒ 消息流里那条交回的抬头用运行表给的标签（不是来话自带的名字）；运行表里没有它 ⇒ 退到来话自带的名字", () => {
    const { tm, streamRootEl } = rig();
    tm.onLine(dispatch(0, "t1", "扫目录"));
    tm.switchTo(SID);
    tm.onSessionRuns({ session_id: SID, runs: [run("w6", "done", "扫目录", "t1")], ended: [] });
    const handback = (seq: number, from: string) =>
      userLine(seq, `u${seq}`, "<agent-message>…</agent-message>", {
        userText: { speaker: { kind: "agentMessage", from, name: "worker-7", handback: true, body: "报告正文" }, text: "报告正文" },
      }) as never;
    tm.onLine(handback(1, "w6"));
    tm.onLine(handback(2, "w9"));
    const titles = [...streamRootEl.querySelectorAll<HTMLElement>(".card-speaker[data-kind=agent] .speaker-title")].map((t) => t.textContent);
    expect(titles).toEqual([copyText("speaker.agent.title", { label: "扫目录" }), copyText("speaker.agent.title", { label: "worker-7" })]);
  });

  it("★ 交回与收场通知只报一次：会话事实说那个子运行交回了 ⇒ 消息流里它的收场通知收起，交回那一条留着", () => {
    const { tm, streamRootEl } = rig();
    tm.onLine(dispatch(0, "t1", "扫目录"));
    tm.switchTo(SID);
    const rec = (seq: number, speaker: Record<string, unknown>) =>
      userLine(seq, `u${seq}`, "x", { userText: { speaker, text: "x" } }) as never;
    tm.onLine(rec(1, { kind: "agentMessage", from: "w6", name: "worker-7", handback: true, body: "报告正文" }));
    tm.onLine(rec(2, { kind: "taskNotification", taskId: "w6", status: "completed", summary: "扫目录" }));
    tm.onLine(rec(3, { kind: "taskNotification", taskId: "w8", status: "completed", summary: "别的" }));
    const notices = () =>
      [...streamRootEl.querySelectorAll<HTMLElement>(".card-notice:not([hidden]) .notice-row:not([hidden])")].map((r) => r.textContent ?? "");
    expect(notices().join("|")).toContain("扫目录");
    // 句子里的状态写字（完成 / 失败），不拿字符当图标（C-W1）。
    expect(notices()).toContain(copyText("speaker.notice.row", { what: "扫目录", state: copyText("runs.state.done"), time: notices()[0].split(" · ").at(-1) ?? "" }));
    const facts = { agent: "claude", end: 1, forkedFrom: null, touchedFiles: [], usage: null, projectDir: null, writers: [], pending: [], lastSay: null, needs: null, handedBack: ["w6"], retries: [] };
    (tm as unknown as { onSessionFacts(sid: string, f: unknown): void }).onSessionFacts(SID, facts);
    expect(notices().join("|")).not.toContain("扫目录");
    expect(notices().join("|"), "别的子运行的通知照常").toContain("别的");
    expect(streamRootEl.querySelectorAll(".card-speaker[data-kind=agent]").length, "交回那一条留着").toBe(1);
  });

  it("★ 后台派出、只拿到「已启动」那次结果、没有完成通知 ⇒ 面板上不是 ✓（界面不自己判「有结果 ⇒ 完成」）；被叫停 ⇒「已停止」", () => {
    const { tm, panel } = rig();
    tm.onLine(dispatch(0, "t1", "扫目录"));
    tm.onLine(launched(1, "t1"));
    tm.switchTo(SID);
    tm.onSessionRuns({ session_id: SID, runs: [run("w6", "running", "扫目录", "t1")], ended: [] });
    expect(rowsIn(panel)).toEqual([{ run: "w6", ...st("running") }]);
    tm.onSessionRuns({ session_id: SID, runs: [run("w6", "stopped", "扫目录", "t1")], ended: [] });
    expect(rowsIn(panel)).toEqual([{ run: "w6", ...st("stopped") }]);
    expect(MARK.stopped).not.toBe(MARK.done);
  });

  it("被挤出运行表的已收场子运行（`ended`）：派出它的那张卡照样标终态；面板上不列它", () => {
    const { tm, streamRootEl, panel } = rig();
    tm.onLine(dispatch(0, "t1", "扫目录"));
    tm.switchTo(SID);
    const card = streamRootEl.querySelector<HTMLElement>('[data-role="run-card"]')!;
    tm.onSessionRuns({ session_id: SID, runs: [], ended: [{ run: "w6", tool: "t1", state: "done" }] });
    expect([card.dataset.runState, card.querySelector('[data-role="run-state"]')?.textContent]).toEqual(["done", copyText("runs.state.done")]);
    expect(panel.pageElement.querySelector('.agent-row[data-run="w6"]')).toBeNull();
  });

  it("派出它的那张工具卡标状态与用时，不就地展开时间线；点面板那一行 / 点卡头 ⇒ 开它自己的窗口；窗口开着 ⇒ 两处都标「窗口已开」", () => {
    const { tm, streamRootEl, panel } = rig();
    tm.onLine(dispatch(0, "t1", "扫目录"));
    tm.switchTo(SID);
    const card = streamRootEl.querySelector<HTMLElement>('[data-role="run-card"]')!;
    const stateOf = () => card.querySelector('[data-role="run-state"]')?.textContent;
    tm.onSessionRuns({ session_id: SID, runs: [run("w6", "running", "扫目录", "t1")], ended: [] });
    expect([card.dataset.runState, stateOf()]).toEqual(["running", copyText("runs.state.running")]);
    const T = Date.parse("2026-10-01T10:00:00Z");
    tm.onSessionRuns({ session_id: SID, runs: [{ ...run("w6", "failed", "扫目录", "t1"), started_ms: T, ended_ms: T + 120_000 }], ended: [] });
    expect([card.dataset.runState, stateOf()]).toEqual([
      "failed",
      [copyText("runs.state.failed"), copyText("agentWindow.facts.took", { dur: "2m" })].join(copyText("kit.text.sep")),
    ]);
    expect(card.querySelector("details:not([data-role])"), "卡里不再有就地展开的时间线").toBeNull();

    const opened = () => vi.mocked(invoke).mock.calls.filter((c) => c[0] === "open_session_in_new_window").map((c) => (c[1] as { run?: string }).run);
    const row = () => panel.pageElement.querySelector<HTMLElement>('.agent-row[data-run="w6"]')!;
    row().click();
    expect(opened()).toEqual(["w6"]);
    expect(row().getAttribute("aria-expanded"), "面板里不再就地展开").toBeNull();
    expect(panel.pageElement.querySelector(".agent-timeline")).toBeNull();

    expect(row().querySelector('[data-role="window-open"]')).toBeNull();
    tm.setRunWindow(SID, "w6", true);
    expect(row().querySelector('[data-role="window-open"]')?.textContent).toBe(copyText("agentsPanel.row.opened"));
    expect(card.querySelector('[data-role="window-open"]')?.textContent).toBe(copyText("agentsPanel.row.opened"));
    tm.setRunWindow(SID, "w6", false);
    expect(row().querySelector('[data-role="window-open"]')).toBeNull();
    expect(card.querySelector('[data-role="window-open"]')).toBeNull();
  });

  it("窗口先开着、运行表后到（主窗口重新载入那一形）⇒ 派出卡照样标「窗口已开」", () => {
    const { tm, streamRootEl } = rig();
    tm.onLine(dispatch(0, "t1", "扫目录"));
    tm.switchTo(SID);
    tm.setRunWindow(SID, "w6", true);
    tm.onSessionRuns({ session_id: SID, runs: [run("w6", "running", "扫目录", "t1")], ended: [] });
    const card = streamRootEl.querySelector<HTMLElement>('[data-role="run-card"]')!;
    expect(card.querySelector('[data-role="window-open"]')).not.toBeNull();
  });

  it("回到派出它的地方（主会话派的）⇒ 切到那个会话、派出它的那张卡闪一下", () => {
    const { tm, streamRootEl } = rig();
    tm.onLine(dispatch(0, "t1", "扫目录"));
    tm.showRunCard(SID, "t1");
    expect(tm.activeSessionId()).toBe(SID);
    expect(streamRootEl.querySelector('[data-role="run-card"]')?.classList.contains("search-hit-flash")).toBe(true);
  });

  it("派出那一方拿到的结果收在派出卡里、可展开：交回的结果写几个字；报错写「报错」", () => {
    const { tm, streamRootEl } = rig();
    tm.onLine(dispatch(0, "t1", "扫目录"));
    tm.onLine(
      line(1, {
        type: "user",
        uuid: "u1",
        timestamp: "2026-09-10T00:00:01.000Z",
        message: { role: "user", content: [{ type: "tool_result", tool_use_id: "t1", content: "找到两处", is_error: false }] },
        sessionId: SID,
      }) as never,
    );
    tm.switchTo(SID);
    const res = streamRootEl.querySelector<HTMLDetailsElement>('[data-role="run-card"] [data-role="run-result"]')!;
    expect(res.querySelector("summary")?.textContent).toBe(copyText("runCard.result.done", { n: 4 }));
    expect(res.open).toBe(false);
    expect(res.textContent, "收着时不建正文").not.toContain("找到两处");
    res.open = true;
    res.dispatchEvent(new Event("toggle"));
    expect(res.textContent).toContain("找到两处");
  });
});

/**
 * 要求：子运行不进主 tab 的消息流，在跑的只在 agent 面板里列（有上界），状态只读后端给的那一份 —— 要求
 * 「全是agent的 / 我们不是有agent部分吗, 为什么全部放主界面」；派出它的那张工具卡标状态。界面只收通用形
 * （运行表 ＋ 归一事件），不认任何一家的目录、字段、事件名。
 *
 * 三段：纯状态（`LiveCards` ＋ 运行表）· 面板照帧里写好的三组与 chip 的字排（`groups` · `summary`，不按 `state` 分）· 真 TabManager ＋ 真面板
 * （主 tab 零子运行行、面板分组与五态、后台派出只拿到「已启动」那次结果的不是完成、被叫停的是「已停止」）。
 */
import { describe, it, expect, vi } from "vitest";
import { owned } from "../../test-support/owned-tab-manager.ts";
import { announced } from "../../test-support/session-faces";
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
vi.mock("../../../src/frontend/ui/behavior", () => ({
  getBehavior: vi.fn().mockResolvedValue({ resumeCommand: "cct" }),
}));
vi.mock("../../../src/frontend/ui/fork-flow", () => ({ runForkFlow: vi.fn().mockResolvedValue(undefined) }));

import { LiveCards } from "../../../src/frontend/ui/live-card";
import type { LiveCardNow } from "../../../src/frontend/ui/generated/LiveCardNow";
import type { SessionLiveCardsPayload } from "../../../src/frontend/ui/generated/SessionLiveCardsPayload";
import { copyText } from "../../../src/frontend/ui/copy-table";
import type { RunInfo } from "../../../src/frontend/ui/generated/RunInfo";
import type { RunState } from "../../../src/frontend/ui/generated/RunState";
import { TabManager } from "../../../src/frontend/ui/tabs";
import { AgentsPanel } from "../../../src/frontend/ui/agents-panel";
import { livePainter } from "../../../src/frontend/ui/live-card-view";
import { runLabel, runLastText, runStateMark, runStateText } from "../../../src/frontend/ui/runs";
import type { RunEnded } from "../../../src/frontend/ui/generated/RunEnded";
import type { RunGroups } from "../../../src/frontend/ui/generated/RunGroups";
import type { SessionRuns } from "../../../src/frontend/ui/generated/SessionRuns";
import { installViewerRig, line, userLine } from "../../test-support/session-viewer-rig";

const O = "<local>";
const SID = "s1";

/** 壳交来的一格：那一处（这个会话 · 某个运行）此刻只有这一张卡。 */
function opened(no: number, rid: string, blocks: LiveCardNow["blocks"], run?: string): SessionLiveCardsPayload {
  return { origin: O, live: { sid: SID, ...(run ? { run } : {}), cards: [{ card: no, rid, phase: "streaming", clipped: false, blocks, head: "核心·头", body: `核心·正文·${rid}`, last: `核心·最近·${rid}` }] } };
}

/** 后端写好的状态字（夹具用记号，界面照抄才对得上：不许按 `state` 自己挑）。 */
const said = (s: RunState) => `核心·${s}`;
const run = (id: string, state: RunState, label = id, tool?: string): RunInfo => ({ run: id, label, state, status_text: said(state), label_text: `核心·${label}`, ...(tool ? { tool } : {}) });
/** 运行表那一帧：三组与 chip 的字是后端写好的（没给三组 ⇒ 全放「最近结束」，最近动过的在前；只看卡与行的几条不在乎分在哪组）。 */
const table = (runs: RunInfo[], ended: RunEnded[] = [], groups?: RunGroups): SessionRuns => ({
  sid: SID,
  runs,
  ended,
  groups: groups ?? { running: [], recent: [...runs].reverse().map((r) => r.run), older: [] },
  ...(runs.length > 0 ? { summary: `核心·agent ${runs.length}` } : {}),
});

/** 状态那一格画的是什么：在跑 ＝ 呼吸的状态点，其余四态 ＝ Phosphor 图标（经 kit/icon）；不许是字符。 */
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

describe("运行表的字照后端那一份", () => {
  it("★ 状态 · 名字 · 记录里最近一件事都照抄写好的字，不按 `state` · `last.t` 挑字", () => {
    const r: RunInfo = { ...run("w1", "failed", "扫目录"), last: { t: "tool", name: "Bash" }, last_text: "核心·调用 Bash" };
    expect([runStateText(r), runLabel(r), runLastText(r, null)]).toEqual(["核心·failed", "核心·扫目录", "核心·调用 Bash"]);
    expect(runLastText({ ...r, last: undefined, last_text: undefined }, null)).toBeNull();
  });
});

describe("运行表 × 活卡", () => {
  it("子运行的卡各归各的运行；主 tab 的活卡只有主运行那张；运行表变了交出收场的那几个（卡由后端撤）", () => {
    const live = new LiveCards(
      (o, s) => o === O && s === SID,
      () => null,
    );
    live.onRuns(SID, table([run("w1", "running", "扫目录", "t1"), run("w2", "running", "写报告", "t2")]));
    live.onCards(opened(1, "r1", [{ kind: "tool", tool: "Bash" }], "w1"));
    live.onCards(opened(2, "r2", [{ kind: "thinking" }], "w2"));
    live.onCards(opened(0, "r0", [{ kind: "text", text: "主" }]));

    const main = live.cardsOf(SID);
    expect(main.map((c) => c.messageId)).toEqual(["r0"]);
    expect(main.map((c) => c.body)).toEqual(["核心·正文·r0"]);
    expect(live.cardsOf(SID, "w1").map((c) => c.messageId)).toEqual(["r1"]);
    expect(live.liveLastOf(SID, "w2"), "「最近」那一截照核心写好的").toBe("核心·最近·r2");

    const finished = live.onRuns(SID, table([run("w1", "stopped", "扫目录", "t1"), run("w2", "running", "写报告", "t2")]));
    expect(finished.map((r) => r.run)).toEqual(["w1"]);
  });
});

describe("面板照帧里写好的三组与 chip 的字排", () => {
  it("★ 三组照 `groups`（与 `state` 无关）、组内照它给的先后；更早那一组的组头照抄 `older_text`、chip 的字照抄 `summary`；组里点名表里没有的 ⇒ 不画", () => {
    const panel = new AgentsPanel();
    // 故意让分组与状态对不上：界面若还按 `state` 自己分，这条就红。
    const runs = [run("a", "done"), run("b", "running"), run("c", "failed"), run("d", "unknown")];
    panel.setSession(SID, { ...table(runs, [], { running: ["c", "a"], recent: ["d"], older: ["b", "gone"], older_text: "核心·更早 ×1" }), summary: "核心·agent 摘要" });
    const rows = () => [...panel.pageElement.querySelectorAll<HTMLElement>(".agent-row")].map((r) => r.dataset.run);
    expect(rows()).toEqual(["c", "a", "d"]);
    expect([...panel.pageElement.querySelectorAll(".agent-group")].map((g) => g.textContent)).toEqual([
      copyText("agentsPanel.group.running"),
      copyText("agentsPanel.group.recent"),
      "核心·更早 ×1",
    ]);
    expect(panel.summaryElement.textContent).toBe("核心·agent 摘要");
    expect(panel.label).toBe("核心·agent 摘要");
    panel.pageElement.querySelector<HTMLButtonElement>(".agent-older-toggle")!.click();
    expect(rows()).toEqual(["c", "a", "d", "b"]);
    panel.setSession(SID, table([]));
    expect(panel.summaryElement.style.display).toBe("none");
  });
});

/** 一条主运行的记录：派出一个子运行（后端给的卡型与标签）。 */
function dispatch(seq: number, tool: string, label: string): never {
  return line(seq, {
    t: "reply",
    id: `a${seq}`,
    at: "2026-09-10T00:00:00.000Z",
    blocks: [{ type: "tool_use", id: tool, name: "Spawn", input: {} }],
    autoReply: false,
    cards: { [tool]: "agent" },
    runs: { [tool]: { label, kind: "Explore" } },
    draw: "steps",
    human: false,
  }) as never;
}

/** 一条工具结果（`who` 是工具结果那一种）。 */
function result(seq: number, tool: string, text: string, kind: "handback" | "launched" = "handback"): never {
  return line(seq, {
    t: "said",
    id: `u${seq}`,
    at: "2026-09-10T00:00:01.000Z",
    who: { speaker: { kind: "toolResult" }, text: "" },
    blocks: [{ type: "tool_result", for: tool, content: [{ type: "text", text }], isError: false }],
    results: { [tool]: { ok: true, preview: text, chars: [...text].length, kind } },
    draw: "steps",
    human: false,
  }) as never;
}

/** 出口省掉了结果正文的那一形（桌面交「折起那一行」声明）：只剩核心出的预览与字数。 */
function foldedResult(seq: number, tool: string, preview: string, chars: number): never {
  return line(seq, {
    t: "said",
    id: `u${seq}`,
    at: "2026-09-10T00:00:01.000Z",
    who: { speaker: { kind: "toolResult" }, text: "" },
    blocks: [{ type: "tool_result", for: tool, isError: false }],
    results: { [tool]: { ok: true, preview, chars, kind: "handback" } },
    draw: "steps",
    human: false,
  }) as never;
}

/** 那次调用当场拿到的结果（后台派出那一形：一拿到就是「已启动」，子运行还在跑）。 */
function launched(seq: number, tool: string): never {
  return result(seq, tool, "Async agent launched successfully.", "launched");
}

describe("真 TabManager ＋ 真 agent 面板", () => {
  function rig() {
    installViewerRig();
    const barEl = document.createElement("div");
    const streamRootEl = document.createElement("div");
    const panel = new AgentsPanel();
    document.body.replaceChildren(barEl, streamRootEl, panel.summaryElement, panel.pageElement);
    const tm = owned(announced(new TabManager(barEl, streamRootEl, undefined, undefined, panel)));
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
  const st = (s: RunState) => ({ icon: MARK[s], state: said(s), cls: `agent-${s}` });

  // ① 前台跑完 ② 后台完成通知 ③ 后台失败 ④ 被叫停 ⑤ 被额度打断（无通知、子记录停写）⑥ 真在跑 —— 状态是后端判好给的。
  const SIX: RunInfo[] = [
    run("w5", "unknown", "额度打断"),
    run("w1", "done", "前台跑完"),
    run("w2", "done", "后台完成"),
    run("w3", "failed", "后台失败"),
    run("w4", "stopped", "被叫停"),
    run("w6", "running", "扫目录", "t1"),
  ];

  it("★ 重画不丢焦点（kit place）：焦点在一行上，运行表换了一份 ⇒ 还是那个节点、焦点还在它上", () => {
    const { tm, panel } = rig();
    tm.onLine(dispatch(0, "t1", "扫目录"));
    tm.switchTo(SID);
    tm.onSessionRuns(table(SIX, []));
    const row = panel.pageElement.querySelector<HTMLElement>('.agent-row[data-run="w6"]')!;
    row.tabIndex = 0;
    row.focus();
    tm.onSessionRuns(table([run("w7", "running", "新的一个", "t2"), ...SIX], []));
    expect(panel.pageElement.querySelector('.agent-row[data-run="w6"]')).toBe(row);
    expect(document.activeElement).toBe(row);
  });

  it("★ 主 tab 的消息流里零子运行行；面板：在跑的一组、最近结束的一组、状态不明的收进「更早的」；①–⑤ 一个都不是在跑", () => {
    const { tm, streamRootEl, panel } = rig();
    tm.onLine(dispatch(0, "t1", "扫目录"));
    tm.switchTo(SID);
    tm.onSessionRuns(table(SIX, [], { running: ["w6"], recent: ["w4", "w3", "w2", "w1"], older: ["w5"], older_text: "核心·更早 ×1" }));

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
      "核心·更早 ×1",
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
    ).not.toContain(said("running"));
  });

  it("★ 运行表先到、交回那一条后到 ⇒ 消息流里那条交回的抬头用运行表给的标签（不是来话自带的名字）；运行表里没有它 ⇒ 退到来话自带的名字", () => {
    const { tm, streamRootEl } = rig();
    tm.onLine(dispatch(0, "t1", "扫目录"));
    tm.switchTo(SID);
    tm.onSessionRuns(table([run("w6", "done", "扫目录", "t1")], []));
    const handback = (seq: number, from: string) =>
      userLine(seq, `u${seq}`, "<agent-message>…</agent-message>", {
        who: { speaker: { kind: "agentMessage", from, name: "worker-7", handback: true, body: "报告正文" }, text: "报告正文" },
      }) as never;
    tm.onLine(handback(1, "w6"));
    tm.onLine(handback(2, "w9"));
    const titles = [...streamRootEl.querySelectorAll<HTMLElement>(".card-speaker[data-kind=agent] .speaker-title")].map((t) => t.textContent);
    expect(titles).toEqual([copyText("speaker.agent.title", { label: "核心·扫目录" }), copyText("speaker.agent.title", { label: "worker-7" })]);
  });

  it("★ 交回与收场通知只报一次：会话事实说那个子运行交回了 ⇒ 消息流里它的收场通知收起，交回那一条留着", () => {
    const { tm, streamRootEl } = rig();
    tm.onLine(dispatch(0, "t1", "扫目录"));
    tm.switchTo(SID);
    const rec = (seq: number, speaker: Record<string, unknown>) =>
      userLine(seq, `u${seq}`, "x", { who: { speaker, text: "x" } }) as never;
    tm.onLine(rec(1, { kind: "agentMessage", from: "w6", name: "worker-7", handback: true, body: "报告正文" }));
    // 那一行是后端写好的（`row`），界面照抄。
    tm.onLine(rec(2, { kind: "taskNotification", taskId: "w6", status: "completed", end: "done", summary: "扫目录", row: "后台任务 扫目录 完成 · 14:02" }));
    tm.onLine(rec(3, { kind: "taskNotification", taskId: "w8", status: "completed", end: "done", summary: "别的", row: "后台任务 别的 完成 · 14:03" }));
    const notices = () =>
      [...streamRootEl.querySelectorAll<HTMLElement>(".card-notice:not([hidden]) .notice-row:not([hidden])")].map((r) => r.textContent ?? "");
    expect(notices()).toEqual(["后台任务 扫目录 完成 · 14:02", "后台任务 别的 完成 · 14:03"]);
    // 后端说这两条是一组、w6 那条交回过不露（`notices` 那一格）。
    const notice = { id: "u2", members: ["u2", "u3"], hidden: ["u2"], head: null, failed: false };
    const facts = { agent: "claude", forkedFrom: null, touchedFiles: [], usage: null, projectDir: null, writers: [], pending: [], lastSay: null, needs: null, handedBack: ["w6"], retries: [], notices: [notice], permissionMode: null, tokens: null, cost: null, mcp: [], startedBy: null, children: [] };
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
    tm.onSessionRuns(table([run("w6", "running", "扫目录", "t1")], []));
    expect(rowsIn(panel)).toEqual([{ run: "w6", ...st("running") }]);
    tm.onSessionRuns(table([run("w6", "stopped", "扫目录", "t1")], []));
    expect(rowsIn(panel)).toEqual([{ run: "w6", ...st("stopped") }]);
    expect(MARK.stopped).not.toBe(MARK.done);
  });

  it("★ 后台派出当场回的「已启动」回执（核心说 `kind: launched`）⇒ 派出卡上不写「交回结果」，卡上的状态照运行表（还在跑）", () => {
    const { tm, streamRootEl } = rig();
    tm.onLine(dispatch(0, "t1", "扫目录"));
    tm.onLine(launched(1, "t1"));
    tm.switchTo(SID);
    tm.onSessionRuns(table([run("w6", "running", "扫目录", "t1")], []));
    const card = streamRootEl.querySelector<HTMLElement>('[data-role="run-card"]')!;
    expect(card.querySelector('[data-role="run-result"]'), "回执不是交回").toBeNull();
    expect(card.dataset.runState).toBe("running");
    // 后来真交回了（前台那一形 / 换了一次）⇒ 才写交回。
    tm.onLine(result(2, "t1", "找到两处"));
    expect(card.querySelector('[data-role="run-result"] summary')?.textContent).toBe(copyText("runCard.result.done", { n: 4 }));
  });

  it("被挤出运行表的已收场子运行（`ended`）：派出它的那张卡照样标终态；面板上不列它", () => {
    const { tm, streamRootEl, panel } = rig();
    tm.onLine(dispatch(0, "t1", "扫目录"));
    tm.switchTo(SID);
    const card = streamRootEl.querySelector<HTMLElement>('[data-role="run-card"]')!;
    tm.onSessionRuns(table([], [{ run: "w6", tool: "t1", state: "done", status_text: said("done") }]));
    expect([card.dataset.runState, card.querySelector('[data-role="run-state"]')?.textContent]).toEqual(["done", said("done")]);
    expect(panel.pageElement.querySelector('.agent-row[data-run="w6"]')).toBeNull();
  });

  it("派出它的那张工具卡标状态与用时，不就地展开时间线；点面板那一行 / 点卡头 ⇒ 开它自己的窗口；窗口开着 ⇒ 两处都标「窗口已开」", () => {
    const { tm, streamRootEl, panel } = rig();
    tm.onLine(dispatch(0, "t1", "扫目录"));
    tm.switchTo(SID);
    const card = streamRootEl.querySelector<HTMLElement>('[data-role="run-card"]')!;
    const stateOf = () => card.querySelector('[data-role="run-state"]')?.textContent;
    tm.onSessionRuns(table([run("w6", "running", "扫目录", "t1")], []));
    expect([card.dataset.runState, stateOf()]).toEqual(["running", said("running")]);
    const T = Date.parse("2026-10-01T10:00:00Z");
    tm.onSessionRuns(table([{ ...run("w6", "failed", "扫目录", "t1"), started_ms: T, ended_ms: T + 120_000 }], []));
    expect([card.dataset.runState, stateOf()]).toEqual([
      "failed",
      [said("failed"), copyText("agentWindow.facts.took", { dur: "2m" })].join(copyText("kit.text.sep")),
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
    tm.onSessionRuns(table([run("w6", "running", "扫目录", "t1")], []));
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
    tm.onLine(result(1, "t1", "找到两处"));
    tm.switchTo(SID);
    const res = streamRootEl.querySelector<HTMLDetailsElement>('[data-role="run-card"] [data-role="run-result"]')!;
    expect(res.querySelector("summary")?.textContent).toBe(copyText("runCard.result.done", { n: 4 }));
    expect(res.open).toBe(false);
    expect(res.textContent, "收着时不建正文").not.toContain("找到两处");
    res.open = true;
    res.dispatchEvent(new Event("toggle"));
    expect(res.textContent).toContain("找到两处");
  });

  it("★ 结果正文被出口省掉：字数照核心那一格写（不拿预览数字），展开写预览", () => {
    const { tm, streamRootEl } = rig();
    tm.onLine(dispatch(0, "t1", "扫目录"));
    tm.onLine(foldedResult(1, "t1", "找到两处", 1234));
    tm.switchTo(SID);
    const res = streamRootEl.querySelector<HTMLDetailsElement>('[data-role="run-card"] [data-role="run-result"]')!;
    expect(res.querySelector("summary")?.textContent).toBe(copyText("runCard.result.done", { n: 1234 }));
    res.open = true;
    res.dispatchEvent(new Event("toggle"));
    expect(res.textContent).toContain("找到两处");
  });
});

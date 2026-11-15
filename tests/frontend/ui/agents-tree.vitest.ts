/**
 * 要求（用户定）：agent 页按派出关系成树 —— 第一层默认展开，第二层及以下默认折叠，折叠行写汇总（下面几个 · 几个在跑）；
 * 不因下层有事自动展开；不记住手动展开 / 折叠（页关了再开回默认）。
 * 树与汇总句是后端写好的（`kids` · `kids_text` · `kids_open`，三组只列第一层）：面板照摆，不自己数、不自己判开不开。
 */
import { describe, it, expect } from "vitest";
import { AgentsPanel } from "../../../src/frontend/ui/agents-panel";
import type { RunInfo } from "../../../src/frontend/ui/generated/RunInfo";
import type { RunState } from "../../../src/frontend/ui/generated/RunState";
import type { SessionRuns } from "../../../src/frontend/ui/generated/SessionRuns";

const SID = "s1";
const run = (id: string, state: RunState, more: Partial<RunInfo> = {}): RunInfo => ({
  run: id,
  state,
  status_text: `核心·${state}`,
  label_text: `核心·${id}`,
  ...more,
});

/**
 * 三层：a（第一层，开着）→ b（第二层，收着，汇总「核心·b 底下」）→ c · d；e 第一层没派过。
 * 汇总句故意不是数出来的样子：界面若自己数，这条就红。
 */
function table(cNeeds = false): SessionRuns {
  const runs = [
    run("a", "running", { kids: ["b"], kids_text: "核心·a 底下", kids_open: true }),
    run("b", "done", { parent: "a", kids: ["c", "d"], kids_text: "核心·b 底下" }),
    run("c", "running", { parent: "b", ...(cNeeds ? { needs: { kind: "approval" } as never } : {}), last_text: cNeeds ? "核心·在等人" : "核心·在写" }),
    run("d", "done", { parent: "b" }),
    run("e", "done"),
  ];
  return { sid: SID, runs, ended: [], groups: { running: ["a"], recent: ["e"], older: [] }, summary: "核心·agent 5" };
}

const rows = (p: AgentsPanel) => [...p.pageElement.querySelectorAll<HTMLElement>(".agent-row")].map((r) => r.dataset.run);
const rowOf = (p: AgentsPanel, id: string) => p.pageElement.querySelector<HTMLElement>(`.agent-row[data-run="${id}"]`)!;
const fold = (p: AgentsPanel, id: string) => rowOf(p, id).querySelector<HTMLButtonElement>(".agent-fold");
const below = (p: AgentsPanel, id: string) => rowOf(p, id).querySelector(".agent-below")?.textContent ?? null;

function opened(t = table()): AgentsPanel {
  const p = new AgentsPanel();
  p.setVisible(true);
  p.setSession(SID, t);
  return p;
}

describe("agent 页按派出关系成树", () => {
  it("★ 第一层开着、第二层收着：第二层那一行写后端给的汇总，第三层不出行", () => {
    const p = opened();
    expect(rows(p)).toEqual(["a", "b", "e"]);
    expect(fold(p, "a")?.getAttribute("aria-expanded")).toBe("true");
    expect(fold(p, "b")?.getAttribute("aria-expanded")).toBe("false");
    expect(below(p, "b")).toBe("核心·b 底下");
    expect(below(p, "a"), "开着的那一行不写汇总").toBeNull();
    expect(fold(p, "e"), "没派过的那一行没有开合").toBeNull();
    expect(rowOf(p, "b").style.getPropertyValue("--agent-depth")).toBe("1");
  });

  it("★ 点开合：开第二层 ⇒ 第三层按后端给的先后出；收第一层 ⇒ 底下都不出、写汇总；点开合不开窗口那一下", () => {
    const p = opened();
    let openedRun: string | null = null;
    p.host = { open: (_s, r) => (openedRun = r), isOpen: () => false, liveOf: () => null };
    fold(p, "b")!.click();
    expect(rows(p)).toEqual(["a", "b", "c", "d", "e"]);
    expect(rowOf(p, "c").style.getPropertyValue("--agent-depth")).toBe("2");
    expect(below(p, "b")).toBeNull();
    fold(p, "a")!.click();
    expect(rows(p)).toEqual(["a", "e"]);
    expect(below(p, "a")).toBe("核心·a 底下");
    expect(openedRun).toBeNull();
  });

  it("★ 下层有事（在等人）不自动展开", () => {
    const p = opened();
    p.setSession(SID, table(true));
    expect(rows(p)).toEqual(["a", "b", "e"]);
    expect(fold(p, "b")?.getAttribute("aria-expanded")).toBe("false");
  });

  it("★ 不记住手动开合：页关了再开、换会话再回来 ⇒ 回默认", () => {
    const p = opened();
    fold(p, "b")!.click();
    fold(p, "a")!.click();
    p.setVisible(false);
    p.setVisible(true);
    expect(rows(p)).toEqual(["a", "b", "e"]);
    fold(p, "b")!.click();
    p.setSession("s2", table());
    p.setSession(SID, table());
    expect(rows(p)).toEqual(["a", "b", "e"]);
  });
});

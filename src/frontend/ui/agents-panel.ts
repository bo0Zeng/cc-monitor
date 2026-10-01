/**
 * issue #23：当前会话的子 agent 面板 —— status bar 一枚 chip（`N agents (M 在跑)`，0 个隐藏）＋ 点击展开 popover，
 * 与 task 面板同位同形态。子运行只在这里列，不进主 tab 的消息流。
 *
 * 数据是那台后端的运行表（`session_runs`，`runs.ts::RunBoard`）：标签 · 状态 · 最近一件事 · 派出它的那次工具调用。
 * **状态只读后端给的那一份**，面板不自己判（「有结果 ⇒ 完成」「会话不忙 ⇒ 中止」这类判断一条都没有）。
 *
 * 列哪些（有上界）：在跑的全列 ＋ 最近结束的 `RECENT_ENDED` 个；其余（含状态不明的）收进「更早的」，点开才列（`runs.ts::panelGroups`）。
 * 每行：图标 · [类别] · 标签 · 状态 · 最近：…；点一行就在它下面展开它的实时时间线（按运行读，宿主留着、续读），再点收起。
 *
 * UI 外壳复用 tasks-popover 的 CSS 类（同形态零新外壳样式）；行样式 .agent-* 自有。
 * 挂到 `#app` 里当 fixed popover。折叠状态写 localStorage（LS_KEYS.agentsPanelCollapsed，全局单例）。
 */

import { LS_KEYS, safeGet, safeSet } from "./local-storage";
import { copyText } from "./copy-table";
import type { RunInfo } from "./generated/RunInfo";
import { panelGroups, runLabel, runLastText, runStateIcon, runStateText, type LiveBlockView } from "./runs";

/** 宿主给面板的口：某个子运行的时间线（点开时要；宿主留着、续读）· 收起了 · 它此刻在生成的那一块。 */
export interface AgentsPanelHost {
  timeline(sid: string, run: string): HTMLElement;
  closed(sid: string, run: string): void;
  liveOf(sid: string, run: string): LiveBlockView | null;
}

function loadCollapsed(): boolean {
  return safeGet(LS_KEYS.agentsPanelCollapsed) !== "0";
}

export class AgentsPanel {
  /** 挂到 status-bar 里当 chip。click 切换 collapsed。 */
  readonly summaryElement: HTMLButtonElement;
  /** 挂到 #app 里当 fixed popover（复用 tasks-popover 外壳样式）。 */
  readonly popoverElement: HTMLElement;

  private summaryArrow: HTMLElement;
  private summaryText: HTMLElement;
  private list: HTMLUListElement;

  private runs: RunInfo[] = [];
  private activeSid: string | null = null;
  private collapsed: boolean;
  /** 「更早的」那一组展开着没有（换会话就收起）。 */
  private olderOpen = false;
  /** 点开着时间线的那几行（`sid\0run`）。 */
  private readonly open = new Set<string>();
  /** 上一次画出来的样子（行字没变就不重画：流里每段文字都会叫一次 `refresh`）。 */
  private drawn = "";
  /** main.ts 注入（时间线由 TabManager 建、留着）。 */
  host: AgentsPanelHost | null = null;

  constructor() {
    this.collapsed = loadCollapsed();

    this.summaryElement = document.createElement("button");
    this.summaryElement.type = "button";
    this.summaryElement.className = "status-tasks status-agents";
    this.summaryElement.style.display = "none";

    this.summaryArrow = document.createElement("span");
    this.summaryArrow.className = "status-tasks-arrow";
    this.summaryArrow.textContent = copyText("agentsPanel.arrow.collapsed");
    this.summaryElement.appendChild(this.summaryArrow);

    this.summaryText = document.createElement("span");
    this.summaryText.className = "status-tasks-text";
    this.summaryElement.appendChild(this.summaryText);

    this.summaryElement.addEventListener("click", () => this.setCollapsed(!this.collapsed));

    this.popoverElement = document.createElement("div");
    this.popoverElement.className = "tasks-popover agents-popover";
    this.popoverElement.style.display = "none";

    const popHead = document.createElement("div");
    popHead.className = "tasks-popover-head";
    const popTitle = document.createElement("span");
    popTitle.className = "tasks-popover-title";
    popTitle.textContent = "Agents";
    popHead.appendChild(popTitle);
    const closeBtn = document.createElement("button");
    closeBtn.type = "button";
    closeBtn.className = "tasks-popover-close";
    closeBtn.textContent = copyText("agentsPanel.ctor.close");
    closeBtn.title = copyText("agentsPanel.ctor.closeHint");
    closeBtn.addEventListener("click", () => this.setCollapsed(true));
    popHead.appendChild(closeBtn);
    this.popoverElement.appendChild(popHead);

    this.list = document.createElement("ul");
    this.list.className = "tasks-popover-list";
    this.popoverElement.appendChild(this.list);

    this.applyCollapsedArrow();
  }

  /** 当前会话与它的运行表（切 tab / 运行表到了时调）。 */
  setSession(sid: string | null, runs: RunInfo[]): void {
    if (sid !== this.activeSid) {
      this.closeAll();
      this.olderOpen = false;
    }
    this.activeSid = sid;
    this.runs = runs;
    // 不再在表里的那几行，时间线交还宿主。
    for (const k of [...this.open]) {
      const run = k.split("\u0000")[1];
      if (!runs.some((r) => r.run === run)) this.close(k);
    }
    this.render(true);
  }

  /** 某个子运行的流有动静：「最近：…」可能变了（字没变就不重画）。 */
  refresh(): void {
    this.render(false);
  }

  private setCollapsed(collapsed: boolean): void {
    this.collapsed = collapsed;
    safeSet(LS_KEYS.agentsPanelCollapsed, collapsed ? "1" : "0");
    this.applyCollapsedArrow();
    this.render(true);
  }

  private applyCollapsedArrow(): void {
    this.summaryArrow.textContent = this.collapsed ? copyText("agentsPanel.arrow.collapsed") : copyText("agentsPanel.arrow.expanded");
  }

  private keyOf(run: string): string {
    return `${this.activeSid ?? ""}\u0000${run}`;
  }

  private close(k: string): void {
    this.open.delete(k);
    const [sid, run] = k.split("\u0000");
    if (sid !== undefined && run !== undefined) this.host?.closed(sid, run);
  }

  private closeAll(): void {
    for (const k of [...this.open]) this.close(k);
  }

  private lastOf(r: RunInfo): string | null {
    const sid = this.activeSid;
    const live = r.state === "running" && sid !== null ? (this.host?.liveOf(sid, r.run) ?? null) : null;
    return runLastText(r, live);
  }

  private render(force: boolean): void {
    if (this.runs.length === 0 || this.activeSid === null) {
      this.summaryElement.style.display = "none";
      this.popoverElement.style.display = "none";
      this.drawn = "";
      return;
    }
    const g = panelGroups(this.runs);
    const shown = [...g.running, ...g.recent, ...(this.olderOpen ? g.older : [])];
    const sig = JSON.stringify([
      this.collapsed,
      this.olderOpen,
      [...this.open],
      g.older.length,
      shown.map((r) => [r.run, r.state, r.label, r.kind, this.lastOf(r)]),
    ]);
    if (!force && sig === this.drawn) return;
    this.drawn = sig;

    this.summaryElement.style.display = "";
    this.popoverElement.style.display = this.collapsed ? "none" : "";
    const running = g.running.length;
    this.summaryText.textContent =
      running > 0 ? copyText("agentsPanel.render.summary", { n: this.runs.length, running }) : `${this.runs.length} agents`;

    const items: HTMLElement[] = [];
    if (g.running.length > 0) items.push(this.head(copyText("agentsPanel.group.running")), ...g.running.flatMap((r) => this.row(r)));
    if (g.recent.length > 0) items.push(this.head(copyText("agentsPanel.group.recent")), ...g.recent.flatMap((r) => this.row(r)));
    if (g.older.length > 0) {
      const li = document.createElement("li");
      li.className = "agent-group agent-older";
      const b = document.createElement("button");
      b.type = "button";
      b.className = "agent-older-toggle";
      b.setAttribute("aria-expanded", this.olderOpen ? "true" : "false");
      b.textContent = copyText("agentsPanel.group.older", { n: g.older.length });
      b.addEventListener("click", () => {
        this.olderOpen = !this.olderOpen;
        if (!this.olderOpen) for (const r of g.older) if (this.open.has(this.keyOf(r.run))) this.close(this.keyOf(r.run));
        this.render(true);
      });
      li.appendChild(b);
      items.push(li);
      if (this.olderOpen) items.push(...g.older.flatMap((r) => this.row(r)));
    }
    this.list.replaceChildren(...items);
  }

  private head(text: string): HTMLElement {
    const li = document.createElement("li");
    li.className = "agent-group";
    li.textContent = text;
    return li;
  }

  /** 一行（点开着 ⇒ 下面紧跟它的时间线）。 */
  private row(r: RunInfo): HTMLElement[] {
    const k = this.keyOf(r.run);
    const row = document.createElement("li");
    row.className = `tasks-popover-item agent-row agent-${r.state} agent-row-clickable`;
    row.dataset.run = r.run;
    row.setAttribute("role", "button");
    row.tabIndex = 0;
    const toggle = (): void => {
      if (this.open.has(k)) this.close(k);
      else this.open.add(k);
      this.render(true);
    };
    row.addEventListener("click", toggle);
    row.addEventListener("keydown", (e) => {
      if (e.key === "Enter" || e.key === " ") {
        e.preventDefault();
        toggle();
      }
    });

    const span = (cls: string, text: string): void => {
      const s = document.createElement("span");
      s.className = cls;
      s.textContent = text;
      row.appendChild(s);
    };
    span("agent-icon", runStateIcon(r.state));
    if (r.kind) span("agent-type", r.kind);
    const label = runLabel(r);
    span("agent-label", label);
    span("agent-state", runStateText(r.state));
    const last = this.lastOf(r);
    if (last !== null) span("agent-last", copyText("agentsPanel.row.last", { last }));
    row.title = label;

    const sid = this.activeSid;
    if (!this.open.has(k) || sid === null || !this.host) return [row];
    row.setAttribute("aria-expanded", "true");
    const li = document.createElement("li");
    li.className = "agent-timeline";
    li.appendChild(this.host.timeline(sid, r.run));
    return [row, li];
  }
}

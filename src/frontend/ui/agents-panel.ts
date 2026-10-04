/**
 * issue #23：当前会话的子 agent 面板 —— status bar 一枚 chip（`N agents (M 在跑)`，0 个隐藏）＋ 点击展开 popover，
 * 与 task 面板同位同形态 ⇒ 两块同一时刻只开一块（`status-popovers.ts`）；看得见时在 Esc 弹层栈上。子运行只在这里列，不进主 tab 的消息流。
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
import { dispatcher } from "./keybindings/registry";
import { popoverExpanded, popoverShown } from "./status-popovers";
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
  /** 画出来的每一个列表项（组头 · 行 · 时间线 · 「更早的」）按身份留着：重画时原地改字、按需挪位，不整表重建 ⇒ 键盘焦点与时间线的滚动位置不丢。 */
  private readonly nodes = new Map<string, HTMLElement>();
  /** 「更早的」那一组此刻是谁（展开 / 收起那颗按钮点下去时读）。 */
  private older: RunInfo[] = [];
  /** main.ts 注入（时间线由 TabManager 建、留着）。 */
  host: AgentsPanelHost | null = null;
  /** 浮层此刻在 Esc 弹层栈上（⇔ 看得见）。 */
  private stacked = false;
  /** 跟着当前 tab 走、不盖住 tab 栏 ⇒ 不拦快捷键（与任务面板同）。 */
  readonly passes = "all" as const;

  constructor() {
    this.collapsed = loadCollapsed();
    popoverExpanded(this, !this.collapsed);

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
    const switched = sid !== this.activeSid;
    if (switched) {
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
    // 同一个会话的运行表又来一帧：样子没变就不动（换了会话才整份重画）。
    this.render(switched);
  }

  /** 某个子运行的流有动静：「最近：…」可能变了（字没变就不重画）。 */
  refresh(): void {
    this.render(false);
  }

  /** dispatcher overlay 接口（只在看得见时在栈上）。 */
  handleEsc(): void {
    this.setCollapsed(true);
  }

  /** 收起（状态栏另一块浮层开了 ⇒ 这一块让位）。 */
  collapse(): void {
    if (!this.collapsed) this.setCollapsed(true);
  }

  /** 弹层栈跟着「浮层此刻看得见」走（与任务面板同一条）。 */
  private syncLayer(): void {
    const shown = this.popoverElement.style.display !== "none";
    if (shown === this.stacked) return;
    this.stacked = shown;
    if (shown) {
      dispatcher.pushOverlay(this);
      popoverShown(this);
    } else {
      dispatcher.popOverlay(this);
    }
  }

  private setCollapsed(collapsed: boolean): void {
    this.collapsed = collapsed;
    safeSet(LS_KEYS.agentsPanelCollapsed, collapsed ? "1" : "0");
    popoverExpanded(this, !collapsed);
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
      this.syncLayer();
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
    this.syncLayer();
    const running = g.running.length;
    this.summaryText.textContent =
      running > 0 ? copyText("agentsPanel.render.summary", { n: this.runs.length, running }) : `${this.runs.length} agents`;

    const items: HTMLElement[] = [];
    if (g.running.length > 0) items.push(this.head("running", copyText("agentsPanel.group.running")), ...g.running.flatMap((r) => this.row(r)));
    if (g.recent.length > 0) items.push(this.head("recent", copyText("agentsPanel.group.recent")), ...g.recent.flatMap((r) => this.row(r)));
    this.older = g.older;
    if (g.older.length > 0) {
      items.push(this.olderToggle(g.older.length));
      if (this.olderOpen) items.push(...g.older.flatMap((r) => this.row(r)));
    }
    this.place(items);
  }

  /** 列表换成 `items` 这一串：留着的节点原地不动（只在次序不对时挪），没了的摘掉；焦点若被挪丢了还回去。 */
  private place(items: HTMLElement[]): void {
    const focused = document.activeElement;
    const hadFocus = focused instanceof HTMLElement && this.list.contains(focused);
    let cur: ChildNode | null = this.list.firstChild;
    for (const n of items) {
      if (n === cur) cur = cur.nextSibling;
      else this.list.insertBefore(n, cur);
    }
    while (cur) {
      const next: ChildNode | null = cur.nextSibling;
      cur.remove();
      cur = next;
    }
    const keep = new Set<HTMLElement>(items);
    for (const [id, el] of this.nodes) if (!keep.has(el)) this.nodes.delete(id);
    if (hadFocus && focused.isConnected && document.activeElement !== focused) focused.focus({ preventScroll: true });
  }

  /** 按身份取一个列表项（没有就建）。 */
  private node(id: string, make: () => HTMLElement): HTMLElement {
    let el = this.nodes.get(id);
    if (!el) {
      el = make();
      this.nodes.set(id, el);
    }
    return el;
  }

  private head(id: string, text: string): HTMLElement {
    const li = this.node(`head\u0000${id}`, () => {
      const el = document.createElement("li");
      el.className = "agent-group";
      return el;
    });
    if (li.textContent !== text) li.textContent = text;
    return li;
  }

  private olderToggle(n: number): HTMLElement {
    const li = this.node("older", () => {
      const el = document.createElement("li");
      el.className = "agent-group agent-older";
      const b = document.createElement("button");
      b.type = "button";
      b.className = "agent-older-toggle";
      b.addEventListener("click", () => {
        this.olderOpen = !this.olderOpen;
        if (!this.olderOpen) for (const r of this.older) if (this.open.has(this.keyOf(r.run))) this.close(this.keyOf(r.run));
        this.render(true);
      });
      el.appendChild(b);
      return el;
    });
    const b = li.firstElementChild as HTMLButtonElement;
    b.setAttribute("aria-expanded", this.olderOpen ? "true" : "false");
    b.textContent = copyText("agentsPanel.group.older", { n });
    return li;
  }

  /** 一行（点开着 ⇒ 下面紧跟它的时间线）。行节点按运行留着，字原地换。 */
  private row(r: RunInfo): HTMLElement[] {
    const k = this.keyOf(r.run);
    const row = this.node(`row\u0000${k}`, () => {
      const el = document.createElement("li");
      el.setAttribute("role", "button");
      el.tabIndex = 0;
      const toggle = (): void => {
        if (this.open.has(k)) this.close(k);
        else this.open.add(k);
        this.render(true);
      };
      el.addEventListener("click", toggle);
      el.addEventListener("keydown", (e) => {
        if (e.key === "Enter" || e.key === " ") {
          e.preventDefault();
          toggle();
        }
      });
      return el;
    });
    row.className = `tasks-popover-item agent-row agent-${r.state} agent-row-clickable`;
    row.dataset.run = r.run;

    const spans: HTMLElement[] = [];
    const span = (cls: string, text: string): void => {
      const s = document.createElement("span");
      s.className = cls;
      s.textContent = text;
      spans.push(s);
    };
    span("agent-icon", runStateIcon(r.state));
    if (r.kind) span("agent-type", r.kind);
    const label = runLabel(r);
    span("agent-label", label);
    span("agent-state", runStateText(r.state));
    const last = this.lastOf(r);
    if (last !== null) span("agent-last", copyText("agentsPanel.row.last", { last }));
    row.replaceChildren(...spans);
    row.title = label;

    const sid = this.activeSid;
    if (!this.open.has(k) || sid === null || !this.host) {
      row.removeAttribute("aria-expanded");
      return [row];
    }
    row.setAttribute("aria-expanded", "true");
    const li = this.node(`timeline\u0000${k}`, () => {
      const el = document.createElement("li");
      el.className = "agent-timeline";
      return el;
    });
    const tl = this.host.timeline(sid, r.run);
    if (tl.parentElement !== li) li.replaceChildren(tl);
    return [row, li];
  }
}

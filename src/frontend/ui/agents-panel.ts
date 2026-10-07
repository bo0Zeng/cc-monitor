/**
 * issue #23：当前会话的子 agent —— status bar 一枚 chip（`agent 2 · 1 在跑`，都结束了 `agent 3`，0 个不渲染），点了交宿主开 / 收底部抽屉的「agent」页。
 * 子运行只在这里列，不进主 tab 的消息流；这个会话没有 agent ⇒ 页里写空态。
 *
 * 数据是那台后端的运行表（`session_runs`，`runs.ts::RunBoard`）：标签 · 状态 · 最近一件事 · 派出它的那次工具调用。
 * **状态只读后端给的那一份**，面板不自己判（「有结果 ⇒ 完成」「会话不忙 ⇒ 中止」这类判断一条都没有）。
 *
 * 列哪些（有上界）：在跑的全列 ＋ 最近结束的 `RECENT_ENDED` 个；其余（含状态不明的）收进「更早的」，点开才列（`runs.ts::panelGroups`）。
 * 每行：图标 · [类别] · 标签 ·（「窗口已开」）· 状态 · 最近：…；点一行 ⇒ 开这个子运行自己的窗口（已开着 ⇒ 拉到前面，宿主管）。
 *
 * 行样式 .agent-* 自有。
 */

import { copyText } from "./copy-table";
import { chip } from "./kit/chip";
import type { IconName } from "./kit/icon";
import { emptyState } from "./kit/empty";
import { tag } from "./kit/badge";
import type { RunInfo } from "./generated/RunInfo";
import { panelGroups, runLabel, runLastText, runStateIcon, runStateText, type LiveBlockView } from "./runs";

/** 宿主给面板的口：开某个子运行的窗口 · 它的窗口开着没有 · 它此刻在生成的那一块。 */
export interface AgentsPanelHost {
  open(sid: string, run: string): void;
  isOpen(sid: string, run: string): boolean;
  liveOf(sid: string, run: string): LiveBlockView | null;
}

/** chip 前面那颗图标。 */
const CHIP_ICON: IconName = "agent";

export class AgentsPanel {
  /** 挂到 status-bar 里当 chip：点了交宿主（开 / 收底部抽屉的「agent」页）。 */
  readonly summaryElement: HTMLButtonElement;
  /** 底部抽屉「agent」页的内容。 */
  readonly pageElement: HTMLElement;
  /** chip 被点了（宿主接到抽屉上）。 */
  onChip: (() => void) | null = null;
  /** chip 上的字变了（宿主同步到抽屉页签）。 */
  onLabel: ((label: string) => void) | null = null;

  private summaryText: HTMLElement;
  private list: HTMLUListElement;
  private empty: HTMLElement;

  private runs: RunInfo[] = [];
  private activeSid: string | null = null;
  /** 「更早的」那一组展开着没有（换会话就收起）。 */
  private olderOpen = false;
  /** 上一次画出来的样子（行字没变就不重画：流里每段文字都会叫一次 `refresh`）。 */
  private drawn = "";
  /** 画出来的每一个列表项（组头 · 行 · 「更早的」）按身份留着：重画时原地改字、按需挪位，不整表重建 ⇒ 键盘焦点不丢。 */
  private readonly nodes = new Map<string, HTMLElement>();
  /** main.ts 注入（开窗口 · 窗口开着没有由 TabManager 管）。 */
  host: AgentsPanelHost | null = null;

  constructor() {
    this.summaryElement = chip({ text: "", icon: CHIP_ICON, onClick: () => this.onChip?.() }) as HTMLButtonElement;
    this.summaryElement.classList.add("status-agents");
    this.summaryElement.style.display = "none"; // 没有子 agent ⇒ 不渲染
    this.summaryElement.setAttribute("aria-expanded", "false");
    this.summaryText = this.summaryElement.querySelector("span") as HTMLElement;

    this.pageElement = document.createElement("div");
    this.pageElement.className = "agents-page";
    this.list = document.createElement("ul");
    this.list.className = "agents-list";
    this.empty = emptyState({ icon: "agent", text: copyText("agentsPanel.page.empty") });
    this.render(true);
  }

  /** 页签上的字（有 agent ⇒ 与 chip 同字；没有 ⇒ 页名）。 */
  get label(): string {
    return this.runs.length > 0 && this.activeSid !== null ? (this.summaryText.textContent ?? "") : copyText("agentsPanel.page.title");
  }

  /** 抽屉开到 / 离开这一页（宿主调）：chip 的「开着」跟着变。 */
  setVisible(on: boolean): void {
    this.summaryElement.setAttribute("aria-expanded", String(on));
  }

  /** 当前会话与它的运行表（切 tab / 运行表到了时调）。 */
  setSession(sid: string | null, runs: RunInfo[]): void {
    const switched = sid !== this.activeSid;
    if (switched) this.olderOpen = false;
    this.activeSid = sid;
    this.runs = runs;
    // 同一个会话的运行表又来一帧：样子没变就不动（换了会话才整份重画）。
    this.render(switched);
  }

  /** 某个子运行的流有动静：「最近：…」可能变了（字没变就不重画）。 */
  refresh(): void {
    this.render(false);
  }

  private keyOf(run: string): string {
    return `${this.activeSid ?? ""}\u0000${run}`;
  }

  private lastOf(r: RunInfo): string | null {
    const sid = this.activeSid;
    const live = r.state === "running" && sid !== null ? (this.host?.liveOf(sid, r.run) ?? null) : null;
    return runLastText(r, live);
  }

  private render(force: boolean): void {
    const none = this.runs.length === 0 || this.activeSid === null;
    if (none) {
      this.summaryElement.style.display = "none";
      if (this.pageElement.firstChild !== this.empty) this.pageElement.replaceChildren(this.empty);
      this.drawn = "";
      this.place([]);
      this.onLabel?.(this.label);
      return;
    }
    const g = panelGroups(this.runs);
    const shown = [...g.running, ...g.recent, ...(this.olderOpen ? g.older : [])];
    const sid = this.activeSid;
    const sig = JSON.stringify([
      this.olderOpen,
      shown.map((r) => (sid !== null && this.host?.isOpen(sid, r.run)) === true),
      g.older.length,
      shown.map((r) => [r.run, r.state, r.label, r.kind, this.lastOf(r)]),
    ]);
    if (!force && sig === this.drawn) return;
    this.drawn = sig;

    this.summaryElement.style.display = "";
    if (this.pageElement.firstChild !== this.list) this.pageElement.replaceChildren(this.list);
    const running = g.running.length;
    this.summaryText.textContent =
      running > 0 ? copyText("agentsPanel.render.summary", { n: this.runs.length, running }) : copyText("agentsPanel.render.summaryDone", { n: this.runs.length });
    this.onLabel?.(this.label);

    const items: HTMLElement[] = [];
    if (g.running.length > 0) items.push(this.head("running", copyText("agentsPanel.group.running")), ...g.running.flatMap((r) => this.row(r)));
    if (g.recent.length > 0) items.push(this.head("recent", copyText("agentsPanel.group.recent")), ...g.recent.flatMap((r) => this.row(r)));
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

  /** 一行。行节点按运行留着，字原地换。 */
  private row(r: RunInfo): HTMLElement[] {
    const k = this.keyOf(r.run);
    const row = this.node(`row\u0000${k}`, () => {
      const el = document.createElement("li");
      el.setAttribute("role", "button");
      el.tabIndex = 0;
      const go = (): void => {
        const sid = this.activeSid;
        if (sid !== null) this.host?.open(sid, r.run);
      };
      el.addEventListener("click", go);
      el.addEventListener("keydown", (e) => {
        if (e.key === "Enter" || e.key === " ") {
          e.preventDefault();
          go();
        }
      });
      return el;
    });
    row.className = `agent-row agent-${r.state} agent-row-clickable`;
    row.dataset.run = r.run;

    const spans: HTMLElement[] = [];
    const span = (cls: string, text: string): HTMLElement => {
      const s = document.createElement("span");
      s.className = cls;
      s.textContent = text;
      spans.push(s);
      return s;
    };
    span("agent-icon", runStateIcon(r.state));
    if (r.kind) span("agent-type", r.kind);
    const label = runLabel(r);
    span("agent-label", label);
    const sid = this.activeSid;
    const opened = sid !== null && this.host?.isOpen(sid, r.run) === true;
    if (opened) {
      const t = tag(copyText("agentsPanel.row.opened"));
      t.dataset.role = "window-open";
      spans.push(t);
    }
    span("agent-state", runStateText(r.state));
    const last = this.lastOf(r);
    if (last !== null) span("agent-last", copyText("agentsPanel.row.last", { last }));
    row.replaceChildren(...spans);
    row.title = opened ? copyText("agentsPanel.row.openedHint") : `${label}${copyText("kit.text.sep")}${copyText("agentsPanel.row.openHint")}`;
    return [row];
  }
}

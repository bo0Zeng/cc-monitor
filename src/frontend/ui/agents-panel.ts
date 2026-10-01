/**
 * issue #23（第二增量）：当前会话的 subagent 列表 + 每个 agent 自己的状态灯。
 *
 * 用户决策：不在 Tab 圆点上区分 agent，而是与 task 面板同位同形态——status bar
 * 一枚 chip（`N agents (M 运行中)`，0 agent 隐藏；F80 去纯装饰 🤖）+ 点击展开 popover，每行
 * 一个 agent：灯（🟢 运行中 / ✓ 完成 / ✗ 中止）+ [类型] 描述。
 *
 * 〔STC · `设计/90 §4` 阶段 C〕数据是后端的会话事实（`history-facts` 的 `agents`：后端读一遍文件配对 Task/Agent 的
 * tool_use（running）与 tool_result（done））；前端只做一件事：会话变 idle/归档时把仍
 * running 的标 aborted（ESC 打断/崩溃不会有 result，防僵尸绿灯 —— 那是事件，`tab-session-facts.ts::abortRunningAgents`）。
 *
 * UI 外壳复用 tasks-popover 的 CSS 类（同形态零新外壳样式）；行样式 .agents-* 自有。
 * 折叠状态写 localStorage（LS_KEYS.agentsPanelCollapsed，全局单例）。
 */

import { LS_KEYS, safeGet, safeSet } from "./local-storage";
import { copyText } from "./copy-table";

export interface AgentEntry {
  /** 派出它的那次工具调用 id（点进去按它读那个子运行，`history-run`） */
  id: string;
  /** 后端给的标签 */
  label: string;
  /** 后端给的类别，无则 null */
  agentType: string | null;
  status: "running" | "done" | "aborted";
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

  private agents: AgentEntry[] = [];
  private activeSid: string | null = null;
  private collapsed: boolean;
  /** F77：点某行 agent → 看它的记录。main.ts 注入（load_subagent → SessionViewer）。 */
  onAgentOpen: ((entry: AgentEntry) => void) | null = null;

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

    this.summaryElement.addEventListener("click", () =>
      this.setCollapsed(!this.collapsed),
    );

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

  /** 切换当前显示的 session（TabManager.switchTo / agents 变化时调）。 */
  setSession(sid: string | null, agents: AgentEntry[]): void {
    this.activeSid = sid;
    this.agents = agents;
    this.render();
  }

  private setCollapsed(collapsed: boolean): void {
    this.collapsed = collapsed;
    safeSet(LS_KEYS.agentsPanelCollapsed, collapsed ? "1" : "0");
    this.applyCollapsedArrow();
    this.render();
  }

  private applyCollapsedArrow(): void {
    this.summaryArrow.textContent = this.collapsed ? copyText("agentsPanel.arrow.collapsed") : copyText("agentsPanel.arrow.expanded");
  }

  private render(): void {
    if (this.agents.length === 0 || this.activeSid === null) {
      this.summaryElement.style.display = "none";
      this.popoverElement.style.display = "none";
      return;
    }
    this.summaryElement.style.display = "";
    this.popoverElement.style.display = this.collapsed ? "none" : "";

    const running = this.agents.filter((a) => a.status === "running").length;
    this.summaryText.textContent =
      running > 0
        ? copyText("agentsPanel.render.summary", { n: this.agents.length, running })
        : `${this.agents.length} agents`;

    // 全量 replace —— per-tab 上限 30 条（TabManager 侧裁剪）
    this.list.replaceChildren();
    for (const a of this.agents) {
      const row = document.createElement("li");
      row.className = `tasks-popover-item agent-row agent-${a.status} agent-row-clickable`;
      // F77：点整行 → 看该 agent 的记录（按运行读 → SessionViewer，main.ts 注入）。键盘可达：
      // role=button + tabindex + Enter/Space（DoD 要求）。
      row.setAttribute("role", "button");
      row.tabIndex = 0;
      row.addEventListener("click", () => this.onAgentOpen?.(a));
      row.addEventListener("keydown", (e) => {
        if (e.key === "Enter" || e.key === " ") {
          e.preventDefault();
          this.onAgentOpen?.(a);
        }
      });

      const dot = document.createElement("span");
      dot.className = "agent-dot";
      row.appendChild(dot);

      if (a.agentType) {
        const type = document.createElement("span");
        type.className = "agent-type";
        type.textContent = a.agentType;
        row.appendChild(type);
      }

      const label = document.createElement("span");
      label.className = "agent-label";
      label.textContent = a.label;
      row.appendChild(label);

      const state = document.createElement("span");
      state.className = "agent-state";
      state.textContent =
        a.status === "running" ? copyText("agentsPanel.status.running") : a.status === "done" ? copyText("agentsPanel.status.done") : copyText("agentsPanel.status.aborted");
      row.appendChild(state);

      row.title = a.label;
      this.list.appendChild(row);
    }
  }
}

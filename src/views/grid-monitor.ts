/**
 * F91（#27）：多 agent 并排**监控**——跨机器只读 mission-control 状态板。
 * 全屏 overlay（照 HistoryView/PanoramaView/UsageView 的 body-level fixed overlay 范式）。
 *
 * **只读 view，零后端、零写、零落盘**（守 INVARIANTS §1 只读铁律 + 北极星「不做驾驶舱」）：
 * 一屏 grid，一 cell/会话（本地 + 所有远端），按机器(origin)分组；每 cell 显红绿灯 / 标题 / cwd /
 * 运行中 subagent 数 / context% / unread / ⚙bg。**点 cell = switchTo 导航**（读侧安全），别的不做。
 * 数据全来自 `TabManager.snapshotSessions()`（纯派生 DTO）；overlay 开着时 1Hz 轮询重渲染
 * （偶尔开、快照纯内存、成本可忽略；push 订阅 = 后续精化）。
 *
 * 分组 / 排序 / 汇总是纯函数，抽出可测。
 */
import { dispatcher } from "../keybindings/registry";
import { activityLightClass, type GridSessionSnapshot } from "../session-status";

/** grid 数据源（TabManager 的只读子集——便于测试注入桩）。 */
export interface GridSource {
  snapshotSessions(): GridSessionSnapshot[];
  switchTo(sessionId: string): void;
}

export interface OriginGroup {
  origin: string | null;
  label: string;
  sessions: GridSessionSnapshot[];
}

/** 按机器(origin)分组：本机（origin=null）组恒在最前，远端组按 label 升序。组内保持输入序。纯函数。 */
export function groupSessionsByOrigin(sessions: GridSessionSnapshot[]): OriginGroup[] {
  const local: GridSessionSnapshot[] = [];
  const remotes = new Map<string, GridSessionSnapshot[]>();
  for (const s of sessions) {
    if (s.origin === null) {
      local.push(s);
    } else {
      const arr = remotes.get(s.origin);
      if (arr) arr.push(s);
      else remotes.set(s.origin, [s]);
    }
  }
  const groups: OriginGroup[] = [];
  if (local.length > 0) groups.push({ origin: null, label: "本机", sessions: local });
  for (const origin of [...remotes.keys()].sort((a, b) => a.localeCompare(b))) {
    groups.push({ origin, label: origin, sessions: remotes.get(origin)! });
  }
  return groups;
}

/** 组内排序优先级：活会话先于归档；活会话内 waiting（要你操作）> busy（运行中）> idle/shell > 未知。
 *  同档保持输入序（稳定）。纯函数——不改入参，返回新数组。 */
export function sortSessionsInGroup(sessions: GridSessionSnapshot[]): GridSessionSnapshot[] {
  const rank = (s: GridSessionSnapshot): number => {
    if (s.status === "archived") return 9;
    switch (s.activityStatus) {
      case "waiting":
        return 0;
      case "busy":
        return 1;
      case "idle":
      case "shell":
        return 2;
      default:
        return 3;
    }
  };
  return sessions
    .map((s, i) => ({ s, i }))
    .sort((a, b) => rank(a.s) - rank(b.s) || a.i - b.i)
    .map((x) => x.s);
}

export interface GridSummary {
  machines: number;
  liveSessions: number;
  runningAgents: number;
}

/** 顶部聚合摘要：机器数（distinct origin，本机算一台）/ 活会话数 / 运行中 agent 总数。纯函数。 */
export function summarizeSessions(sessions: GridSessionSnapshot[]): GridSummary {
  const origins = new Set<string | null>();
  let liveSessions = 0;
  let runningAgents = 0;
  for (const s of sessions) {
    origins.add(s.origin);
    if (s.status === "live") liveSessions += 1;
    runningAgents += s.runningAgents;
  }
  return { machines: origins.size, liveSessions, runningAgents };
}

export class GridMonitorView {
  private root: HTMLElement;
  private summaryEl!: HTMLElement;
  private bodyEl!: HTMLElement;
  private isOpen = false;
  private timer: ReturnType<typeof setInterval> | null = null;

  constructor(private source: GridSource) {
    this.root = this.build();
  }

  private build(): HTMLElement {
    const view = document.createElement("div");
    view.className = "grid-monitor";

    const bar = document.createElement("div");
    bar.className = "grid-monitor-bar";
    const back = document.createElement("button");
    back.type = "button";
    back.className = "grid-monitor-back";
    back.textContent = "← 返回";
    back.addEventListener("click", () => this.close());
    const title = document.createElement("span");
    title.className = "grid-monitor-title";
    title.textContent = "多 agent 监控";
    this.summaryEl = document.createElement("span");
    this.summaryEl.className = "grid-monitor-summary";
    bar.append(back, title, this.summaryEl);
    view.appendChild(bar);

    const note = document.createElement("div");
    note.className = "grid-monitor-note";
    note.textContent =
      "跨机器只读监控：一屏看所有会话的实时状态。点会话卡片跳到该会话。（只读——不派发/不驱动 agent。）";
    view.appendChild(note);

    this.bodyEl = document.createElement("div");
    this.bodyEl.className = "grid-monitor-body";
    view.appendChild(this.bodyEl);

    return view;
  }

  isVisible(): boolean {
    return this.isOpen;
  }

  handleEsc(): void {
    this.close();
  }

  open(): void {
    if (this.isOpen) return;
    document.body.appendChild(this.root);
    this.isOpen = true;
    dispatcher.pushOverlay(this);
    this.render();
    // overlay 开着时 1Hz 轮询快照重渲染（快照纯内存、N 会话小 DOM，成本可忽略）。
    this.timer = setInterval(() => this.render(), 1000);
  }

  close(): void {
    if (!this.isOpen) return;
    if (this.timer !== null) {
      clearInterval(this.timer);
      this.timer = null;
    }
    this.root.remove();
    this.isOpen = false;
    dispatcher.popOverlay(this);
  }

  private render(): void {
    const sessions = this.source.snapshotSessions();
    const summary = summarizeSessions(sessions);
    this.summaryEl.textContent =
      sessions.length === 0
        ? ""
        : `${summary.machines} 台机器 · ${summary.liveSessions} 个活跃会话 · ${summary.runningAgents} 个 agent 运行中`;

    this.bodyEl.replaceChildren();
    if (sessions.length === 0) {
      const empty = document.createElement("div");
      empty.className = "grid-monitor-empty";
      empty.textContent = "暂无会话。";
      this.bodyEl.appendChild(empty);
      return;
    }

    for (const group of groupSessionsByOrigin(sessions)) {
      const groupEl = document.createElement("div");
      groupEl.className = "grid-monitor-group";
      const gt = document.createElement("div");
      gt.className = "grid-monitor-group-title";
      gt.textContent = `${group.label}（${group.sessions.length}）`;
      groupEl.appendChild(gt);

      const grid = document.createElement("div");
      grid.className = "grid-monitor-grid";
      for (const s of sortSessionsInGroup(group.sessions)) {
        grid.appendChild(this.renderCell(s));
      }
      groupEl.appendChild(grid);
      this.bodyEl.appendChild(groupEl);
    }
  }

  private renderCell(s: GridSessionSnapshot): HTMLElement {
    const cell = document.createElement("button");
    cell.type = "button";
    cell.className = "grid-monitor-cell";
    if (s.status === "archived") cell.classList.add("archived");
    if (s.kind !== null && s.kind !== "interactive") cell.classList.add("cell-bg");

    // 头行：红绿灯点 + 标题
    const head = document.createElement("div");
    head.className = "grid-monitor-cell-head";
    const dot = document.createElement("span");
    dot.className = "live-dot";
    const light = activityLightClass(s.activityStatus);
    if (light) dot.classList.add(light);
    const name = document.createElement("span");
    name.className = "grid-monitor-cell-title";
    name.textContent = s.title;
    head.append(dot, name);
    cell.appendChild(head);

    // cwd（暗）
    if (s.cwd) {
      const cwd = document.createElement("div");
      cwd.className = "grid-monitor-cell-cwd";
      cwd.textContent = s.cwd;
      cwd.title = s.cwd;
      cell.appendChild(cwd);
    }

    // 徽标行：运行中 agent 数 / context% / unread
    const badges = document.createElement("div");
    badges.className = "grid-monitor-cell-badges";
    if (s.runningAgents > 0) {
      const b = document.createElement("span");
      b.className = "grid-monitor-badge badge-agents";
      b.textContent = `▶ ${s.runningAgents} agent`;
      b.title = `${s.runningAgents} 个 subagent 运行中（共 ${s.totalAgents}）`;
      badges.appendChild(b);
    }
    if (s.contextPct != null) {
      const rounded = Math.round(s.contextPct);
      const b = document.createElement("span");
      b.className = "grid-monitor-badge badge-ctx";
      if (rounded >= 80) b.classList.add("is-high");
      b.textContent = `ctx ${rounded}%`;
      b.title = "context 占用近似（最新一轮 prompt token ÷ 模型上限）";
      badges.appendChild(b);
    }
    if (s.unread > 0) {
      const b = document.createElement("span");
      b.className = "grid-monitor-badge badge-unread";
      b.textContent = s.unread > 99 ? "99+" : `${s.unread}`;
      b.title = `${s.unread} 条未读`;
      badges.appendChild(b);
    }
    if (s.activityStatus === "waiting" && s.waitingFor) {
      const b = document.createElement("span");
      b.className = "grid-monitor-badge badge-waiting";
      b.textContent = `等待：${s.waitingFor}`;
      b.title = `等待操作：${s.waitingFor}`;
      badges.appendChild(b);
    }
    if (badges.childElementCount > 0) cell.appendChild(badges);

    // 点击 → 导航到该会话（只读）
    cell.addEventListener("click", () => {
      this.source.switchTo(s.sessionId);
      this.close();
    });
    return cell;
  }
}

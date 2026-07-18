/**
 * F91（#27）：多 agent 并排**监控**——跨机器只读 mission-control 状态板。
 * 全屏 overlay（照 HistoryView/PanoramaView/UsageView 的 body-level fixed overlay 范式）。
 *
 * **只读 view，零后端、零写、零落盘**（守 INVARIANTS §1 只读铁律 + 北极星「不做驾驶舱」）：
 * 一屏 grid，一 cell/会话（本地 + 所有远端），按机器(origin)分组；每 cell 显红绿灯 / 标题 / cwd /
 * 运行中 subagent 数 / context% / unread / ⚙bg。**F91b：点 cell = 选中高亮 + 底部 peek 内容详情**
 * （板不关，连续 triage）；导航（switchTo+close）移到 peek 里「跳转到该会话」按钮。别的不做（读侧安全）。
 * 数据全来自 `TabManager.snapshotSessions()`（纯派生 DTO）+ 选中时 `peekSession()`；overlay 开着时 1Hz 轮询重渲染
 * （偶尔开、快照纯内存、成本可忽略；push 订阅 = 后续精化）。
 *
 * 分组 / 排序 / 汇总是纯函数，抽出可测。
 */
import { dispatcher } from "../keybindings/registry";
import {
  activityLightClass,
  type GridSessionSnapshot,
  type SessionPeek,
} from "../session-status";

/** grid 数据源（TabManager 的只读子集——便于测试注入桩）。 */
export interface GridSource {
  snapshotSessions(): GridSessionSnapshot[];
  switchTo(sessionId: string): void;
  /** F91b：选中 cell 的内容 peek 补充数据。**可选**——旧桩 {snapshotSessions,switchTo} 不破；
   *  缺省时 peek 降级只显 snapshot 字段。 */
  peekSession?(sessionId: string): SessionPeek | null;
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

/** F91b：peek 内容签名——把 peek 渲染依赖的所有字段序列化。相等 = 无需重建 DOM（保选区/滚动）。
 *  纯函数。selected=null → 空串（收起态）。 */
function peekSignature(selected: GridSessionSnapshot | null, peek: SessionPeek | null): string {
  if (!selected) return "";
  return JSON.stringify([
    selected.sessionId,
    selected.title,
    selected.origin,
    selected.cwd,
    selected.status,
    selected.activityStatus,
    selected.waitingFor,
    selected.contextPct,
    selected.unread,
    peek?.model ?? null,
    peek?.agents.map((a) => `${a.label}:${a.status}`) ?? null,
    peek?.recentFiles ?? null,
  ]);
}

export class GridMonitorView {
  private root: HTMLElement;
  private summaryEl!: HTMLElement;
  private bodyEl!: HTMLElement;
  private peekEl!: HTMLElement;
  private isOpen = false;
  private timer: ReturnType<typeof setInterval> | null = null;
  /** F91b：当前选中的会话（高亮 + peek）；null = 无选中（peek 收起）。 */
  private selectedId: string | null = null;
  /** F91b：上次 peek 渲染的内容签名——1Hz 重渲染下签名不变则跳过重建（保住选区/滚动）。 */
  private peekSig: string | null = null;

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
      "跨机器只读监控：一屏看所有会话的实时状态。点卡片看内容详情（板不关，可连续排查）；详情里「跳转」再切到该会话。（只读——不派发/不驱动 agent。）";
    view.appendChild(note);

    this.bodyEl = document.createElement("div");
    this.bodyEl.className = "grid-monitor-body";
    view.appendChild(this.bodyEl);

    // F91b：底部 peek 面板（选中一格时出内容详情；无选中时 .is-empty → 收起）。
    this.peekEl = document.createElement("div");
    this.peekEl.className = "grid-monitor-peek is-empty";
    view.appendChild(this.peekEl);

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
    this.selectedId = null; // 重开从干净态起（无残留选中/peek）
    this.peekSig = null; // 强制重开时 peek 重建（不被上次签名短路）
    dispatcher.popOverlay(this);
  }

  private render(): void {
    const sessions = this.source.snapshotSessions();
    const summary = summarizeSessions(sessions);
    this.summaryEl.textContent =
      sessions.length === 0
        ? ""
        : `${summary.machines} 台机器 · ${summary.liveSessions} 个活跃会话 · ${summary.runningAgents} 个 agent 运行中`;

    // F91b：选中的会话若已消失（归档移除/远端断线）→ 自动清选中、收 peek。
    const selected = this.selectedId
      ? (sessions.find((s) => s.sessionId === this.selectedId) ?? null)
      : null;
    if (this.selectedId && !selected) this.selectedId = null;

    this.bodyEl.replaceChildren();
    if (sessions.length === 0) {
      const empty = document.createElement("div");
      empty.className = "grid-monitor-empty";
      empty.textContent = "暂无会话。";
      this.bodyEl.appendChild(empty);
      this.renderPeek(null);
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

    this.renderPeek(selected); // 1Hz 也刷 peek（选中会话内容随之更新）
  }

  /** F91b：点 cell = 选中/取消选中（**不再** switchTo+close），高亮 + 出 peek；连续 triage。 */
  private select(sessionId: string): void {
    this.selectedId = this.selectedId === sessionId ? null : sessionId;
    this.render();
  }

  /** F91b：底部 peek 面板——选中会话的内容详情 + 「跳转到该会话」显式导航。null → 收起。
   *  **只在内容真变化时重建**（memo by 签名）：1Hz 重渲染下若选中会话内容未变则原样保留 DOM，
   *  否则每秒 replaceChildren 会清掉用户在 peek 里的文本选区/滚动位置（正是要读/复制文件路径时）。 */
  private renderPeek(selected: GridSessionSnapshot | null): void {
    const peek = selected ? (this.source.peekSession?.(selected.sessionId) ?? null) : null;
    const sig = peekSignature(selected, peek);
    if (sig === this.peekSig) return; // 内容未变 → 不动 DOM（保住选区/滚动）
    this.peekSig = sig;

    this.peekEl.replaceChildren();
    if (!selected) {
      this.peekEl.classList.add("is-empty");
      return;
    }
    this.peekEl.classList.remove("is-empty");

    // 头行：标题 +（远端）origin + 跳转/关闭
    const head = document.createElement("div");
    head.className = "grid-monitor-peek-head";
    const title = document.createElement("span");
    title.className = "grid-monitor-peek-title";
    title.textContent = selected.title;
    head.appendChild(title);
    if (selected.origin) {
      const org = document.createElement("span");
      org.className = "grid-monitor-peek-origin";
      org.textContent = selected.origin;
      head.appendChild(org);
    }
    const spacer = document.createElement("span");
    spacer.className = "grid-monitor-peek-spacer";
    head.appendChild(spacer);
    const jump = document.createElement("button");
    jump.type = "button";
    jump.className = "grid-monitor-peek-jump";
    jump.textContent = "跳转到该会话 →";
    jump.addEventListener("click", () => {
      this.source.switchTo(selected.sessionId); // 显式导航（旧 cell 点击语义搬到这）
      this.close();
    });
    const closeBtn = document.createElement("button");
    closeBtn.type = "button";
    closeBtn.className = "grid-monitor-peek-close";
    closeBtn.textContent = "✕";
    closeBtn.title = "收起预览";
    closeBtn.addEventListener("click", () => this.select(selected.sessionId)); // toggle off
    head.append(jump, closeBtn);
    this.peekEl.appendChild(head);

    // 事实行：cwd 全路径 / 活动态 / model / ctx% / unread
    const facts = document.createElement("div");
    facts.className = "grid-monitor-peek-facts";
    const addFact = (label: string, value: string): void => {
      const row = document.createElement("div");
      row.className = "grid-monitor-peek-fact";
      const k = document.createElement("span");
      k.className = "grid-monitor-peek-k";
      k.textContent = label;
      const v = document.createElement("span");
      v.className = "grid-monitor-peek-v";
      v.textContent = value;
      v.title = value;
      row.append(k, v);
      facts.appendChild(row);
    };
    if (selected.cwd) addFact("目录", selected.cwd);
    const act =
      selected.activityStatus === null
        ? "未知"
        : selected.waitingFor
          ? `${selected.activityStatus}（等待：${selected.waitingFor}）`
          : selected.activityStatus;
    addFact("状态", selected.status === "archived" ? `已归档 · ${act}` : act);
    if (peek?.model) addFact("模型", peek.model);
    if (selected.contextPct != null) addFact("context", `${Math.round(selected.contextPct)}%`);
    if (selected.unread > 0) addFact("未读", `${selected.unread}`);
    this.peekEl.appendChild(facts);

    // subagent 名单（运行中优先）
    if (peek && peek.agents.length > 0) {
      const agentsWrap = document.createElement("div");
      agentsWrap.className = "grid-monitor-peek-agents";
      const running = peek.agents.filter((a) => a.status === "running").length;
      const k = document.createElement("span");
      k.className = "grid-monitor-peek-k";
      k.textContent = `subagent（${running} 运行 / ${peek.agents.length} 共）`;
      agentsWrap.appendChild(k);
      for (const a of peek.agents.slice(0, 8)) {
        const chip = document.createElement("span");
        chip.className = `grid-monitor-peek-agent status-${a.status}`;
        chip.textContent = a.label;
        agentsWrap.appendChild(chip);
      }
      if (peek.agents.length > 8) {
        const more = document.createElement("span");
        more.className = "grid-monitor-peek-more";
        more.textContent = `+${peek.agents.length - 8}`;
        agentsWrap.appendChild(more);
      }
      this.peekEl.appendChild(agentsWrap);
    }

    // 改过的文件（「谁跑偏」信号）
    if (peek && peek.recentFiles.length > 0) {
      const filesWrap = document.createElement("div");
      filesWrap.className = "grid-monitor-peek-files";
      const k = document.createElement("span");
      k.className = "grid-monitor-peek-k";
      k.textContent = `改过的文件（${peek.recentFiles.length}）`;
      filesWrap.appendChild(k);
      const list = document.createElement("div");
      list.className = "grid-monitor-peek-filelist";
      if (peek.recentFiles.length > 8) {
        const more = document.createElement("span");
        more.className = "grid-monitor-peek-more";
        more.textContent = `+${peek.recentFiles.length - 8} 个更早改的（未列）`;
        list.appendChild(more);
      }
      for (const f of peek.recentFiles.slice(-8)) {
        const item = document.createElement("code");
        item.className = "grid-monitor-peek-file";
        item.textContent = f;
        item.title = f;
        list.appendChild(item);
      }
      filesWrap.appendChild(list);
      this.peekEl.appendChild(filesWrap);
    }
  }

  private renderCell(s: GridSessionSnapshot): HTMLElement {
    const cell = document.createElement("button");
    cell.type = "button";
    cell.className = "grid-monitor-cell";
    if (s.status === "archived") cell.classList.add("archived");
    if (s.kind !== null && s.kind !== "interactive") cell.classList.add("cell-bg");
    if (s.sessionId === this.selectedId) cell.classList.add("is-selected"); // F91b 选中高亮

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

    // F91b：点击 = 选中/取消选中（高亮 + peek，板保持开，连续 triage）；导航移到 peek 的「跳转」按钮。
    cell.addEventListener("click", () => this.select(s.sessionId));
    return cell;
  }
}

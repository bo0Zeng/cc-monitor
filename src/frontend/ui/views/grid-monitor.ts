/**
 * 多 agent 并排监控：跨机器只读状态板，全屏 overlay（与 HistoryView 同一种 body 级 fixed overlay）。
 *
 * 只读：零后端、零写、零落盘（INVARIANTS §1）。一会话一格（本机 ＋ 所有远端），按机器分组；每格红绿灯 / 标题 / cwd /
 * 运行中子 agent 数 / context% / 未读 / ⚙bg。点一格 = 选中高亮 ＋ 底部 peek 详情（板不关，连续看）；跳转在 peek 里的按钮上。
 * 数据来自 `TabManager.snapshotSessions()`（纯派生）＋ 选中时 `peekSession()`；开着时 1Hz 重渲染，按格子差量更新
 * （格子按 sid 留住、组按机器留住，没变的一拍零 DOM 写）。分组 / 排序 / 汇总是纯函数。
 */
import { dispatcher } from "../keybindings/registry";
import { isLocalOrigin, isRemoteOrigin, LOCAL_ORIGIN, type Origin } from "../ipc/origin";
import { sessionDot, type GridSessionSnapshot, type SessionPeek } from "../session-status";
import { isLive, isResumeOnly, stateView } from "../tab-session-state";
import { copyText } from "../copy-table";
import { setDot, statusDot } from "../kit/status-dot";

/** grid 数据源（TabManager 的只读子集——便于测试注入桩）。 */
export interface GridSource {
  snapshotSessions(): GridSessionSnapshot[];
  switchTo(sessionId: string): void;
  /** 选中一格时 peek 的补充数据。可选：缺省时 peek 只显 snapshot 字段。 */
  peekSession?(sessionId: string): SessionPeek | null;
}

export interface OriginGroup {
  origin: Origin;
  label: string;
  sessions: GridSessionSnapshot[];
}

/** 按机器(origin)分组：本机（`LOCAL_ORIGIN`）组恒在最前，远端组按 label 升序。组内保持输入序。纯函数。 */
export function groupSessionsByOrigin(sessions: GridSessionSnapshot[]): OriginGroup[] {
  const local: GridSessionSnapshot[] = [];
  const remotes = new Map<string, GridSessionSnapshot[]>();
  for (const s of sessions) {
    if (isLocalOrigin(s.origin)) {
      local.push(s);
    } else {
      const arr = remotes.get(s.origin);
      if (arr) arr.push(s);
      else remotes.set(s.origin, [s]);
    }
  }
  const groups: OriginGroup[] = [];
  if (local.length > 0) groups.push({ origin: LOCAL_ORIGIN, label: copyText("gridMonitor.groups.local"), sessions: local });
  for (const origin of [...remotes.keys()].sort((a, b) => a.localeCompare(b))) {
    groups.push({ origin, label: origin, sessions: remotes.get(origin)! });
  }
  return groups;
}

/** 组内排序优先级：活会话先于归档；活会话内 等人（要你操作）> 在干活 > 闲着 > 说不清。
 *  同档保持输入序（稳定）。纯函数——不改入参，返回新数组。 */
export function sortSessionsInGroup(sessions: GridSessionSnapshot[]): GridSessionSnapshot[] {
  const rank = (s: GridSessionSnapshot): number => {
    // 两轴：已结束（只能 resume）排最后；可重连（claude 退、tmux 在）排活会话之后、已结束之前。
    if (isResumeOnly(s.state)) return 9;
    if (s.state.liveness === "dead") return 8;
    switch (s.activity) {
      case "needs_you":
        return 0;
      case "working":
        return 1;
      case "idle":
      case "background_work":
        return 2;
      case null:
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
  const origins = new Set<Origin>();
  let liveSessions = 0;
  let runningAgents = 0;
  for (const s of sessions) {
    origins.add(s.origin);
    if (isLive(s.state)) liveSessions += 1; // 按活性：可重连的不算活
    runningAgents += s.runningAgents;
  }
  return { machines: origins.size, liveSessions, runningAgents };
}

/**
 * peek 内容签名：相等 = 不重建 DOM（保住选区 / 滚动）。纯函数；selected = null → 空串（收起态）。
 * - 不含 contextPct / unread：高频字段（每来一条消息就涨），纳入会让活跃会话每秒重建 peek、清掉正在复制的路径；
 * - 只签可见的那 8 条 ＋ 计数：渲染只画 slice(8)，签全量会比它保护的 DOM 还重。
 */
function peekSignature(selected: GridSessionSnapshot | null, peek: SessionPeek | null): string {
  if (!selected) return "";
  return JSON.stringify([
    selected.sessionId,
    selected.title,
    selected.origin,
    selected.cwd,
    selected.state.liveness, // 两轴都签：「状态」一格从两轴派生
    selected.state.recoverability,
    selected.activityText,
    selected.needs,
    peek?.model ?? null,
    peek?.agents.length ?? 0,
    peek?.agents.slice(0, 8).map((a) => `${a.label}:${a.status}`) ?? null,
    peek?.recentFiles.length ?? 0,
    peek?.recentFiles.slice(-8) ?? null,
  ]);
}

/** 一个格子留住的 DOM 引用 ＋ 它上次画出去的样子（按输出比，一样就不写）。 */
interface CellRefs {
  cell: HTMLButtonElement;
  dot: HTMLSpanElement;
  name: HTMLSpanElement;
  /** cwd 那一行；没有 cwd 时不在 DOM 里。 */
  cwd: HTMLDivElement | null;
  /** 徽标行；一个徽标都没有时不在 DOM 里（同上）。 */
  badges: HTMLDivElement | null;
  /** 徽标行上次画出去时的输入（`badgesInputs`）。 */
  badgesDrawn: string;
}

/** 一个机器分组留住的 DOM 引用。 */
interface GroupRefs {
  el: HTMLDivElement;
  title: HTMLDivElement;
  grid: HTMLDivElement;
}

/**
 * 徽标行画成什么样只取决于这几项（下面 `renderBadges` 逐字照原 `renderCell` 那一段）⇒ 拿它们当签名，
 * 一样就不重画。纯函数。
 */
function badgesInputs(s: GridSessionSnapshot): string {
  return [
    s.runningAgents,
    s.totalAgents,
    s.context?.text ?? "",
    s.context?.tone ?? "",
    s.unread,
    s.needs ?? "",
  ].join("\u0000");
}

/** 徽标行的内容（运行中 agent 数 / context% / unread / 等待）。原 `renderCell` 那一段，逐字。 */
function renderBadges(s: GridSessionSnapshot, badges: HTMLElement): void {
  if (s.runningAgents > 0) {
    const b = document.createElement("span");
    b.className = "grid-monitor-badge badge-agents";
    b.textContent = copyText("gridMonitor.renderBadges.agentsLabel", { runningAgents: s.runningAgents });
    b.title = copyText("gridMonitor.renderBadges.agents", { runningAgents: s.runningAgents, totalAgents: s.totalAgents });
    badges.appendChild(b);
  }
  if (s.context !== null) {
    // 字与语气照抄核心（上限判不出时核心只写用了多少、语气常规）。
    const b = document.createElement("span");
    b.className = "grid-monitor-badge badge-ctx";
    if (s.context.tone === "warn") b.classList.add("is-high");
    b.textContent = copyText("gridMonitor.renderBadges.ctx", { ctx: s.context.text });
    b.title = s.context.percent === null ? copyText("gridMonitor.renderBadges.ctxTokensHint") : copyText("gridMonitor.renderBadges.ctxHint");
    badges.appendChild(b);
  }
  if (s.unread > 0) {
    const b = document.createElement("span");
    b.className = "grid-monitor-badge badge-unread";
    b.textContent = s.unread > 99 ? "99+" : `${s.unread}`;
    b.title = copyText("gridMonitor.renderBadges.unread", { unread: s.unread });
    badges.appendChild(b);
  }
  if (s.needs !== null) {
    const b = document.createElement("span");
    b.className = "grid-monitor-badge badge-waiting";
    b.textContent = s.needs;
    b.title = s.needs;
    badges.appendChild(b);
  }
}

/** 组的留存键 —— 就是那台机器的 origin（本机是具名的 `LOCAL_ORIGIN`，与主机名撞不上：
 *  `"<local>"` 在全仓只指本机，`Origin::route` 就按它分本机）。 */
const groupKey = (origin: Origin): string => origin;

export class GridMonitorView {
  private root: HTMLElement;
  private summaryEl!: HTMLElement;
  private bodyEl!: HTMLElement;
  private peekEl!: HTMLElement;
  private isOpen = false;
  private timer: ReturnType<typeof setInterval> | null = null;
  /** 当前选中的会话（高亮 ＋ peek）；null = 无选中（peek 收起）。 */
  private selectedId: string | null = null;
  /** 上次 peek 渲染的内容签名 —— 签名不变则跳过重建（保住选区 / 滚动）。 */
  private peekSig: string | null = null;
  /** sid → 格子。格子跨拍留住，每拍只改变了的那几处。 */
  private readonly cells = new Map<string, CellRefs>();
  /** 机器 → 分组容器（键见 `groupKey`）。 */
  private readonly groups = new Map<string, GroupRefs>();
  /** 摘要那一行上次写的时候的数（`null` = 还没写过）。 */
  private summarySig: string | null = null;
  /** 空态那一行（建一次，没会话时挂上、有会话时摘掉）。 */
  private emptyEl: HTMLElement | null = null;

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
    back.textContent = copyText("gridMonitor.build.back");
    back.addEventListener("click", () => this.close());
    const title = document.createElement("span");
    title.className = "grid-monitor-title";
    title.textContent = copyText("gridMonitor.build.title");
    this.summaryEl = document.createElement("span");
    this.summaryEl.className = "grid-monitor-summary";
    bar.append(back, title, this.summaryEl);
    view.appendChild(bar);

    const note = document.createElement("div");
    note.className = "grid-monitor-note";
    note.textContent =
      copyText("gridMonitor.build.intro");
    view.appendChild(note);

    this.bodyEl = document.createElement("div");
    this.bodyEl.className = "grid-monitor-body";
    view.appendChild(this.bodyEl);

    // 底部 peek 面板（选中一格时出详情；无选中时 .is-empty → 收起）。
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
    // 调度：钟 —— 浮层开着时每秒按格重画（只读内存快照、不取数），关浮层即清
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
    // 摘要那一行也只在数变了时写。
    const summarySig = `${sessions.length}/${summary.machines}/${summary.liveSessions}/${summary.runningAgents}`;
    if (this.summarySig !== summarySig) {
      this.summarySig = summarySig;
      this.summaryEl.textContent =
        sessions.length === 0
          ? ""
          : copyText("gridMonitor.render.summary", { machines: summary.machines, liveSessions: summary.liveSessions, runningAgents: summary.runningAgents });
    }

    // 选中的会话若已消失（归档移除 / 远端断线）→ 清选中、收 peek。
    const selected = this.selectedId
      ? (sessions.find((s) => s.sessionId === this.selectedId) ?? null)
      : null;
    if (this.selectedId && !selected) this.selectedId = null;

    // 焦点在某一格上时，本拍之后要让它还在同一会话那一格上：被 `insertBefore` 挪了位置的那一格会丢焦点（浏览器的 focus fixup），丢了就按 sid 找回来。
    const active = document.activeElement;
    const focusedSid =
      active instanceof HTMLElement && this.bodyEl.contains(active) ? active.dataset.sid : undefined;

    // 先摘：本拍快照里没有了的格子、没有了的机器分组。
    const alive = new Set(sessions.map((s) => s.sessionId));
    for (const [sid, refs] of this.cells) {
      if (!alive.has(sid)) {
        refs.cell.remove();
        this.cells.delete(sid);
      }
    }
    const groups = sessions.length === 0 ? [] : groupSessionsByOrigin(sessions);
    const wantedGroups = new Set(groups.map((g) => groupKey(g.origin)));
    for (const [key, g] of this.groups) {
      if (!wantedGroups.has(key)) {
        g.el.remove();
        this.groups.delete(key);
      }
    }

    if (sessions.length === 0) {
      if (!this.emptyEl) {
        this.emptyEl = document.createElement("div");
        this.emptyEl.className = "grid-monitor-empty";
        this.emptyEl.textContent = copyText("gridMonitor.render.empty");
      }
      if (this.emptyEl.parentNode !== this.bodyEl) this.bodyEl.appendChild(this.emptyEl);
      this.renderPeek(null);
      return;
    }
    if (this.emptyEl?.parentNode === this.bodyEl) this.emptyEl.remove();

    // 再摆：组按 `groupSessionsByOrigin` 的顺序、格子按 `sortSessionsInGroup` 的顺序，
    // 「游标 ＋ 不在位才 `insertBefore`」（与 tab 栏同一个形状）⇒ 顺序没变的一拍零搬动。
    let groupCursor: ChildNode | null = null;
    for (const group of groups) {
      const g = this.groupFor(group.origin);
      const titleText = `${group.label}（${group.sessions.length}）`;
      if (g.title.textContent !== titleText) g.title.textContent = titleText;
      const groupAt: ChildNode | null = groupCursor ? groupCursor.nextSibling : this.bodyEl.firstChild;
      if (g.el !== groupAt) this.bodyEl.insertBefore(g.el, groupAt);
      groupCursor = g.el;

      let cellCursor: ChildNode | null = null;
      for (const s of sortSessionsInGroup(group.sessions)) {
        const refs = this.cellFor(s.sessionId);
        this.updateCell(refs, s);
        const cellAt: ChildNode | null = cellCursor ? cellCursor.nextSibling : g.grid.firstChild;
        if (refs.cell !== cellAt) g.grid.insertBefore(refs.cell, cellAt);
        cellCursor = refs.cell;
      }
    }

    // 恢复键盘焦点到本拍之前那一格（还在、且真丢了时）。preventScroll：维护性重聚焦不把格子滚回视口（否则滚动浏览时每秒被弹回）。
    if (focusedSid) {
      const refs = this.cells.get(focusedSid);
      if (refs && document.activeElement !== refs.cell) refs.cell.focus({ preventScroll: true });
    }

    this.renderPeek(selected); // 1Hz 也刷 peek（选中会话内容随之更新）
  }

  /** 某台机器的分组容器（没有就建一次）。 */
  private groupFor(origin: Origin): GroupRefs {
    const key = groupKey(origin);
    let g = this.groups.get(key);
    if (!g) {
      const el = document.createElement("div");
      el.className = "grid-monitor-group";
      const title = document.createElement("div");
      title.className = "grid-monitor-group-title";
      el.appendChild(title);
      const grid = document.createElement("div");
      grid.className = "grid-monitor-grid";
      el.appendChild(grid);
      g = { el, title, grid };
      this.groups.set(key, g);
    }
    return g;
  }

  /** 点一格 = 选中 / 取消选中，高亮 ＋ 出 peek（板不关）。 */
  private select(sessionId: string): void {
    this.selectedId = this.selectedId === sessionId ? null : sessionId;
    this.render();
  }

  /**
   * 底部 peek 面板：选中会话的详情 ＋「跳转到该会话」。null → 收起。
   * 只在签名变了时重建：每秒 replaceChildren 会清掉 peek 里的选区 / 滚动位置。
   */
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
    if (isRemoteOrigin(selected.origin)) {
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
    jump.textContent = copyText("gridMonitor.renderPeek.jump");
    jump.addEventListener("click", () => {
      this.source.switchTo(selected.sessionId); // 显式导航（旧 cell 点击语义搬到这）
      this.close();
    });
    const closeBtn = document.createElement("button");
    closeBtn.type = "button";
    closeBtn.className = "grid-monitor-peek-close";
    closeBtn.textContent = copyText("gridMonitor.renderPeek.close");
    closeBtn.title = copyText("gridMonitor.renderPeek.closeHint");
    closeBtn.addEventListener("click", () => this.select(selected.sessionId)); // toggle off
    head.append(jump, closeBtn);
    this.peekEl.appendChild(head);

    // 事实行：cwd 全路径 / 活动态 / model。
    // 不显 ctx% / 未读：选中那一格的徽标上就有，peek 里那份会因签名排除它俩而滞后、与徽标对不上。
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
    if (selected.cwd) addFact(copyText("gridMonitor.fact.dir"), selected.cwd);
    const doing = selected.activityText;
    const act =
      doing === null
        ? copyText("gridMonitor.renderPeek.unknown")
        : selected.needs
          ? copyText("gridMonitor.renderPeek.statusWaiting", { activityStatus: doing, needs: selected.needs })
          : doing;
    // 活着 ⇒ 活动状态；死了 ⇒ 状态名（已结束 / 可重连）—— 死会话的活动是陈旧的。
    addFact(copyText("gridMonitor.fact.status"), stateView(selected.state).name ?? act);
    if (peek?.model) addFact(copyText("gridMonitor.fact.model"), peek.model);
    this.peekEl.appendChild(facts);

    // subagent 名单（运行中优先）
    if (peek && peek.agents.length > 0) {
      const agentsWrap = document.createElement("div");
      agentsWrap.className = "grid-monitor-peek-agents";
      const running = peek.agents.filter((a) => a.status === "running").length;
      const k = document.createElement("span");
      k.className = "grid-monitor-peek-k";
      k.textContent = copyText("gridMonitor.renderPeek.agentsHead", { running, agentsCount: peek.agents.length });
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
      k.textContent = copyText("gridMonitor.renderPeek.filesHead", { recentFilesCount: peek.recentFiles.length });
      filesWrap.appendChild(k);
      const list = document.createElement("div");
      list.className = "grid-monitor-peek-filelist";
      if (peek.recentFiles.length > 8) {
        const more = document.createElement("span");
        more.className = "grid-monitor-peek-more";
        more.textContent = copyText("gridMonitor.renderPeek.filesMore", { more: peek.recentFiles.length - 8 });
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

  /** 某个会话的格子（没有就建一次：骨架 ＋ 点击监听；内容由 `updateCell` 填）。 */
  private cellFor(sessionId: string): CellRefs {
    let refs = this.cells.get(sessionId);
    if (!refs) {
      const cell = document.createElement("button");
      cell.type = "button";
      cell.className = "grid-monitor-cell";
      cell.dataset.sid = sessionId; // 焦点跨拍恢复用（render 按此比对）

      // 头行：红绿灯点 + 标题
      const head = document.createElement("div");
      head.className = "grid-monitor-cell-head";
      const dot = statusDot("running", "", "compact");
      const name = document.createElement("span");
      name.className = "grid-monitor-cell-title";
      head.append(dot, name);
      cell.appendChild(head);

      // 点击 = 选中 / 取消选中（高亮 ＋ peek，板不关）；跳转在 peek 的按钮上。
      cell.addEventListener("click", () => this.select(sessionId));
      refs = { cell, dot, name, cwd: null, badges: null, badgesDrawn: "\u0000none" };
      this.cells.set(sessionId, refs);
    }
    return refs;
  }

  /** 按输出比：每一处只在「要画的」与「已画的」不同时写。 */
  private updateCell(refs: CellRefs, s: GridSessionSnapshot): void {
    const { cell } = refs;
    // `toggle(x, 布尔)` 状态没变时不写 DOM（规范：force 与现状一致直接返回）。
    const view = stateView(s.state); // 类与灯只从两轴派生（与 tab 栏同一份）
    cell.classList.toggle("ended", view.ended);
    cell.classList.toggle("cell-bg", s.background);
    cell.classList.toggle("is-selected", s.sessionId === this.selectedId); // 选中高亮

    // 状态点：与标签栏同一个点（颜色 ＝ 核心的语气，形状 ＝ 两轴）；名字：活着照抄核心的字，否则那一态的名字。
    const dot = sessionDot(s.state, s.activityTone);
    const label = (isLive(s.state) ? s.activityText : null) ?? view.name ?? "";
    if (refs.dot.dataset.state !== dot || refs.dot.title !== label) setDot(refs.dot, dot, label);
    if (refs.name.textContent !== s.title) refs.name.textContent = s.title;

    // cwd（暗）：有才在 DOM 里，位置恒在头行之后。
    if (s.cwd) {
      if (!refs.cwd) {
        refs.cwd = document.createElement("div");
        refs.cwd.className = "grid-monitor-cell-cwd";
        cell.insertBefore(refs.cwd, cell.children[1] ?? null);
      }
      if (refs.cwd.textContent !== s.cwd) {
        refs.cwd.textContent = s.cwd;
        refs.cwd.title = s.cwd;
      }
    } else if (refs.cwd) {
      refs.cwd.remove();
      refs.cwd = null;
    }

    // 徽标行：整行作为一个单位比 —— 输入变了才重画，而且先在一个不挂 DOM 的新行里画好，
    // 再一次换上去（一次 `replaceWith` / `appendChild`）。一个徽标都没有 ⇒ 这一行不在 DOM 里。
    const drawn = badgesInputs(s);
    if (drawn === refs.badgesDrawn) return;
    refs.badgesDrawn = drawn;
    const badges = document.createElement("div");
    badges.className = "grid-monitor-cell-badges";
    renderBadges(s, badges);
    if (badges.childElementCount === 0) {
      refs.badges?.remove();
      refs.badges = null;
    } else if (refs.badges) {
      refs.badges.replaceWith(badges);
      refs.badges = badges;
    } else {
      cell.appendChild(badges);
      refs.badges = badges;
    }
  }
}

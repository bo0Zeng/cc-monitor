/**
 * 多 agent 并排监控：跨机器只读状态板，全屏 overlay（与 HistoryView 同一种 body 级 fixed overlay）。
 *
 * 只读：零写、零落盘（INVARIANTS §1）。一会话一格（本机 ＋ 所有远端）；每格红绿灯 / 标题 / cwd /
 * 运行中子 agent 数 / context% / 未读 / ⚙bg。点一格 = 选中高亮 ＋ 底部 peek 详情（板不关，连续看）；跳转在 peek 里的按钮上。
 * 版面（分组 · 组序 · 组头 · 组内先后 · 顶上那一句 · 没答的那台）照本机后端合好的那份 `MonitorBoard` 摆：开板问一次，
 * 之后宿主转来「标签页增减 · 脸变 · 运行表变 · 那台接上」才再问（`poke` 合批）。格子里的点 · 字 · 徽标照
 * `TabManager.snapshotSessions()`（与标签栏同一张脸）＋ 选中时 `peekSession()`；开着时 1Hz 只重画格子，按格子差量更新
 * （格子按 sid 留住、组按机器留住，没变的一拍零 DOM 写）。
 */
import { dispatcher } from "../keybindings/registry";
import type { Origin } from "../ipc/origin";
import type { GridSessionSnapshot, SessionPeek } from "../session-status";
import type { MonitorAsk } from "../generated/MonitorAsk";
import type { MonitorBoard } from "../generated/MonitorBoard";
import { backgroundLine } from "../session-face";
import { isEnded } from "../face-can";
import { copyText } from "../copy-table";
import { machineName } from "../control-said";
import { saidOf } from "../kit/detail";
import { setDot, statusDot } from "../kit/status-dot";
import { place } from "../kit/morph";

/** grid 数据源（TabManager 的只读子集 ＋ 版面那一问——便于测试注入桩）。 */
export interface GridSource {
  snapshotSessions(): GridSessionSnapshot[];
  switchTo(sessionId: string): void;
  /** 选中一格时 peek 的补充数据。可选：缺省时 peek 只显 snapshot 字段。 */
  peekSession?(sessionId: string): SessionPeek | null;
  /** 版面：交这一窗此刻列着的那几条，回本机后端合好的那一份（失败 ⇒ 抛，那一句已经说好）。 */
  board(picks: MonitorAsk[]): Promise<MonitorBoard>;
}

/** 连着来的几次「变了」合成一问。 */
const POKE_MS = 300;

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
    selected.face.name,
    selected.needs,
    peek?.model ?? null,
    peek?.summary ?? null,
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
  /** 那台没答那一句；答了 ⇒ 不在 DOM 里。 */
  down: HTMLDivElement | null;
  grid: HTMLDivElement;
}

/**
 * 徽标行画成什么样只取决于这几项（下面 `renderBadges` 逐字照原 `renderCell` 那一段）⇒ 拿它们当签名，
 * 一样就不重画。纯函数。
 */
function badgesInputs(s: GridSessionSnapshot, now: number): string {
  return [
    s.runningAgents,
    s.agents ?? "",
    s.context?.text ?? "",
    s.context?.tone ?? "",
    s.unread,
    s.needs ?? "",
    s.face.dot === "background" && s.backgroundWork ? `${s.backgroundWork.what ?? ""}\u0000${backgroundLine(s.backgroundWork, now)}` : "",
  ].join("\u0000");
}

/** 徽标行的内容（运行中 agent 数 / context% / unread / 等待 / 后台任务）。 */
function renderBadges(s: GridSessionSnapshot, badges: HTMLElement, now: number): void {
  if (s.runningAgents > 0) {
    const b = document.createElement("span");
    b.className = "grid-monitor-badge badge-agents";
    b.textContent = copyText("gridMonitor.renderBadges.agentsLabel", { runningAgents: s.runningAgents });
    if (s.agents !== null) b.title = s.agents; // 后端写好的那一句，照抄
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
    badges.appendChild(b);
  }
  // 后台任务运行中：徽标写命令那一格（核心写的），悬停给整句（时长那一截按此刻走）。
  const bg = s.face.dot === "background" ? s.backgroundWork : null;
  if (bg?.what) {
    const b = document.createElement("span");
    b.className = "grid-monitor-badge badge-bgwork one-line";
    b.textContent = copyText("gridMonitor.renderBadges.background", { cmd: bg.what });
    b.title = backgroundLine(bg, now);
    badges.appendChild(b);
  }
}

/** 组的留存键 —— 版面里那一组的 `machine`（本机是具名的 `LOCAL_ORIGIN`，与主机名撞不上：
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
  /** 空态那一行（建一次，没会话 · 头一问就失败时挂上，否则摘掉）。 */
  private emptyEl: HTMLElement | null = null;
  /** 本机后端最近答的那份版面；还没答过 ⇒ `null`（格子先不摆）。 */
  private board: MonitorBoard | null = null;
  /** 还没有一份版面时那一问失败的那一句；有版面 ⇒ `null`（留着上一份）。 */
  private failed: string | null = null;
  /** 第几问：叠着问只收最后一问。 */
  private seq = 0;
  private pokeTimer: ReturnType<typeof setTimeout> | null = null;

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
    void this.refresh();
    // overlay 开着时 1Hz 轮询快照重画格子（快照纯内存、N 会话小 DOM，成本可忽略；每秒重画时不问版面）。
    // 调度：钟 —— 浮层开着时每秒按格重画（只读内存快照、不取数），关浮层即清
    this.timer = setInterval(() => this.render(), 1000);
  }

  close(): void {
    if (!this.isOpen) return;
    if (this.timer !== null) {
      clearInterval(this.timer);
      this.timer = null;
    }
    if (this.pokeTimer !== null) {
      clearTimeout(this.pokeTimer);
      this.pokeTimer = null;
    }
    this.seq += 1; // 关着时回来的那一问不收
    this.root.remove();
    this.isOpen = false;
    this.selectedId = null; // 重开从干净态起（无残留选中/peek）
    this.peekSig = null; // 强制重开时 peek 重建（不被上次签名短路）
    dispatcher.popOverlay(this);
  }

  /** 宿主转来「标签页增减 · 脸变 · 运行表变 · 那台接上」：开着才问，连着来的合成一问。 */
  poke(): void {
    if (!this.isOpen || this.pokeTimer !== null) return;
    // 调度：合批 —— 连着来的标签页 · 脸 · 运行表变化 · 机器接上合成一问
    this.pokeTimer = setTimeout(() => {
      this.pokeTimer = null;
      void this.refresh();
    }, POKE_MS);
  }

  /** 问一次版面（开着才问；叠着问只收最后一问）。失败 ⇒ 留着上一份；一份都还没有 ⇒ 空态写那一句。 */
  async refresh(): Promise<void> {
    if (!this.isOpen) return;
    const mine = ++this.seq;
    const picks = this.source.snapshotSessions().map((s) => ({ machine: s.origin, sid: s.sessionId }));
    try {
      const b = await this.source.board(picks);
      if (mine !== this.seq || !this.isOpen) return;
      this.board = b;
      this.failed = null;
    } catch (e) {
      if (mine !== this.seq || !this.isOpen) return;
      this.failed = this.board === null ? saidOf(e) : null;
    }
    this.render();
  }

  private render(): void {
    const sessions = this.source.snapshotSessions();
    const bySid = new Map(sessions.map((s) => [s.sessionId, s]));
    const summaryText = sessions.length === 0 ? "" : (this.board?.summary ?? "");
    if (this.summaryEl.textContent !== summaryText) this.summaryEl.textContent = summaryText;

    // 选中的会话若已消失（归档移除 / 远端断线）→ 清选中、收 peek。
    const selected = this.selectedId ? (bySid.get(this.selectedId) ?? null) : null;
    if (this.selectedId && !selected) this.selectedId = null;

    // 版面照后端那一份；此刻快照里已没有的那几条不画（下一问的版面里也就没有了）。
    const groups = (sessions.length === 0 ? [] : (this.board?.groups ?? []))
      .map((g) => ({ g, shown: g.sids.flatMap((sid) => bySid.get(sid) ?? []) }))
      .filter((x) => x.shown.length > 0);

    // 先摘：这一份版面里没有了的格子、没有了的机器分组。
    const alive = new Set(groups.flatMap((x) => x.shown.map((s) => s.sessionId)));
    for (const [sid, refs] of this.cells) {
      if (!alive.has(sid)) {
        refs.cell.remove();
        this.cells.delete(sid);
      }
    }
    const wantedGroups = new Set(groups.map((x) => groupKey(x.g.machine)));
    for (const [key, g] of this.groups) {
      if (!wantedGroups.has(key)) {
        g.el.remove();
        this.groups.delete(key);
      }
    }

    const emptyText = sessions.length === 0 ? copyText("gridMonitor.render.empty") : this.board === null ? this.failed : null;
    if (emptyText !== null) {
      if (!this.emptyEl) {
        this.emptyEl = document.createElement("div");
        this.emptyEl.className = "grid-monitor-empty";
      }
      if (this.emptyEl.textContent !== emptyText) this.emptyEl.textContent = emptyText;
      place(this.bodyEl, [this.emptyEl]);
      this.renderPeek(null);
      return;
    }

    // 再摆：组 · 组头 · 组内先后照版面；按身份原地摆（kit `place`：
    // 顺序没变的一拍零搬动，挪过的那一格丢了焦点还回去，不把格子滚回视口）。
    const groupEls: HTMLElement[] = [];
    for (const { g: group, shown } of groups) {
      const g = this.groupFor(group.machine);
      if (g.title.textContent !== group.title) g.title.textContent = group.title;
      this.paintDown(g, group.down ?? null);
      groupEls.push(g.el);
      const cells: HTMLElement[] = [];
      for (const s of shown) {
        const refs = this.cellFor(s.sessionId);
        this.updateCell(refs, s);
        cells.push(refs.cell);
      }
      place(g.grid, cells);
    }
    place(this.bodyEl, groupEls);

    this.renderPeek(selected); // 1Hz 也刷 peek（选中会话内容随之更新）
  }

  /** 那台没答那一句：有才在 DOM 里，位置恒在组头之后。 */
  private paintDown(g: GroupRefs, said: string | null): void {
    if (said === null) {
      g.down?.remove();
      g.down = null;
      return;
    }
    if (!g.down) {
      g.down = document.createElement("div");
      g.down.dataset.down = ""; // 钩子：那台没答那一句
      g.el.insertBefore(g.down, g.grid);
    }
    if (g.down.textContent !== said) g.down.textContent = said;
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
      g = { el, title, down: null, grid };
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

    // 头行：标题 + 那台的名字 + 跳转/关闭
    const head = document.createElement("div");
    head.className = "grid-monitor-peek-head";
    const title = document.createElement("span");
    title.className = "grid-monitor-peek-title one-line";
    title.textContent = selected.title;
    head.appendChild(title);
    const org = document.createElement("span");
    org.className = "grid-monitor-peek-origin";
    org.textContent = machineName(selected.origin);
    head.appendChild(org);
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
      v.className = "grid-monitor-peek-v one-line";
      v.textContent = value;
      row.append(k, v);
      facts.appendChild(row);
    };
    if (selected.cwd) addFact(copyText("gridMonitor.fact.dir"), selected.cwd);
    const doing = selected.face.name;
    addFact(copyText("gridMonitor.fact.status"), selected.needs ? copyText("gridMonitor.renderPeek.statusWaiting", { activityStatus: doing, needs: selected.needs }) : doing);
    if (peek?.model) addFact(copyText("gridMonitor.fact.model"), peek.model);
    this.peekEl.appendChild(facts);

    // 子运行名单：照那台写好的三组序摆，头那一句照抄那台写好的
    if (peek && peek.agents.length > 0) {
      const agentsWrap = document.createElement("div");
      agentsWrap.className = "grid-monitor-peek-agents";
      const k = document.createElement("span");
      k.className = "grid-monitor-peek-k";
      k.textContent = peek.summary ?? "";
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
        item.className = "grid-monitor-peek-file one-line";
        item.textContent = f;
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
      name.className = "grid-monitor-cell-title one-line";
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
    cell.classList.toggle("ended", isEnded(s.face));
    cell.classList.toggle("cell-bg", s.background);
    cell.classList.toggle("is-selected", s.sessionId === this.selectedId); // 选中高亮

    // 状态点与名字：与标签栏同一张脸。
    const dot = s.face.dot;
    const label = s.face.name;
    if (refs.dot.dataset.state !== dot || refs.dot.title !== label) setDot(refs.dot, dot, label);
    if (refs.name.textContent !== s.title) refs.name.textContent = s.title;

    // cwd（暗）：有才在 DOM 里，位置恒在头行之后。
    if (s.cwd) {
      if (!refs.cwd) {
        refs.cwd = document.createElement("div");
        refs.cwd.className = "grid-monitor-cell-cwd one-line";
        cell.insertBefore(refs.cwd, cell.children[1] ?? null);
      }
      if (refs.cwd.textContent !== s.cwd) {
        refs.cwd.textContent = s.cwd;
      }
    } else if (refs.cwd) {
      refs.cwd.remove();
      refs.cwd = null;
    }

    // 徽标行：整行作为一个单位比 —— 输入变了才重画，而且先在一个不挂 DOM 的新行里画好，
    // 再一次换上去（一次 `replaceWith` / `appendChild`）。一个徽标都没有 ⇒ 这一行不在 DOM 里。
    const now = Date.now();
    const drawn = badgesInputs(s, now);
    if (drawn === refs.badgesDrawn) return;
    refs.badgesDrawn = drawn;
    const badges = document.createElement("div");
    badges.className = "grid-monitor-cell-badges";
    renderBadges(s, badges, now);
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

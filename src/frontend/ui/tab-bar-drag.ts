/**
 * tab 栏上的拖拽状态机：栏内拖动排序 · 中间停住建组 / 进组 · 拖进拖出组 · 拖出右缘松手撕成独立窗口 ·
 * 往上 / 下 / 左拖出栏或按 Esc 取消。
 *
 * 「落在哪」「落下后顺序与组怎么变」全在 `tab-drop.ts`（纯函数，判据直接打它）；这里只管量尺寸、挂 / 摘 document 监听、
 * 影子 · 插入线 · 目标框 · 小标签、停留计时器，以及松手那一拍把「顺序 ＋ 组」一次落实并给撤销。
 * 与 tab 栏视图的约定：**拖拽进行中不重排 tab 栏**（`deferRefresh`），被挡下的刷新收尾时补一次。
 */
import {
  autoScrollStep,
  defaultGroupName,
  dwellCandidate,
  keyGroupMoveTarget,
  keyJoinPrevTarget,
  keyLeaveTarget,
  keyMoveTarget,
  pickDropTarget,
  pickGroupDropTarget,
  planDrop,
  planGroupDrop,
  DRAG_THRESHOLD_PX,
  DWELL_MS,
  DWELL_MOVE_PX,
  type BarRow,
  type DropTarget,
  type RowRect,
} from "./tab-drop";
import { newCollectionId } from "./tab-collections";
import type { TabStore } from "./tab-store";
import { sayCollectionRefusal, type TabBarPrefs } from "./tab-bar-prefs";
import { copyText } from "./copy-table";
import { dispatcher, type OverlayHandle } from "./keybindings/registry";
import { statusDot } from "./kit/status-dot";
import { dotOf, stateWord, titleParts } from "./session-face";
import s from "./tab-group.module.css";

/** 拖拽要宿主做的三件事。 */
export interface TabBarDragHost {
  refreshTabBar(): void;
  /** 拖出右缘松手 ⇒ 在落点屏幕坐标开独立窗口（与右键「在新窗口打开」同一个动作）。 */
  openInNewWindow(sid: string, screenX?: number, screenY?: number): Promise<void>;
  /** 刚建的组：组头名字框立刻打开。 */
  renameGroupNow(gid: string): void;
  /** 按下的这一个在多选里（≥2）⇒ 选中的全部（按栏里的顺序）；否则只有它。 */
  selectedFor(sid: string): string[];
}

/** 拖拽要读的栏（`TabBarView` 的那几格）。 */
export interface TabBarDragView {
  readonly tabButtons: ReadonlyMap<string, { root: HTMLElement }>;
  readonly listEl: HTMLElement;
  measureRows(): RowRect[];
  /** 栏里从上到下的行（键盘挪位按看到的顺序算）。 */
  rows(): BarRow[];
  groupParts(gid: string): { wrap: HTMLElement; head: HTMLElement; list: HTMLElement } | undefined;
}

/** 拖出栏右缘多远算「松开在新窗口打开」。 */
const DETACH_PX = 16;
/** 组员缩进（与 `.tab-group-list` 的 margin-left 同值）：落在组里的插入线从这里起。 */
const GROUP_INDENT_PX = 12;
/** 影子离窗口底边至少留这么多（影子高 28 ＋ 余量）。 */
const GHOST_ROOM_PX = 34;
/** 插入线左端离列表左边（空心圆的圆心在这里）。 */
const LINE_INSET_PX = 6;
/** 组下沿那段空的一半：「组后面、组外」那条线画在空里，不压在组的最后一行上。 */
const AFTER_GROUP_PX = 3;
/** 组头上沿那段空的一半：「组前面、组外」那条线画在空里。 */
const BEFORE_GROUP_PX = 2;
/** 落下那几行底色淡出的时长（与 `tab-group.module.css` 的 `landed` 同值）。 */
const LANDED_MS = 600;

/** 拖整组之后要吞掉的那次组头 click 的键（与 sid 分开：组 id 与 sid 不同源）。 */
const groupClickKey = (gid: string): string => `\u0000group:${gid}`;

type DragState = {
  /** 按下的那一个（多选拖时是其中之一）；拖整组时是组 id。 */
  sid: string;
  /** 一起拖的标签页（多选 ⇒ 选中的全部，按栏里的顺序；拖整组 ⇒ 组员）。 */
  sids: string[];
  /** 拖的是整个组（组 id）；拖标签页 ⇒ `null`。 */
  group: string | null;
  /** 变暗的那几块（源行 / 整个组）。 */
  dim: HTMLElement[];
  /** 最近一次指针位置（自动滚的那几帧里没有 mousemove，照它重算落点）。 */
  lastX: number;
  lastY: number;
  /** 自动滚的那一帧（`null` ＝ 没在滚）。 */
  scrollRaf: number | null;
  /** 松手时的落点（只在既没 armed 也没 cancel 时有意义）。 */
  dropTarget: DropTarget;
  /** 停留正攒在谁身上（`null` ＝ 指针不在任何一行的中间一半）。 */
  dwellSid: string | null;
  /** 停留已经攒满的那一行。计时器到点才写，抖动 / 换目标即清回 `null`。 */
  dwellArmed: string | null;
  dwellX: number;
  dwellY: number;
  /** 停留计时器句柄。`teardownDrag` 必须清 —— 否则它会在拖拽结束后才到点。 */
  dwellTimer: number | null;
  startX: number;
  startY: number;
  bar: DOMRect;
  root: HTMLElement;
  dragging: boolean;
  armed: boolean;
  cancel: boolean;
  ghost: HTMLElement | null;
  ghostText: HTMLElement | null;
  line: HTMLElement | null;
  overlay: OverlayHandle | null;
  /**
   * 栏里每一行的矩形，起拖后第一次要用时量一次，之后 mousemove 只做算术（每次都量 ＝ 每次一次强制同步布局）。
   * 栏滚动 / 窗口尺寸变 ⇒ `onInvalidate` 置回 `null`，下一次 mousemove 重量；拖拽期间整刷挂起（`deferRefresh`）。
   * 目标框用 `outline`、不改盒模型 ⇒ 不会改到矩形。
   */
  rects: RowRect[] | null;
  onInvalidate: () => void;
  onMove: (e: MouseEvent) => void;
  onUp: (e: MouseEvent) => void;
};

export class TabBarDrag {
  /**
   * 拖拽状态。同一时刻只允许一个拖拽；`null` ＝ 当前无拖拽。
   * - mousedown（左键，非子动作按钮）记录起点 → 候选拖拽（dragging=false）
   * - document mousemove 越过 4px 阈值 → dragging=true，建影子、源行变暗
   * - 指针拖离竖栏右缘（clientX > 右缘 + 16）→ armed（松手即开独立窗口）
   * - 指针往上 / 下 / 左出栏 → cancel（松手什么都不变）；Esc 任何时候 ＝ 取消
   * - document mouseup：armed → openInNewWindow(落点)；cancel → 不变；否则落实落点。都抑制紧随的 click
   */
  private drag: DragState | null = null;
  /** 此刻画着的目标框 / 小标签 / 点亮的引导线（换落点只动旧的与新的）。 */
  private marked: { box: HTMLElement | null; pill: HTMLElement | null; hot: HTMLElement | null } = { box: null, pill: null, hot: null };
  /** 此刻画着的是哪个落点（同一个落点的 mousemove 不再碰 DOM；矩形作废时清回 `null` 重画）。 */
  private markedKey: string | null = "";
  /** 拖拽进行中有人要求刷 tab 栏 —— 记一笔，`teardownDrag` 收尾时补一次。 */
  private tabBarDirtyDuringDrag = false;
  /** 拖拽结束后要抑制掉紧随 mouseup 的那次 click 的 sid（拖完不切 Tab）；click handler 命中后清零。 */
  private suppressClickSid: string | null = null;

  constructor(
    private readonly store: TabStore,
    private readonly prefs: TabBarPrefs,
    private readonly barEl: HTMLElement,
    private readonly view: TabBarDragView,
    private readonly host: TabBarDragHost,
  ) {}

  /**
   * tab 栏视图每次要整刷之前先问这一句 —— **拖拽进行中（真起拖了）就别刷**，记一笔脏，收尾时补一次。
   * 守的是 `dragging`：按下还没动那一段照常刷新（与点击没有区别）。
   */
  deferRefresh(): boolean {
    if (this.drag?.dragging) {
      this.tabBarDirtyDuringDrag = true;
      return true;
    }
    return false;
  }

  /** 拖整组刚结束的那次 click 不收展、不改名。一次性消费。 */
  takeSuppressedHeadClick(gid: string): boolean {
    return this.takeSuppressedClick(groupClickKey(gid));
  }

  /** 拖拽刚结束的那次 click 不切 Tab（drag-then-release ≠ 选中）。一次性消费：命中返回 `true` 并清掉。 */
  takeSuppressedClick(sid: string): boolean {
    if (this.suppressClickSid === sid) {
      this.suppressClickSid = null;
      return true;
    }
    return false;
  }

  /** 拖拽起点（左键 mousedown）。只是候选：记录起点 ＋ 挂 document 级 mousemove/mouseup，越过阈值才真拖。 */
  begin(e: MouseEvent, sid: string, root: HTMLElement): void {
    this.start(e, sid, null, root);
  }

  /** 按住组头：候选拖整个组（越过阈值才真拖；没越 ⇒ 组头照常点）。 */
  beginGroup(e: MouseEvent, gid: string, head: HTMLElement): void {
    this.start(e, gid, gid, head);
  }

  private start(e: MouseEvent, sid: string, group: string | null, root: HTMLElement): void {
    if (this.drag) return;
    this.suppressClickSid = null;
    const onMove = (ev: MouseEvent): void => this.onDragMove(ev);
    const onUp = (ev: MouseEvent): void => this.onDragUp(ev);
    const onInvalidate = (): void => {
      if (this.drag) this.drag.rects = null;
      this.markedKey = null;
    };
    this.drag = {
      sid,
      sids: [],
      group,
      dim: [],
      lastX: e.clientX,
      lastY: e.clientY,
      scrollRaf: null,
      startX: e.clientX,
      startY: e.clientY,
      bar: this.barEl.getBoundingClientRect(),
      root,
      dragging: false,
      armed: false,
      cancel: false,
      dropTarget: { kind: "insert", at: null, gid: null },
      dwellSid: null,
      dwellArmed: null,
      dwellX: e.clientX,
      dwellY: e.clientY,
      dwellTimer: null,
      ghost: null,
      ghostText: null,
      line: null,
      overlay: null,
      rects: null,
      onInvalidate,
      onMove,
      onUp,
    };
    document.addEventListener("mousemove", onMove);
    document.addEventListener("mouseup", onUp);
    // `scroll` 不冒泡，捕获阶段挂在 `barEl` 上才收得到栏里任何一层滚动。
    this.barEl.addEventListener("scroll", onInvalidate, true);
    window.addEventListener("resize", onInvalidate);
  }

  /** document mousemove：阈值判定 → 起拖；随后跟随 · 判 armed / 出栏取消 · 停留 · 落点 · 画。 */
  private onDragMove(e: MouseEvent): void {
    const d = this.drag;
    if (!d) return;

    // 主键已松开（mouseup 在窗口外丢失）→ 收尾取消，不落实（落点不可信）。
    if ((e.buttons & 1) === 0) {
      const wasDragging = d.dragging;
      const key = d.group !== null ? groupClickKey(d.group) : d.sid;
      this.teardownDrag();
      if (wasDragging) this.suppressClickSid = key;
      return;
    }

    if (!d.dragging) {
      if (Math.hypot(e.clientX - d.startX, e.clientY - d.startY) <= DRAG_THRESHOLD_PX) return;
      e.preventDefault();
      this.startDragging(d);
    }

    if (d.ghost) {
      // 跟着指针、偏右下；贴着窗口底边时收到指针上方（拖出栏底取消时影子要看得见）。
      d.ghost.style.left = `${e.clientX + 8}px`;
      d.ghost.style.top = `${Math.min(e.clientY + 8, window.innerHeight - GHOST_ROOM_PX)}px`;
    }
    d.lastX = e.clientX;
    d.lastY = e.clientY;
    this.track(d);
  }

  /** 照指针此刻的位置判 armed / 出栏 / 落点并画；指针进栏顶 / 栏底那一段 ⇒ 栏自己滚（滚的那几帧里也走这里）。 */
  private track(d: DragState): void {
    const e = { clientX: d.lastX, clientY: d.lastY };

    // 撕成独立窗口只认一个标签页（拖整组 / 几个一起拖出右缘 ＝ 出栏取消）。
    const armed = d.group === null && d.sids.length <= 1 && e.clientX > d.bar.right + DETACH_PX;
    // 栏量不出尺寸（`width` 为 0：没布局的环境）⇒ 不判出栏。
    const outside = d.bar.width > 0 && (e.clientX < d.bar.left || e.clientX > d.bar.right + DETACH_PX || e.clientY < d.bar.top || e.clientY > d.bar.bottom);
    const cancel = !armed && outside;
    if (armed !== d.armed || cancel !== d.cancel) {
      d.armed = armed;
      d.cancel = cancel;
      this.paintGhost(d);
    }
    if (armed || cancel) {
      this.clearDwell(d);
      this.mark(null);
      this.autoScroll(d, 0);
      return;
    }
    const list = this.view.listEl.getBoundingClientRect();
    this.autoScroll(d, list.height > 0 ? autoScrollStep(e.clientY, list.top, list.bottom) : 0);
    if (d.group !== null) {
      // 拖整个组：只认组外，不攒停留（不合并、不嵌套）。
      d.dropTarget = { kind: "insert", at: pickGroupDropTarget(this.dragRects(), e.clientY, d.group), gid: null };
    } else {
      this.updateDwell(e.clientX, e.clientY);
      d.dropTarget = pickDropTarget(this.dragRects(), e.clientY, new Set(d.sids), d.dwellArmed);
    }
    this.mark(d.dropTarget);
  }

  /** 栏自己滚：每帧滚 `step`（0 ＝ 停）。滚了矩形作废，下一帧照最近的指针位置重算落点。 */
  private autoScroll(d: DragState, step: number): void {
    if (step === 0) {
      if (d.scrollRaf !== null) cancelAnimationFrame(d.scrollRaf);
      d.scrollRaf = null;
      return;
    }
    if (d.scrollRaf !== null) return;
    const frame = (): void => {
      const cur = this.drag;
      if (cur !== d) return;
      d.scrollRaf = null;
      const list = this.view.listEl;
      const before = list.scrollTop;
      const r = list.getBoundingClientRect();
      list.scrollTop += autoScrollStep(d.lastY, r.top, r.bottom);
      if (list.scrollTop === before) return; // 到头了：停（指针再动会再起）
      d.rects = null;
      this.markedKey = null;
      this.track(d);
    };
    // 调度：自链 —— 拖到栏顶 / 栏底那一段时每帧滚一下再排下一帧；指针离开那一段、滚到头或收尾时停
    d.scrollRaf = requestAnimationFrame(frame);
  }

  /** 越过阈值那一拍：源行变暗、建影子（状态点 ＋ 标题，同行上的写法）与插入线、接管 Esc。 */
  private startDragging(d: DragState): void {
    d.dragging = true;
    if (d.group !== null) {
      d.sids = this.store.orderedIds.filter((x) => this.store.tabs.get(x)?.group === d.group);
      const wrap = this.view.groupParts(d.group)?.wrap;
      d.dim = wrap ? [wrap] : [d.root];
    } else {
      d.sids = this.host.selectedFor(d.sid);
      d.dim = d.sids.map((x) => this.view.tabButtons.get(x)?.root).filter((x): x is HTMLElement => x !== undefined);
    }
    for (const el of d.dim) el.classList.add("dragging");
    const ghost = document.createElement("div");
    ghost.className = "tab-drag-ghost";
    // 几个一起拖：影子叠一层。
    if (d.group === null && d.sids.length > 1) ghost.classList.add(s.ghostStack);
    const tab = d.group === null ? this.store.tabs.get(d.sid) : undefined;
    if (tab) {
      const dot = dotOf(tab);
      ghost.appendChild(statusDot(dot, stateWord(tab), "compact"));
    }
    const text = document.createElement("span");
    ghost.appendChild(text);
    document.body.appendChild(ghost);
    d.ghost = ghost;
    d.ghostText = text;
    const line = document.createElement("div");
    line.className = s.dropLine;
    line.hidden = true;
    document.body.appendChild(line);
    d.line = line;
    // Esc 走弹层栈（不手搓 window 级 Esc 监听）：拖拽中按 Esc ＝ 取消。
    d.overlay = {
      handleEsc: () => {
        this.cancelDrag();
        return true;
      },
    };
    dispatcher.pushOverlay(d.overlay);
    this.paintGhost(d);
  }

  /** 影子的字与样子：照常（标题）· armed（松开在新窗口打开）· 出栏（松开取消，变灰）。 */
  private paintGhost(d: DragState): void {
    if (!d.ghost || !d.ghostText) return;
    d.ghost.classList.toggle("armed", d.armed);
    d.ghost.classList.toggle(s.ghostCancel, d.cancel);
    d.ghostText.textContent = d.armed && d.group === null
      ? copyText("tabBarDrag.onDragMove.detachHint")
      : d.cancel
        ? copyText("tabDrop.ghost.cancel")
        : this.ghostLabel(d);
  }

  /** 影子照常写什么：标题（几个一起 ⇒ 后面「+N」）/ 整组 ⇒「名字 · 组员数」。 */
  private ghostLabel(d: DragState): string {
    if (d.group !== null) {
      const col = this.prefs.collections.find((c) => c.id === d.group);
      return copyText("tabDrop.ghost.group", { name: col?.name ?? "", n: d.sids.length });
    }
    const tab = this.store.tabs.get(d.sid);
    const title = tab ? titleParts(tab).title : "";
    return d.sids.length > 1 ? copyText("tabDrop.ghost.many", { title, n: d.sids.length - 1 }) : title;
  }

  /** 拖拽期间用的矩形：缓存里有就用缓存，没有（刚起拖 / 刚过期）才量一次。 */
  private dragRects(): RowRect[] {
    const d = this.drag;
    if (!d) return this.view.measureRows();
    if (d.rects === null) d.rects = this.view.measureRows();
    return d.rects;
  }

  private clearDwell(d: DragState): void {
    if (d.dwellTimer !== null) window.clearTimeout(d.dwellTimer);
    d.dwellTimer = null;
    d.dwellSid = null;
    d.dwellArmed = null;
  }

  /**
   * 停留判定：指针在某行的**中间一半**里停 ≥ 400ms 且抖动 < 4px ⇒ 切进 `onto`（建组 / 进组）。
   * 用计时器，不在 `mousemove` 里数时间：指针停住之后 `mousemove` 就不来了。
   * 三种情况清零重来：换了一行 · 不在任何一行的中段 · 离锚点超过 `DWELL_MOVE_PX`。
   */
  private updateDwell(clientX: number, clientY: number): void {
    const d = this.drag;
    if (!d) return;
    const hovered = dwellCandidate(this.dragRects(), clientY, new Set([d.sid]));
    const moved = Math.hypot(clientX - d.dwellX, clientY - d.dwellY);
    if (hovered === d.dwellSid && moved < DWELL_MOVE_PX) return; // 还在攒，别打断计时
    this.clearDwell(d);
    d.dwellSid = hovered;
    d.dwellX = clientX;
    d.dwellY = clientY;
    if (hovered === null) return;
    // 调度：一次性 —— 指针停在一行的中间一半满 400ms 才切成「建组 / 进组」；换目标 / 抖动 / 收尾时清
    d.dwellTimer = window.setTimeout(() => {
      const cur = this.drag;
      if (!cur || cur.dwellTimer === null || cur.dwellSid !== hovered) return;
      cur.dwellTimer = null;
      cur.dwellArmed = hovered;
      // 攒满那一刻就给反馈（指针停着不动 ⇒ 不会再有 `mousemove` 来重算）。
      cur.dropTarget = { kind: "onto", sid: hovered };
      if (!cur.armed && !cur.cancel) this.mark(cur.dropTarget);
    }, DWELL_MS);
  }

  /**
   * 画落点（`null` ＝ 不画：armed / 出栏取消 / 收尾）：
   * - 插入：2px accent 线，落在组里从组员缩进处起、那个组的引导线点亮；落在组外通栏。
   * - 压住（`onto`）/ 落在组头：目标那一行 2px 外框 ＋ 底色 ＋ 右侧小标签（建分组 / 进「名字」）。
   */
  private mark(target: DropTarget | null): void {
    const key = target === null ? "" : JSON.stringify(target);
    if (key === this.markedKey) return;
    this.markedKey = key;
    let box: HTMLElement | null = null;
    let pillText: string | null = null;
    let hot: HTMLElement | null = null;
    let lineY: number | null = null;
    let lineGid: string | null = null;
    if (target?.kind === "onto") {
      box = this.view.tabButtons.get(target.sid)?.root ?? null;
      const gid = this.store.tabs.get(target.sid)?.group ?? null;
      const col = gid === null ? undefined : this.prefs.collections.find((c) => c.id === gid);
      pillText = col ? copyText("tabDrop.pill.join", { name: col.name }) : copyText("tabDrop.pill.found");
    } else if (target?.kind === "insert" && target.head !== undefined) {
      const parts = this.view.groupParts(target.head);
      box = parts?.head ?? null;
      const col = this.prefs.collections.find((c) => c.id === target.head);
      pillText = col ? copyText("tabDrop.pill.join", { name: col.name }) : null;
      hot = parts?.list ?? null;
    } else if (target?.kind === "insert") {
      const rows = this.dragRects();
      if (target.at === null) {
        const last = rows.filter((r) => r.height > 0).sort((a, b) => a.top - b.top).at(-1);
        lineY = last ? last.top + last.height : this.view.listEl.getBoundingClientRect().top;
      } else {
        const at = target.at;
        const r = rows.find((x) => x.kind === "tab" && x.id === at.sid);
        const head = r && r.gid !== null ? rows.find((x) => x.kind === "head" && x.id === r.gid) : undefined;
        const first = r && r.gid !== null ? rows.find((x) => x.kind === "tab" && x.gid === r.gid)?.id === r.id : false;
        if (r && head && target.gid === null && at.side === "before" && first) lineY = head.top - BEFORE_GROUP_PX; // 组前面（组外）：线在组头上面
        else if (r && head && r.height === 0) lineY = head.top + head.height + AFTER_GROUP_PX; // 挨着的那一行藏在收着的组里
        else if (r) lineY = at.side === "before" ? r.top : r.top + r.height + (r.gid !== null && target.gid === null ? AFTER_GROUP_PX : 0);
      }
      lineGid = target.gid;
      if (target.gid !== null) hot = this.view.groupParts(target.gid)?.list ?? null;
    }
    if (this.marked.box !== box) {
      this.marked.box?.classList.remove("drop-onto");
      box?.classList.add("drop-onto");
      this.marked.box = box;
    }
    this.marked.pill?.remove();
    this.marked.pill = null;
    if (box && pillText !== null) {
      const pill = document.createElement("span");
      pill.className = s.pill;
      pill.textContent = pillText;
      box.appendChild(pill);
      this.marked.pill = pill;
    }
    if (this.marked.hot !== hot) {
      this.marked.hot?.classList.remove(s.hot);
      hot?.classList.add(s.hot);
      this.marked.hot = hot;
    }
    const line = this.drag?.line;
    if (line) {
      line.hidden = lineY === null;
      if (lineY !== null) {
        const list = this.view.listEl.getBoundingClientRect();
        const left = list.left + (lineGid !== null ? GROUP_INDENT_PX : 0) + LINE_INSET_PX;
        line.style.left = `${left}px`;
        line.style.width = `${Math.max(0, list.right - 6 - left)}px`;
        line.style.top = `${lineY - 1}px`;
      }
    }
  }

  /**
   * 把一次落点的全部后果落实：顺序 ＋ 组，一拍做完；给撤销（改了分组 ⇒ toast；只排了顺序 ⇒ 不出条，`Ctrl+Z` 能撤）。
   * 建组 ⇒ 组就在目标那一格、名字框立刻打开。没拉过组表的实例（撕离出来的查看窗）⇒ 组一个字不动，顺序照常。
   */
  applyDrop(dragged: string | readonly string[], target: DropTarget): void {
    const sids = typeof dragged === "string" ? [dragged] : [...dragged];
    const sid = sids[0];
    const before = this.prefs.snapshot();
    const groupOf = (x: string): string | null => this.store.tabs.get(x)?.group ?? null;
    const known = new Set(this.prefs.collections.map((c) => c.id));
    const plan = planDrop(this.store.orderedIds, groupOf, known, sids, target);
    if (plan.order.length !== this.store.orderedIds.length) {
      this.host.refreshTabBar(); // 防御：被拖的 sid 已不在顺序里（拖拽中 tab 没了）
      return;
    }
    let title: string | null = null;
    let founded: string | null = null;
    const move = this.prefs.collectionsLoaded ? plan.move : ({ kind: "stay" } as const);
    if (move.kind === "found") {
      const name = defaultGroupName(
        this.store.tabs.get(move.with)?.projectDir ?? null,
        this.store.tabs.get(sid)?.projectDir ?? null,
        this.prefs.collections.map((c) => c.name),
      );
      const id = newCollectionId();
      const why = this.prefs.foundGroup([move.with, ...sids], name, id);
      if (why) sayCollectionRefusal(why);
      else {
        founded = id;
        title = copyText("tabBar.group.founded", { name });
      }
    } else if (move.kind === "join") {
      void this.prefs.joinGroupMany(sids, move.gid);
      const col = this.prefs.collections.find((c) => c.id === move.gid);
      if (col) title = sids.length > 1 ? copyText("tabBar.group.joinedMany", { name: col.name, n: sids.length }) : copyText("tabBar.group.joined", { name: col.name });
    } else if (move.kind === "leave") {
      const old = before.groups.find((c) => c.id === before.groupOf.get(sid));
      void this.prefs.leaveGroupMany(sids);
      if (sids.length > 1) title = copyText("tabBar.group.leftMany", { n: sids.length });
      else if (old) title = copyText("tabBar.group.left", { name: old.name });
    }
    const moved = plan.order.some((x, i) => x !== this.store.orderedIds[i]);
    if (moved) this.store.orderedIds = plan.order;
    this.host.refreshTabBar();
    if (moved) void this.prefs.persistOrder();
    if (title === null && !moved) return;
    this.prefs.offerUndo(title, before);
    this.flashLanded(sids);
    if (founded !== null) this.host.renameGroupNow(founded);
  }

  /** 整个组挪到 `target` 的位置（组外）：组员次序不变；给「已移动分组」可撤的 toast。 */
  applyGroupDrop(gid: string, target: DropTarget): void {
    if (target.kind !== "insert") return;
    const before = this.prefs.snapshot();
    const known = new Set(this.prefs.collections.map((c) => c.id));
    const next = planGroupDrop(this.store.orderedIds, (x) => this.store.tabs.get(x)?.group ?? null, known, gid, target.at);
    const moved = next.length === this.store.orderedIds.length && next.some((x, i) => x !== this.store.orderedIds[i]);
    if (moved) this.store.orderedIds = next;
    this.host.refreshTabBar();
    if (!moved) return;
    void this.prefs.persistOrder();
    const col = this.prefs.collections.find((c) => c.id === gid);
    this.prefs.offerUndo(copyText("tabBar.group.moved", { name: col?.name ?? "" }), before);
    this.flashLanded(this.store.orderedIds.filter((x) => this.store.tabs.get(x)?.group === gid));
  }

  /**
   * 键盘挪位（焦点在栏里 `Alt+↑↓`）：标签页（选了几个 ⇒ 一起）上 / 下一格；组头 ⇒ 整组一格。
   * 落点按看到的行算（`tab-drop.ts::keyMoveTarget` · `keyGroupMoveTarget`），落实与撤销同拖放。
   */
  keyMove(row: { kind: "tab"; sids: readonly string[] } | { kind: "head"; gid: string }, dir: -1 | 1): void {
    if (row.kind === "head") {
      const at = keyGroupMoveTarget(this.view.rows(), row.gid, dir);
      if (at !== undefined) this.applyGroupDrop(row.gid, { kind: "insert", at, gid: null });
      return;
    }
    const t = keyMoveTarget(this.view.rows(), row.sids, dir);
    if (t !== null) this.applyDrop(row.sids, t);
  }

  /** `Alt+←`：出组，放到组后面。 */
  keyLeave(sids: readonly string[]): void {
    const t = keyLeaveTarget(this.view.rows(), sids);
    if (t !== null) this.applyDrop(sids, t);
  }

  /** `Alt+→`：和上一行建组 / 进上一行的组。 */
  keyJoinPrev(sids: readonly string[]): void {
    const t = keyJoinPrevTarget(this.view.rows(), sids);
    if (t !== null) this.applyDrop(sids, t);
  }

  /** 右键 / 命令「新建分组」：建「分组 N」把这几个放进去（不问名字），名字框立刻打开；可撤。 */
  foundNow(sids: readonly string[]): void {
    if (!this.prefs.collectionsLoaded || sids.length === 0) return;
    const before = this.prefs.snapshot();
    const name = defaultGroupName(null, null, this.prefs.collections.map((c) => c.name));
    const id = newCollectionId();
    const why = this.prefs.foundGroup(sids, name, id);
    if (why) {
      sayCollectionRefusal(why);
      return;
    }
    this.host.refreshTabBar();
    this.prefs.offerUndo(copyText("tabBar.group.founded", { name }), before);
    this.host.renameGroupNow(id);
  }

  /** 右键 / 命令「分组 ▸ 某组」：这几个挪进那个组、排最后；可撤（同拖进组）。 */
  joinNow(sids: readonly string[], gid: string): void {
    const m = this.view.rows().filter((r): r is Extract<BarRow, { kind: "tab" }> => r.kind === "tab" && r.gid === gid);
    const last = m.filter((r) => !sids.includes(r.sid)).pop();
    this.applyDrop(sids, { kind: "insert", at: last ? { sid: last.sid, side: "after" } : null, gid });
  }

  /** 右键 / 命令「移出分组」：这几个出组、留在原处（组后面）；可撤。 */
  leaveNow(sids: readonly string[]): void {
    this.keyLeave(sids);
  }

  /** 落下的那几行底色淡出一次（只过渡底色；减少动效时不画，见 `tab-group.module.css`）。 */
  private flashLanded(sids: readonly string[]): void {
    for (const sid of sids) {
      const el = this.view.tabButtons.get(sid)?.root;
      if (!el) continue;
      el.classList.add(s.landed);
      // 调度：一次性 —— 落下那几行的淡出放完摘掉类
      window.setTimeout(() => el.classList.remove(s.landed), LANDED_MS);
    }
  }

  /** Esc：取消这次拖拽（什么都不变），抑制紧随的 click。 */
  private cancelDrag(): void {
    const d = this.drag;
    if (!d) return;
    const { sid, dragging, group } = d;
    this.teardownDrag();
    if (dragging) this.suppressClickSid = group !== null ? groupClickKey(group) : sid;
  }

  /** 收尾：拆 document listener、清影子 / 插入线 / 目标框 / 源行变暗 / 弹层、清空拖拽状态。 */
  private teardownDrag(): void {
    const d = this.drag;
    if (!d) return;
    document.removeEventListener("mousemove", d.onMove);
    document.removeEventListener("mouseup", d.onUp);
    this.barEl.removeEventListener("scroll", d.onInvalidate, true);
    window.removeEventListener("resize", d.onInvalidate);
    this.clearDwell(d);
    this.mark(null);
    d.ghost?.remove();
    d.line?.remove();
    if (d.overlay) dispatcher.popOverlay(d.overlay);
    if (d.scrollRaf !== null) cancelAnimationFrame(d.scrollRaf);
    d.root.classList.remove("dragging");
    for (const el of d.dim) el.classList.remove("dragging");
    this.drag = null;
    // 拖拽期间被守卫挡下的那些刷新，在这里补一次（必须在 `this.drag = null` 之后，否则被自己的守卫挡回去）。
    if (this.tabBarDirtyDuringDrag) {
      this.tabBarDirtyDuringDrag = false;
      this.host.refreshTabBar();
    }
  }

  /** document mouseup：收尾；armed ⇒ 在落点开独立窗口；出栏 ⇒ 取消；否则落实落点。 */
  private onDragUp(e: MouseEvent): void {
    const d = this.drag;
    if (!d) return;
    const { dragging, armed, cancel, sid, sids, group, dropTarget } = d;
    this.teardownDrag();
    if (!dragging) return; // 没越阈值 = 纯点击，交给 click handler 正常切 Tab / 收展组。
    this.suppressClickSid = group !== null ? groupClickKey(group) : sid;
    if (cancel) return;
    if (group !== null) {
      this.applyGroupDrop(group, dropTarget);
      return;
    }
    if (armed) {
      // 撕窗口这一路顺序不动。
      void this.host.openInNewWindow(sid, e.screenX, e.screenY);
      return;
    }
    this.applyDrop(sids, dropTarget);
  }
}

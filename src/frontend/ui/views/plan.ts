/**
 * **计划页**：主窗口上的全屏视图，跟历史页同形。
 *
 * 代码里片 · 块 · 格 ＝ pb 0.5 的仓库 · 权力范围 · 节点（给人看的字照 pb 的叫法，在文案表里）。
 * - 页头：所有仓库（同一工作区的挨着；别的机器的带机器徽标）· 按标题找 · 自动继续 · ［接过仓库…］（只做计划的）·［整张图］（pb render）· 返回。
 * - 子头：工作区 · project 的负责人 · 阶段 · 站在 · N 条不通过 · 判不了的节点 · 过滤三枚（全部 · 没签收的 · 需手动）。
 * - 左：大纲（只写标题；状态图标 · 类一道颜色 ＋ 一个词 · 行尾原因短词 / 负责人 / 阶段）；底下类的图例与圈。
 * - 右：没选节点 ⇒ 概览（权力范围 · 顶层进度）；选了 ⇒ 那个节点的详情（`plan-cell.ts`，没人接着的根节点带交接说明）。
 * - 新登记、还没有图的仓库 ⇒ 整页一屏，带 project 的交接说明（不带认领码，码找人要；本页不替人敲 `pb reissue`）。
 *
 * 数据：每台问 `plan-list`，选中的那一片所在的工作区问 `plan-read`；那台推 `plan-changed` ⇒ 摘要变了才重问。
 * 判定全在后端；这里只排版、只认最后一趟回答。
 */
import { openPath } from "@tauri-apps/plugin-opener";
import { button, setDisabled } from "../kit/button";
import { banner } from "../kit/banner";
import { morph } from "../kit/morph";
import { icon } from "../kit/icon";
import { foldCaret } from "../kit/fold";
import { emptyState } from "../kit/empty";
import { skeletonRows } from "../kit/skeleton";
import { toggleSwitch } from "../kit/switch";
import { attachTooltip } from "../kit/tooltip";
import { statusDot } from "../kit/status-dot";
import { askText, confirmDialog } from "../kit/dialog";
import { toast, failToast } from "../kit/toast";
import { dispatcher } from "../keybindings/registry";
import { copyText } from "../copy-table";
import { writeClipboard } from "../clipboard";
import { commands } from "../ipc/commands";
import { ChanError } from "../../../comms/inward/chan";
import { ControlError, machineName } from "../control-said";
import { saidOf } from "../kit/detail";
import { LOCAL_ORIGIN, isRemoteOrigin, type Origin } from "../ipc/origin";
import { LS_KEYS, safeGetJson, safeSetJson } from "../local-storage";
import type { Tab } from "../tab-model";
import { titleParts } from "../session-face";
import { ackNeed, adoptRepo, fetchPlanList, fetchPlanRead, planCommand, type PlanBlock, type PlanCell, type PlanNeed, type PlanList, type PlanRead, type PlanSlice, type PlanWho } from "../plan-reads";
import type { PlanMoved } from "../changed-stream";
import {
  blockRoots,
  cellIndex,
  isFresh,
  kindSlot,
  labelOf,
  outlineRows,
  shortId,
  ringLanes,
  ringRules,
  statusLook,
  topBlock,
  type PlanFilter,
  type RingRow,
} from "./plan-model";
import { CellDetail, handoffBox, takeoverCell, takeoverSpec, unbuiltTakeoverCell } from "./plan-cell";
import { cellTarget, needBar, openNeeds, openReturn, returnedBar, type ReviewHost } from "./plan-review";
import type { PlanNeedsBook } from "../plan-needs";
import type { PlanNeedItem } from "../needs-book";
import { freshPane, phaseBadge, setKindColor, STATUS_ICON } from "./plan-bits";
import { openNewSession } from "../new-session";
import { openSettingsWindow } from "../open-settings";
import { pbDirOf } from "../settings-dest";
import s from "./plan.module.css";

/** 记在本机的界面偏好（不是数据）。 */
interface Prefs {
  /** 上次看的那一片：`机器 \u0000 工作区 \u0000 片名`。 */
  last?: string;
  /** 手加的工作区目录（每台一份；`""` ＝ 本机）。 */
  dirs: Record<string, string[]>;
}

function loadPrefs(): Prefs {
  const v = safeGetJson<Partial<Prefs>>(LS_KEYS.planPrefs) ?? {};
  return { last: typeof v.last === "string" ? v.last : undefined, dirs: v.dirs && typeof v.dirs === "object" ? v.dirs : {} };
}

/** 一台此刻的样子。 */
type Machine =
  | { state: "loading"; prev: PlanList | null }
  | { state: "ok"; list: PlanList; at: number }
  | { state: "failed"; offline: boolean; prev: PlanList | null; at: number | null };

/** 页头一枚片：哪台 · 哪个工作区 · 叫什么。 */
interface SliceRef {
  origin: Origin;
  workspace: string;
  name: string;
}

const refKey = (r: SliceRef): string => `${r.origin}\u0000${r.workspace}\u0000${r.name}`;

/** 一个工作区读到的那一份（选中的片所在的那一个才读）。 */
type Read = { state: "loading"; prev: PlanRead | null } | { state: "ok"; doc: PlanRead } | { state: "failed"; prev: PlanRead | null };

export class PlanView {
  /** 主窗口里这个会话的标签页（拿标题、状态点）；不在 ⇒ `null`。 */
  tabOf: (sid: string) => Tab | null = () => null;
  /** 切到主窗口里的这个会话（并关掉计划页）。 */
  switchTo: (sid: string) => void = () => {};
  /** 那台离线 ⇒［重新连接］。 */
  reconnect: (origin: Origin) => void = () => {};
  /** 会话的［恢复 ▾］菜单（接手的会话停了那一条）。 */
  resume: (anchor: HTMLElement, sid: string) => void = () => {};
  /** 计划的账（开页先拿上一份画 · 会话头那一枚标）；问到的 `plan-list` 交给它。 */
  book: PlanNeedsBook | null = null;
  /** 「需手动」里的计划项（各台 `sessions-needs` 合好的那一份，`NeedsBook.plans()`）。 */
  needItems: () => readonly PlanNeedItem[] = () => [];
  /** 经 Ctrl J 打开时站在第几条计划项（`needItems()` 的下标）；用户自己点别处 ⇒ 清掉。 */
  private atItem: number | null = null;
  /** 打开某一条计划项之后、读回来之前：要选中那一片里第几条需手动。 */
  private pendingNeed: number | null = null;
  private readonly review: ReviewHost;

  private readonly root: HTMLElement;
  private isOpen = false;
  private prefs: Prefs = loadPrefs();
  private machines: Origin[] = [LOCAL_ORIGIN];
  private per = new Map<Origin, Machine>();
  private reads = new Map<string, Read>();
  private current: SliceRef | null = null;
  private filter: PlanFilter = "all";
  private query = "";
  private folded = new Set<string>();
  private selected: string | null = null;
  private seq = 0;
  private autoFail: string | null = null;
  private renderQueued = false;

  private headEl!: HTMLElement;
  private subEl!: HTMLElement;
  private stripsEl!: HTMLElement;
  private bodyEl!: HTMLElement;
  private searchInput!: HTMLInputElement;
  private detail: CellDetail;
  private readonly layer = { handleEsc: () => this.handleEsc() };

  constructor() {
    this.detail = new CellDetail({
      select: (id) => this.select(id),
      who: (w) => this.whoChip(w),
      origin: () => this.current?.origin ?? LOCAL_ORIGIN,
      workspace: () => this.current?.workspace ?? "",
      switchTo: (sid) => this.goSession(sid),
      returnCell: (slice, cell) => openReturn(this.review, slice, cellTarget(slice, cell)),
      takeover: (b) => this.takeover(b),
    });
    this.review = {
      origin: () => this.current?.origin ?? LOCAL_ORIGIN,
      workspace: () => this.current?.workspace ?? "",
      who: (w) => this.whoChip(w),
      sessionName: (w) => {
        const tab = w?.sid ? this.tabOf(w.sid) : null;
        return tab ? titleParts(tab).title : w ? shortId(w.id) : "";
      },
      switchTo: (sid) => this.goSession(sid),
      resume: (anchor, sid) => this.resume(anchor, sid),
      ack: (need) => void this.ack(need),
      next: () => this.nextNeed(),
      reread: () => {
        if (this.current) this.read(this.current.origin, this.current.workspace);
      },
    };
    this.root = this.build();
  }

  isVisible(): boolean {
    return this.isOpen;
  }

  async open(): Promise<void> {
    if (this.isOpen) return;
    document.body.appendChild(this.root);
    this.isOpen = true;
    dispatcher.pushOverlay(this.layer);
    this.render();
    this.machines = [LOCAL_ORIGIN, ...(await commands.list_remote_mcp_origins().catch(() => [] as string[]))];
    // 关着时「需手动」的账已问过各台：先拿那一份画（问回来再换；那台这会儿离线就留着它、整体变淡）。
    for (const m of this.machines) {
      const known = this.book?.list(m);
      if (known && !this.per.has(m)) this.per.set(m, { state: "ok", list: known, at: Date.now() });
    }
    this.refresh(true);
  }

  /** 开计划页并选中某一片的某一格（会话头那一枚标 · 文件窗口「在计划里看」）。 */
  async openAt(origin: Origin, workspace: string, slice: string, cell: string | null): Promise<void> {
    this.current = { origin, workspace, name: slice };
    this.selected = cell;
    this.atItem = null;
    // 已经开着（文件窗口那一下常是这样）⇒ 就地换到那一片那一格。
    if (this.isOpen) this.afterList();
    else await this.open();
  }

  /** 此刻站在第几条计划项（标签栏「需手动」的下一站从它算）；不在计划页 / 没站在哪一条 ⇒ `null`。 */
  needAt(): number | null {
    return this.isOpen ? this.atItem : null;
  }

  /** 去第 `i` 条计划项（`Ctrl J`）：开那一片、选中那一条所在的格。 */
  async openNeedItem(i: number): Promise<void> {
    const it = this.needItems()[i];
    if (!it) return;
    this.atItem = i;
    this.pendingNeed = it.k;
    this.current = { origin: it.origin, workspace: it.workspace, name: it.slice };
    this.selected = null;
    if (this.isOpen) this.afterList();
    else await this.open();
  }

  close(): void {
    if (!this.isOpen) return;
    this.seq++;
    this.root.remove();
    this.isOpen = false;
    this.autoFail = null;
    dispatcher.popOverlay(this.layer);
  }

  /** Esc 一次一层：搜索框有字 ⇒ 清空 ＞ 选着格 ⇒ 回概览 ＞ 关页。 */
  private handleEsc(): boolean {
    if (this.searchInput.value) {
      this.searchInput.value = "";
      this.query = "";
      this.render();
      return true;
    }
    if (this.selected !== null) {
      this.selected = null;
      this.render();
      return true;
    }
    this.close();
    return true;
  }

  /** 那台推来「这几个工作区的计划变了」（或整台可能漏了）。 */
  onChanged(origin: Origin, change: { moved: readonly PlanMoved[]; all: boolean }): void {
    if (!this.isOpen) return;
    const known = this.per.get(origin);
    const listed = known?.state === "ok" ? known.list : null;
    const stale = change.all || change.moved.some((m) => !listed?.workspaces.some((w) => w.workspace === m.workspace && w.rev === m.rev));
    if (stale) this.refresh(false, origin);
  }

  // ───────────────────────── 数据 ─────────────────────────

  private savePrefs(): void {
    safeSetJson(LS_KEYS.planPrefs, this.prefs);
  }

  /** 逐台问 `plan-list`（`fresh` ＝ 认过的目录也重问 pb）；哪台先答先画。之后读选中的那一片所在的工作区。 */
  private refresh(fresh: boolean, only?: Origin): void {
    const seq = ++this.seq;
    for (const m of only !== undefined ? [only] : this.machines) {
      const was = this.per.get(m);
      const prev = was?.state === "ok" ? was.list : (was?.prev ?? null);
      this.per.set(m, { state: "loading", prev });
      const dirs = this.prefs.dirs[isRemoteOrigin(m) ? m : ""] ?? [];
      fetchPlanList(m, fresh, dirs).then(
        (list) => {
          if (seq !== this.seq && only === undefined) return;
          this.per.set(m, { state: "ok", list, at: Date.now() });
          this.book?.keep(m, list);
          this.afterList();
        },
        (e: unknown) => {
          if (seq !== this.seq && only === undefined) return;
          const was2 = this.per.get(m);
          const keep = was2?.state === "loading" ? was2.prev : null;
          const lastAt = was?.state === "ok" ? was.at : was?.state === "failed" ? was.at : null;
          this.per.set(m, { state: "failed", offline: e instanceof ChanError && e.error.layer === "hop", prev: keep, at: lastAt });
          this.afterList();
        },
      );
    }
    this.render();
  }

  /** 页头那一排：每台每个工作区每一片（同一工作区挨着）。 */
  private allSlices(): { ref: SliceRef; list: PlanList; ws: PlanList["workspaces"][number]; slice: PlanList["workspaces"][number]["slices"][number]; down: boolean }[] {
    const out = [];
    for (const m of this.machines) {
      const st = this.per.get(m);
      const list = st?.state === "ok" ? st.list : (st?.prev ?? null);
      if (!list) continue;
      for (const ws of list.workspaces) for (const slice of ws.slices) out.push({ ref: { origin: m, workspace: ws.workspace, name: slice.name }, list, ws, slice, down: st?.state === "failed" });
    }
    return out;
  }

  private afterList(): void {
    const all = this.allSlices();
    if (!this.current || !all.some((x) => refKey(x.ref) === refKey(this.current!))) {
      const remembered = all.find((x) => refKey(x.ref) === this.prefs.last);
      const pick = remembered ?? all.find((x) => x.slice.current && !isRemoteOrigin(x.ref.origin)) ?? all[0];
      if (pick && this.everyMachineAnswered() !== false) this.current = pick.ref;
      else if (pick && !this.current) this.current = pick.ref;
    }
    if (this.current) {
      const ws = all.find((x) => refKey(x.ref) === refKey(this.current!))?.ws;
      const key = `${this.current.origin}\u0000${this.current.workspace}`;
      const r = this.reads.get(key);
      const have = r?.state === "ok" ? r.doc.rev : null;
      if (!r || (ws && have !== ws.rev && r.state !== "loading")) this.read(this.current.origin, this.current.workspace);
    }
    this.render();
  }

  private everyMachineAnswered(): boolean {
    return this.machines.every((m) => this.per.get(m)?.state !== "loading");
  }

  private read(origin: Origin, workspace: string): void {
    const key = `${origin}\u0000${workspace}`;
    const was = this.reads.get(key);
    const prev = was?.state === "ok" ? was.doc : (was?.prev ?? null);
    this.reads.set(key, { state: "loading", prev });
    const seq = this.seq;
    fetchPlanRead(origin, workspace).then(
      (doc) => {
        if (seq !== this.seq && !this.isOpen) return;
        this.reads.set(key, { state: "ok", doc });
        this.takePending();
        this.render();
      },
      () => {
        this.reads.set(key, { state: "failed", prev });
        this.render();
      },
    );
  }

  private currentRead(): Read | null {
    if (!this.current) return null;
    return this.reads.get(`${this.current.origin}\u0000${this.current.workspace}`) ?? null;
  }

  private currentDoc(): PlanRead | null {
    const r = this.currentRead();
    if (!r) return null;
    return r.state === "ok" ? r.doc : r.prev;
  }

  private currentSlice(): PlanSlice | null {
    const doc = this.currentDoc();
    return doc?.slices.find((x) => x.name === this.current?.name) ?? null;
  }

  private pick(ref: SliceRef): void {
    this.current = ref;
    this.prefs.last = refKey(ref);
    this.savePrefs();
    this.selected = null;
    this.folded.clear();
    this.autoFail = null;
    this.afterList();
  }

  private select(id: string | null): void {
    this.selected = id;
    this.atItem = null;
    this.render();
  }

  /** 经 Ctrl J 开的那一条：读回来了 ⇒ 选中它所在的格（顶块那一种没有格 ⇒ 概览）。 */
  private takePending(): void {
    const k = this.pendingNeed;
    const slice = this.currentSlice();
    if (k === null || !slice) return;
    this.pendingNeed = null;
    const n = openNeeds(slice).filter((x) => x.kind !== "ask")[k];
    const id = n?.cell ?? null;
    this.selected = id !== null && cellIndex(slice).has(id) ? id : null;
  }

  /** 这一片里下一条需手动（绕回第一条）。 */
  private nextNeed(): void {
    const slice = this.currentSlice();
    if (!slice) return;
    const all = openNeeds(slice);
    if (all.length === 0) return;
    const i = all.findIndex((n) => this.needHere(slice, n));
    const n = all[(i + 1) % all.length];
    const byId = cellIndex(slice);
    this.select(n.cell !== null && byId.has(n.cell) ? n.cell : null);
  }

  /** 这一条画在此刻这一页上：选着的格就是它的格；概览上画没有格可落的那几条（顶块那一种）。 */
  private needHere(slice: PlanSlice, n: PlanNeed): boolean {
    const has = n.cell !== null && cellIndex(slice).has(n.cell);
    return this.selected === null ? !has : n.cell === this.selected;
  }

  /** 认可一条：记上 ⇒ toast［撤销］8 秒 ⇒ 选下一条 ⇒ 重读。 */
  private async ack(need: PlanNeed): Promise<void> {
    const cur = this.current;
    const slice = this.currentSlice();
    if (!cur || !slice) return;
    try {
      await ackNeed(cur.origin, cur.workspace, slice.name, need.key, true);
    } catch (e) {
      failToast(copyText("plan.review.ackFailed"), e);
      return;
    }
    const what = need.cell ? labelOf(slice, need.cell) : slice.name;
    toast(copyText("plan.review.acked", { what }), "", {
      level: "success",
      action: {
        label: copyText("plan.review.undo"),
        run: () =>
          void ackNeed(cur.origin, cur.workspace, slice.name, need.key, false).then(
            () => this.read(cur.origin, cur.workspace),
            (e: unknown) => failToast(copyText("plan.review.ackFailed"), e),
          ),
      },
    });
    const rest = openNeeds(slice).filter((n) => n.key !== need.key);
    const n = rest[0];
    this.selected = n && n.cell !== null && cellIndex(slice).has(n.cell) ? n.cell : null;
    this.read(cur.origin, cur.workspace);
  }

  /** 此刻这一页顶上那几条：需手动（这一页的）· 退回过的那一格。 */
  private reviewBars(slice: PlanSlice, cell: PlanCell | undefined): HTMLElement[] {
    const all = openNeeds(slice);
    const out: HTMLElement[] = [];
    all.forEach((n, i) => {
      if (this.needHere(slice, n)) out.push(needBar(this.review, slice, n, { i, n: all.length }));
    });
    // 选了格 ⇒ 那一格退回过没有；概览 ⇒ 顶块退回过没有（记在片上）。
    const r = returnedBar(this.review, cell ? (cell.returned ?? null) : (slice.returned ?? null));
    if (r) out.push(r);
    return out;
  }

  /** ［新会话接手…］：全产品那一个新建会话框，机器锁这一片那台、目录锁在工作区根、第一句是那一块的交接说明。 */
  private takeover(b: PlanBlock): void {
    const cur = this.current;
    if (cur) void openNewSession(takeoverSpec(cur.origin, cur.workspace, b));
  }

  private goSession(sid: string): void {
    this.close();
    this.switchTo(sid);
  }

  // ───────────────────────── 外壳 ─────────────────────────

  private build(): HTMLElement {
    const root = document.createElement("div");
    root.className = `${s.pv} plan-view`;
    this.headEl = document.createElement("div");
    this.headEl.className = s.pvHead;
    this.subEl = document.createElement("div");
    this.subEl.className = s.pvSub;
    this.stripsEl = document.createElement("div");
    this.stripsEl.className = s.pvStrips;
    this.bodyEl = document.createElement("div");
    this.bodyEl.className = s.pvBody;
    this.searchInput = document.createElement("input");
    this.searchInput.type = "search";
    this.searchInput.placeholder = copyText("plan.page.search");
    this.searchInput.setAttribute("aria-label", copyText("plan.page.search"));
    this.searchInput.addEventListener("input", () => {
      this.query = this.searchInput.value.trim();
      this.renderBody();
    });
    root.append(this.headEl, this.subEl, this.stripsEl, this.bodyEl);
    return root;
  }

  private render(): void {
    if (!this.isOpen) return;
    if (this.renderQueued) return;
    this.renderQueued = true;
    queueMicrotask(() => {
      this.renderQueued = false;
      if (!this.isOpen) return;
      this.renderHead();
      this.renderSub();
      this.renderStrips();
      this.renderBody();
    });
  }

  private renderHead(): void {
    const title = document.createElement("div");
    title.className = s.pvTitle;
    title.append(icon("plan"), document.createTextNode(copyText("plan.page.title")));
    const slices = document.createElement("div");
    slices.className = s.pvSlices;
    for (const x of this.allSlices()) slices.appendChild(this.sliceChip(x.ref, x.slice, x.down));
    const search = document.createElement("label");
    search.className = s.pvSearch;
    search.append(icon("search", "compact"), this.searchInput);
    const parts: HTMLElement[] = [title, slices, spacer(), search];
    const ws = this.currentWs();
    if (ws) {
      parts.push(this.autoSwitch(ws));
      if (this.autoFail) {
        const f = document.createElement("span");
        f.className = s.pvAutoFail;
        f.textContent = copyText("plan.auto.failed", { reason: this.autoFail });
        parts.push(f);
      }
      const slice = this.currentSlice();
      if (slice?.docsOnly) parts.push(this.adoptButton(slice));
      parts.push(this.wholeButton());
    }
    parts.push(button({ label: copyText("plan.page.back"), kind: "ghost", onClick: () => this.close() }));
    this.headEl.replaceChildren(...parts);
  }

  private currentWs(): PlanList["workspaces"][number] | null {
    if (!this.current) return null;
    return this.allSlices().find((x) => refKey(x.ref) === refKey(this.current!))?.ws ?? null;
  }

  private sliceChip(ref: SliceRef, slice: PlanList["workspaces"][number]["slices"][number], down: boolean): HTMLElement {
    const b = document.createElement("button");
    b.type = "button";
    b.className = `${s.pvSlice} plan-slice`;
    b.setAttribute("aria-current", String(this.current !== null && refKey(ref) === refKey(this.current)));
    if (down) b.dataset.down = "true";
    if (isRemoteOrigin(ref.origin)) {
      const m = document.createElement("span");
      m.className = s.pvMachine;
      m.textContent = ref.origin;
      b.appendChild(m);
    }
    const name = document.createElement("span");
    name.className = s.pvSliceName;
    name.textContent = slice.name;
    b.appendChild(name);
    if (slice.domain) b.appendChild(document.createTextNode(slice.domain));
    if (slice.progress) {
      const n = document.createElement("span");
      n.className = s.pvNum;
      n.textContent = `${slice.progress.done}/${slice.progress.of}`;
      b.appendChild(n);
    }
    b.addEventListener("click", () => this.pick(ref));
    return b;
  }

  private autoSwitch(ws: PlanList["workspaces"][number]): HTMLElement {
    const sw = toggleSwitch({
      label: copyText("plan.auto.label"),
      on: ws.auto,
      onChange: async (on) => {
        const origin = this.current?.origin ?? LOCAL_ORIGIN;
        this.autoFail = null;
        try {
          await planCommand(origin, ws.workspace, on ? "continue" : "pause");
          this.refresh(false, origin);
          return true;
        } catch (e) {
          this.autoFail = saidOf(e);
          this.render();
          return false;
        }
      },
    });
    sw.root.classList.add(s.pvAuto);
    attachTooltip(sw.input, () => [copyText("plan.auto.scope", { ws: ws.workspace, list: ws.slices.map((x) => x.name).join(copyText("kit.text.sep")) }), copyText("plan.auto.cmds"), copyText("plan.auto.what")].join("\n"));
    return sw.root;
  }

  private wholeButton(): HTMLButtonElement {
    const remote = this.current !== null && isRemoteOrigin(this.current.origin);
    const b = button({
      label: copyText("plan.page.whole"),
      icon: "front",
      hint: copyText("plan.page.wholeHint"),
      onClick: () => void this.openWhole(b),
    });
    if (remote) setDisabled(b, copyText("plan.page.wholeRemote"));
    return b;
  }

  /** 只做计划的那个仓库：［接过仓库…］＝ 以人的身份代敲 pb 的 `adopt`（退不回 ⇒ 先问一句）；接过来的是哪一个照回包说。 */
  private adoptButton(slice: PlanSlice): HTMLButtonElement {
    const b = button({
      label: copyText("plan.adopt.open"),
      icon: "folder",
      hint: copyText("plan.adopt.hint", { path: slice.path ?? "" }),
      onClick: () => void this.adoptRepo(b, slice),
    });
    b.dataset.act = "adopt";
    return b;
  }

  private async adoptRepo(b: HTMLButtonElement, slice: PlanSlice): Promise<void> {
    const cur = this.current;
    if (!cur) return;
    const ok = await confirmDialog({
      title: copyText("plan.adopt.title", { slice: slice.name }),
      action: copyText("plan.adopt.action"),
      body: copyText("plan.adopt.body", { path: slice.path ?? "" }),
      note: slice.current ? undefined : copyText("plan.adopt.switchNote", { slice: slice.name }),
    });
    if (!ok) return;
    b.disabled = true;
    try {
      const r = await adoptRepo(cur.origin, cur.workspace, slice.name);
      const got = r.adopted;
      toast(copyText("plan.adopt.done", { slice: got.slice, path: got.path }), got.switched ? copyText("plan.adopt.switched", { slice: got.slice }) : "", { level: "success" });
      this.refresh(false, cur.origin);
    } catch (e) {
      failToast(copyText("plan.adopt.failed"), e);
    } finally {
      b.disabled = false;
    }
  }

  private async openWhole(b: HTMLButtonElement): Promise<void> {
    const cur = this.current;
    if (!cur) return;
    b.disabled = true;
    try {
      const r = await planCommand(cur.origin, cur.workspace, "render");
      if (!r.path) throw new ControlError(r.said ?? "");
      await openPath(r.path);
      toast(copyText("plan.page.wholeOpened"), "", { level: "success" });
    } catch (e) {
      failToast(copyText("plan.page.wholeFailed"), e);
    } finally {
      b.disabled = false;
    }
  }

  private renderSub(): void {
    const slice = this.currentSlice();
    const cur = this.current;
    if (!slice || !cur || slice.bare) {
      this.subEl.hidden = true;
      return;
    }
    this.subEl.hidden = false;
    const parts: (HTMLElement | Text)[] = [];
    const where = document.createElement("span");
    where.textContent = joinSep(machineName(cur.origin), cur.workspace);
    // 无图：子头只留在哪（没有顶块可写、没有格可过滤）。
    if (isFresh(slice)) {
      this.subEl.replaceChildren(where);
      return;
    }
    parts.push(where);
    if (slice.docsOnly) parts.push(this.docsOnlyBadge(slice));
    parts.push(sep());
    const top = topBlock(slice);
    if (top) {
      parts.push(document.createTextNode(copyText("plan.sub.top")));
      if (top.owner) parts.push(this.whoChip(top.owner));
      if (top.phase) parts.push(phaseBadge(top.phase));
      const at = top.at ? cellIndex(slice).get(top.at) : null;
      if (at) parts.push(document.createTextNode(copyText("plan.sub.standing", { title: labelOf(slice, at.id) })));
    }
    if (slice.check.red.length > 0) {
      const red = document.createElement("span");
      red.className = s.pvRed;
      red.append(icon("warning", "compact"), document.createTextNode(copyText("plan.sub.red", { n: slice.check.red.length })));
      parts.push(red);
    }
    // 元数据坏了、编号还认得出的节点：pb 判不了，不在大纲里 ⇒ 子头照编号列出来。
    if (slice.check.unparsed.length > 0) {
      const un = document.createElement("span");
      un.className = `${s.pvRed} plan-unparsed`;
      un.append(icon("warning", "compact"), document.createTextNode(copyText("plan.check.unparsed", { list: slice.check.unparsed.join(copyText("plan.page.listSep")) })));
      parts.push(un);
    }
    parts.push(spacer(), this.filterChips(slice));
    this.subEl.replaceChildren(...parts);
  }

  /** 「只写文档」：pb 只管图、仓库不守，接过来之前 agent 不写仓库。 */
  private docsOnlyBadge(slice: PlanSlice): HTMLElement {
    const b = phaseBadge(copyText("plan.sub.docsOnly"));
    b.dataset.badge = "docs-only";
    attachTooltip(b, () => copyText("plan.sub.docsOnlyHint", { path: slice.path ?? "" }));
    return b;
  }

  private filterChips(slice: PlanSlice): HTMLElement {
    const box = document.createElement("div");
    box.className = s.pvChips;
    const total = slice.cells.length;
    const items: { key: PlanFilter; label: string; n: number | null; tone?: string }[] = [
      { key: "all", label: copyText("plan.filter.all"), n: total },
      { key: "open", label: copyText("plan.filter.open"), n: slice.openCells },
    ];
    items.push({ key: "needs", label: copyText("plan.filter.needs"), n: slice.needCount, tone: "needs" });
    for (const it of items) {
      const b = document.createElement("button");
      b.type = "button";
      b.className = `${s.pvChip} plan-filter`;
      b.setAttribute("aria-pressed", String(this.filter === it.key));
      if (it.tone) b.dataset.tone = it.tone;
      b.append(document.createTextNode(it.label));
      if (it.n !== null) {
        const n = document.createElement("span");
        n.className = s.pvNum;
        n.textContent = String(it.n);
        b.appendChild(n);
      }
      b.addEventListener("click", () => {
        this.filter = it.key;
        this.render();
      });
      box.appendChild(b);
    }
    return box;
  }

  /** 顶上那几条：那台离线 · 这一片读不成 · 整次读不成。 */
  private renderStrips(): void {
    const out: HTMLElement[] = [];
    const cur = this.current;
    const m = cur ? this.per.get(cur.origin) : undefined;
    if (cur && m?.state === "failed" && m.offline && m.prev) {
      const re = button({ label: copyText("plan.strip.reconnect"), size: "compact", onClick: () => this.reconnect(cur.origin) });
      out.push(banner("warn", copyText("plan.strip.offline", { machine: cur.origin, seen: this.currentDoc()?.seen?.said ?? copyText("plan.page.none") }), [re]));
    }
    const doc = this.currentDoc();
    const slice = this.currentSlice();
    if (doc?.stale) out.push(banner("error", joinSep(copyText("plan.strip.unreadable", { reason: doc.stale.said ?? "" }), copyText("plan.strip.stale", { time: doc.stale.sinceText ?? "" }))));
    else if (slice?.stale) out.push(banner("error", joinSep(copyText("plan.strip.sliceUnreadable", { reason: slice.stale.said ?? "" }), copyText("plan.strip.stale", { time: slice.stale.sinceText ?? "" }))));
    // pb 读不成的几处（哪一份 · 第几行 · pb 那一句）：那一片照读出来的画，这几处照实列在顶上。
    for (const e of slice?.check.unreadable ?? []) out.push(banner("warn", joinSep(copyText("plan.check.unreadable", { file: e.line === null ? (e.file ?? "") : `${e.file ?? ""}:${e.line}` }), e.message ?? "")));
    this.stripsEl.replaceChildren(...out);
  }

  private renderBody(): void {
    const whole = this.wholeState();
    if (whole) {
      this.bodyEl.className = s.pvWhole;
      morph(this.bodyEl, [whole]);
      return;
    }
    this.bodyEl.className = s.pvBody;
    const slice = this.currentSlice();
    const stale = !!this.currentDoc()?.stale || !!slice?.stale || this.per.get(this.current!.origin)?.state === "failed";
    this.bodyEl.dataset.stale = String(stale);
    const tree = document.createElement("div");
    tree.className = s.pvTree;
    const pane = document.createElement("div");
    pane.className = `${s.pvPane} plan-pane`;
    if (!slice) {
      tree.appendChild(skeletonRows(8));
      morph(this.bodyEl, [tree, pane]);
      return;
    }
    if (slice.bare) {
      pane.appendChild(emptyState({ icon: "warning", text: copyText("plan.strip.sliceUnreadable", { reason: slice.error ?? "" }) }));
      morph(this.bodyEl, [tree, pane]);
      return;
    }
    // 新登记的仓库还没有图：整页一屏（没有大纲可画）＋ project 的交接说明（后端判了给接手才给：`open` 那一句 · `no_line` 为什么）。
    if (isFresh(slice)) {
      const ref = this.current!;
      this.bodyEl.className = s.pvWhole;
      const top = topBlock(slice);
      const hand = top && (top.takeover === "open" || top.takeover === "no_line") ? handoffBox(top) : undefined;
      morph(this.bodyEl, [freshPane(slice, ref.workspace, (cwd) => void openNewSession({ origin: ref.origin, cwd }), hand)]);
      return;
    }
    tree.append(this.outline(slice), this.legend(slice));
    const cell = this.selected ? cellIndex(slice).get(this.selected) : undefined;
    if (!cell && this.selected) this.selected = null;
    if (cell) pane.append(...this.reviewBars(slice, cell), this.detail.render(slice, cell, this.currentDoc()?.rev ?? ""));
    else pane.append(...this.reviewBars(slice, cell), this.overview(slice));
    morph(this.bodyEl, [tree, pane]);
  }

  /** 整页一句的几种：首次加载 · pb 没装 / 太旧 · 没有计划。`null` ＝ 有片可画。 */
  private wholeState(): HTMLElement | null {
    const any = this.allSlices().length > 0;
    if (any && this.current) return null;
    const states = this.machines.map((m) => this.per.get(m));
    if (states.some((x) => x === undefined || x.state === "loading") && !any) {
      const box = document.createElement("div");
      box.appendChild(skeletonRows(6));
      return box;
    }
    const local = this.per.get(LOCAL_ORIGIN);
    if (local?.state === "ok" && local.list.pb.state !== "ok") {
      const [head, ...rest] = (local.list.pb.said ?? "").split("\n");
      // 找不到 pb ⇒ 去本机设置里填 pb 路径那一行。
      const set: HTMLButtonElement | undefined =
        local.list.pb.state === "missing"
          ? button({ label: copyText("plan.empty.setPb"), kind: "ghost", onClick: () => void openSettingsWindow(set, pbDirOf(LOCAL_ORIGIN)) })
          : undefined;
      return emptyState({ icon: "warning", text: head, hint: rest.join(copyText("kit.text.sep")) || undefined, action: set });
    }
    const add = button({ label: copyText("plan.empty.add"), kind: "ghost", onClick: () => void this.addWorkspace() });
    return emptyState({ icon: "plan", text: copyText("plan.empty.text"), hint: copyText("plan.empty.hint"), action: add });
  }

  private async addWorkspace(): Promise<void> {
    const dir = await askText({ title: copyText("plan.addWs.title"), action: copyText("plan.addWs.ok"), label: copyText("plan.addWs.label") });
    const d = dir?.trim();
    if (!d) return;
    const list = (this.prefs.dirs[""] ??= []);
    if (!list.includes(d)) list.push(d);
    this.savePrefs();
    this.refresh(true, LOCAL_ORIGIN);
  }

  // ───────────────────────── 大纲 ─────────────────────────

  private outline(slice: PlanSlice): HTMLElement {
    const box = document.createElement("div");
    box.className = s.pvRows;
    box.setAttribute("role", "tree");
    box.setAttribute("aria-label", copyText("plan.outline.label"));
    // 顶上那一行：这一片。
    const head = document.createElement("div");
    head.className = `${s.pvRow} ${s.pvRowHead} plan-row`;
    head.setAttribute("aria-selected", String(this.selected === null));
    head.style.paddingLeft = "16px";
    const hd = document.createElement("span");
    hd.className = s.pvTt;
    hd.textContent = slice.name;
    const trail = document.createElement("span");
    trail.className = s.pvTrail;
    if (slice.progress) trail.textContent = copyText("plan.outline.rootProgress", { done: slice.progress.done, of: slice.progress.of });
    head.append(icon("plan", "compact"), hd, trail);
    head.addEventListener("click", () => this.select(null));
    box.appendChild(head);
    const rows = outlineRows(slice, {
      filter: this.filter,
      folded: this.folded,
      query: this.query,
      needsCells: new Set((slice.needs ?? []).map((n) => n.cell).filter((x): x is string => x !== null)),
    });
    const lanes = ringLanes(
      slice,
      rows.map((r) => r.cell.id),
      new Set(rows.filter((r) => r.folded).map((r) => r.cell.id)),
      this.selected,
    );
    rows.forEach((r, i) => box.appendChild(this.row(slice, r, lanes[i])));
    return box;
  }

  private row(slice: PlanSlice, r: ReturnType<typeof outlineRows>[number], rings: RingRow): HTMLElement {
    const c = r.cell;
    const look = statusLook(c);
    const el = document.createElement("div");
    el.className = `${s.pvRow} plan-row`;
    el.setAttribute("role", "treeitem");
    el.dataset.look = look;
    el.dataset.cell = c.id;
    if (r.block) el.dataset.block = "true";
    el.setAttribute("aria-selected", String(this.selected === c.id));
    if (r.hasKids) el.setAttribute("aria-expanded", String(!r.folded));
    el.style.paddingLeft = `${16 + r.depth * 18}px`;
    setKindColor(el, kindSlot(slice, c.kind));
    // covers 画圈：最左一条槽，一条规矩固定一列（同一条规矩同一色），一段起头带上钩、末行带下钩，停在上面说是哪一条；
    // 选中了画圈的规矩 ⇒ 它那几道亮、别的淡，圈住的行铺它的色。
    if (rings.marks.length > 0) {
      const slot = document.createElement("span");
      slot.className = s.pvRings;
      for (const g of rings.marks) {
        const b = document.createElement("span");
        b.className = s.pvRing;
        setKindColor(b, g.slot);
        b.style.setProperty("--col", String(g.col));
        if (g.start) b.dataset.start = "true";
        if (g.end) b.dataset.end = "true";
        if (g.look) b.dataset.look = g.look;
        b.title = labelOf(slice, g.rule);
        slot.appendChild(b);
      }
      el.appendChild(slot);
    }
    if (rings.tint !== null) {
      el.dataset.ringed = "true";
      el.style.setProperty("--rc", `var(--plan-kind-${rings.tint})`);
    }
    const chev = document.createElement("span");
    chev.className = s.pvChev;
    if (r.hasKids) {
      chev.setAttribute("aria-expanded", String(!r.folded));
      chev.appendChild(foldCaret());
      chev.addEventListener("click", (ev) => {
        ev.stopPropagation();
        if (this.folded.has(c.id)) this.folded.delete(c.id);
        else this.folded.add(c.id);
        this.renderBody();
      });
    }
    const st = document.createElement("span");
    st.className = s.pvSt;
    st.dataset.look = look;
    st.appendChild(icon(STATUS_ICON[look], "compact"));
    st.setAttribute("aria-label", c.stateLine ?? c.whyShort?.text ?? "");
    const bar = document.createElement("span");
    bar.className = s.pvKbar;
    const tt = document.createElement("span");
    tt.className = s.pvTt;
    tt.textContent = labelOf(slice, c.id);
    const kw = document.createElement("span");
    kw.className = s.pvKw;
    kw.textContent = c.kind ?? "";
    const trail = document.createElement("span");
    trail.className = s.pvTrail;
    // 没签收 · 待定没人定 ⇒ 后端写好的那一个短词；签收了 ⇒ 不写（状态图标已说）。
    const w = c.whyShort;
    if (w) {
      const why = document.createElement("span");
      why.className = s.pvWhy;
      if (w.wait) why.dataset.wait = "true";
      if (c.undecided) why.dataset.undecided = "true";
      why.textContent = w.text;
      trail.appendChild(why);
    }
    if ((slice.needs ?? []).some((n) => n.cell === c.id)) {
      const d = document.createElement("span");
      d.className = s.pvNdot;
      trail.appendChild(d);
    }
    if (r.block) {
      if (r.block.owner) trail.appendChild(this.whoChip(r.block.owner));
      if (r.block.phase) trail.appendChild(phaseBadge(r.block.phase));
    }
    if (r.standing) {
      const u = icon("person", "compact");
      trail.appendChild(u);
    }
    if (c.covers.includes("project")) trail.appendChild(text(copyText("plan.cell.coversAll"), s.pvKw));
    el.append(chev, st, bar, tt, kw, trail);
    el.addEventListener("click", () => this.select(c.id));
    return el;
  }

  private legend(slice: PlanSlice): HTMLElement {
    const box = document.createElement("div");
    box.className = s.pvLegend;
    slice.kinds.forEach((k, i) => {
      const it = document.createElement("span");
      it.className = s.pvLegendKind;
      setKindColor(it, i % 7);
      it.textContent = k.name ?? "";
      box.appendChild(it);
    });
    // 圈：画圈的每条规矩一项（同括弧一色），点了去那一格。
    const rules = ringRules(slice);
    if (rules.length > 0) {
      const rs = document.createElement("span");
      rs.dataset.legend = "rings";
      rs.appendChild(document.createTextNode(copyText("plan.outline.rings")));
      box.appendChild(rs);
      for (const g of rules) {
        const it = document.createElement("span");
        it.className = s.pvLegendKind;
        setKindColor(it, g.slot);
        const go = document.createElement("button");
        go.type = "button";
        go.className = s.pvLink;
        go.textContent = labelOf(slice, g.rule);
        go.addEventListener("click", () => this.select(g.rule));
        it.appendChild(go);
        box.appendChild(it);
      }
    }
    return box;
  }

  // ───────────────────────── 概览 ─────────────────────────

  private overview(slice: PlanSlice): HTMLElement {
    const box = document.createElement("div");
    const byId = cellIndex(slice);
    const h1 = document.createElement("div");
    h1.className = s.pvH1;
    const t = document.createElement("span");
    t.className = s.pvH1Text;
    t.textContent = slice.name;
    h1.appendChild(t);
    if (slice.domain) h1.appendChild(phaseBadge(slice.domain));
    if (slice.progress) {
      const p = slice.progress;
      h1.appendChild(text(copyText("plan.overview.top", { done: p.done, of: p.of }), s.pvMeta));
    }
    box.appendChild(h1);
    const doc = this.currentDoc();
    if (doc?.readAtText) box.appendChild(text(copyText("plan.overview.readAt", { time: doc.readAtText }), s.pvMeta));

    // 块
    // 末一格「接手」照后端那一格（建了的 `takeover` · 还没建的那一列）。
    const blocks = slice.blocks.filter((b) => b.id !== "project");
    const unbuilt = slice.unbuiltBlocks;
    if (blocks.length + unbuilt.length > 0) {
      const sec = section(copyText("plan.overview.blocks"), String(blocks.length + unbuilt.length));
      const tbl = table([copyText("plan.blocks.block"), copyText("plan.blocks.owner"), copyText("plan.blocks.phase"), copyText("plan.blocks.at"), copyText("plan.blocks.inside"), ""], [4, 5]);
      const none = (): HTMLElement => text(copyText("plan.page.none"));
      for (const b of blocks) {
        const root = byId.get(b.cells[0] ?? "");
        const at = b.at ? byId.get(b.at) : null;
        const tr = tbl.row([
          text(labelOf(slice, root?.id ?? b.id)),
          b.owner ? this.whoChip(b.owner) : none(),
          b.phase ? phaseBadge(b.phase) : none(),
          text(at ? labelOf(slice, at.id) : copyText("plan.page.none")),
          text(b.inside ? `${b.inside.done}/${b.inside.of}` : copyText("plan.page.none")),
          stop(takeoverCell(b, () => this.takeover(b))),
        ]);
        if (root) clickRow(tr, () => this.select(root.id));
      }
      for (const u of unbuilt) {
        const root = byId.get(u.cells[0] ?? "");
        const tr = tbl.row([text(labelOf(slice, root?.id ?? u.id ?? "")), none(), none(), none(), none(), stop(unbuiltTakeoverCell())]);
        if (root) clickRow(tr, () => this.select(root.id));
      }
      sec.appendChild(tbl.el);
      box.appendChild(sec);
    }

    // 顶层进度
    const sec2 = section(copyText("plan.overview.progress"));
    const tbl2 = table([copyText("plan.progress.cell"), copyText("plan.progress.status"), copyText("plan.progress.block")], []);
    const roots = blockRoots(slice);
    for (const id of slice.top) {
      const c = byId.get(id);
      if (!c) continue;
      const name = document.createElement("span");
      name.className = s.pvCellName;
      setKindColor(name, kindSlot(slice, c.kind));
      const bar = document.createElement("span");
      bar.className = s.pvKbar;
      name.append(bar, document.createTextNode(labelOf(slice, c.id)));
      const look = statusLook(c);
      const stw = document.createElement("span");
      stw.className = s.pvStw;
      stw.dataset.look = look;
      stw.append(icon(STATUS_ICON[look], "compact"), document.createTextNode(joinSep(c.stateLine ?? "", c.whyShort?.text ?? "")));
      const b = roots.get(c.id);
      const tr = tbl2.row([name, stw, b?.owner ? this.whoChip(b.owner) : text("")]);
      clickRow(tr, () => this.select(c.id));
    }
    sec2.appendChild(tbl2.el);
    box.appendChild(sec2);
    return box;
  }

  // ───────────────────────── 会话标签 ─────────────────────────

  /**
   * 负责人 / signer：会话 ⇒ 那个标签页的点 ＋ 标题 ＋ 机器；子 agent ⇒ 挂在父会话那个标签页上、写后端给的「{父会话} 的 agent」；
   * 认不出 ⇒ 虚线框「认不出 · 前后几位」，点了复制完整 id。
   */
  whoChip(w: PlanWho): HTMLElement {
    const b = document.createElement("button");
    b.type = "button";
    b.className = `${s.pvOwner} plan-who`;
    b.dataset.kind = w.kind;
    const txt = document.createElement("span");
    txt.className = s.pvOwnerText;
    if (w.kind === "unknown" || w.sid === null) {
      txt.textContent = copyText("plan.who.unknown", { id: shortId(w.id) });
      b.appendChild(txt);
      b.addEventListener("click", (ev) => {
        ev.stopPropagation();
        void writeClipboard(w.id).then(() => toast(copyText("plan.who.copied"), "", { level: "success" }), (e: unknown) => failToast(copyText("detail.act.failed"), e, { level: "error" }));
      });
      return b;
    }
    const tab = this.tabOf(w.sid);
    const face = tab ? tab.face : w.face;
    b.appendChild(statusDot(face?.dot ?? null, face?.name ?? null, "compact"));
    const name = tab ? titleParts(tab).title : shortId(w.sid);
    // 子 agent ⇒ 后端写好的「{父会话} 的 agent」照画；会话本身 ⇒ 标签页的名字。
    txt.textContent = w.label ?? name;
    b.appendChild(txt);
    if (tab) {
      const mt = document.createElement("span");
      mt.className = s.pvOwnerMt;
      mt.textContent = machineName(tab.origin);
      b.appendChild(mt);
    }
    const sid = w.sid;
    b.addEventListener("click", (ev) => {
      ev.stopPropagation();
      if (this.tabOf(sid)) this.goSession(sid);
    });
    return b;
  }
}

// ───────────────────────── 小件 ─────────────────────────

function joinSep(...parts: string[]): string {
  return parts.join(copyText("kit.text.sep"));
}

function spacer(): HTMLElement {
  const e = document.createElement("span");
  e.className = s.pvSpacer;
  return e;
}

function sep(): HTMLElement {
  const e = document.createElement("span");
  e.className = s.pvSep;
  return e;
}

function text(t: string, cls?: string): HTMLElement {
  const e = document.createElement("span");
  if (cls) e.className = cls;
  e.textContent = t;
  return e;
}

function section(title: string, aside?: string): HTMLElement {
  const sec = document.createElement("div");
  sec.className = s.pvSec;
  const h = document.createElement("div");
  h.className = s.pvSecHead;
  h.append(document.createTextNode(title), spacer());
  if (aside) h.appendChild(text(aside));
  sec.appendChild(h);
  return sec;
}

function table(heads: string[], right: number[]): { el: HTMLTableElement; row(cells: HTMLElement[]): HTMLTableRowElement } {
  const el = document.createElement("table");
  el.className = s.pvTbl;
  const thead = el.createTHead().insertRow();
  heads.forEach((h, i) => {
    const th = document.createElement("th");
    th.textContent = h;
    if (right.includes(i)) th.className = s.pvR;
    thead.appendChild(th);
  });
  const body = el.createTBody();
  return {
    el,
    row(cells) {
      const tr = body.insertRow();
      cells.forEach((c, i) => {
        const td = tr.insertCell();
        if (right.includes(i)) td.className = s.pvR;
        td.appendChild(c);
      });
      return tr;
    },
  };
}

/** 行里那颗按钮的点击不连带选中那一行。 */
function stop(el: HTMLElement): HTMLElement {
  el.addEventListener("click", (ev) => ev.stopPropagation());
  return el;
}

function clickRow(tr: HTMLTableRowElement, go: () => void): void {
  tr.dataset.click = "true";
  tr.addEventListener("click", go);
}
